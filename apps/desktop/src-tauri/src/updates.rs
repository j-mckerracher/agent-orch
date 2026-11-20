//! In-app updates from GitHub Releases through the user's own `gh` login.
//!
//! The webview never sees a URL, a token or a path: it asks whether an update
//! exists and, if so, to install one version. Discovery and download both go
//! through `gh api`, so authentication stays in GitHub CLI and Tauri's own
//! network check is never used; `Updater::restore_update` turns the manifest
//! `gh` fetched into the plugin's `Update`, which then verifies the bytes
//! against the public key baked into this build.
//!
//! Installing replaces the app, so the daemon must stop first, but only if it
//! is idle: the install takes the data-directory lock (asking an idle daemon to
//! stop when it holds it), keeps the bridge from restarting it, and releases
//! everything again on any failure.

use crate::bridge::{Bridge, InstallGuard};
use crate::launch;
use orchestrator_ipc::{Capability, Client};
use orchestrator_private_fs::{DataDirectoryLock, DataDirectoryLockError};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::io::AsyncReadExt;

const HOST: &str = "github.com";
const REPO: &str = "mc-innersource/motif-mcao";
const MANIFEST_NAME: &str = "latest.json";
const LIST_MAX: u64 = 8 * 1024 * 1024;
const MANIFEST_MAX: u64 = 64 * 1024;
const PAYLOAD_MAX: u64 = 512 * 1024 * 1024;
const API_DEADLINE: Duration = Duration::from_secs(30);
const PAYLOAD_DEADLINE: Duration = Duration::from_secs(600);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_WAIT: Duration = Duration::from_secs(10);
const LOCK_RETRY: Duration = Duration::from_millis(100);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The updater target of this build; `None` where no update is shipped.
const TARGET: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("darwin-aarch64")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("windows-x86_64")
} else {
    None
};

/// The two payloads every release carries, as the release scripts name them:
/// `MCAO-<version>-<os_arch>-<signed|unsigned><suffix-tail>`.
struct Payload {
    key: &'static str,
    os_arch: &'static str,
    suffix: &'static str,
}

const PAYLOADS: [Payload; 2] = [
    Payload {
        key: "darwin-aarch64",
        os_arch: "macos-arm64",
        suffix: ".app.tar.gz",
    },
    Payload {
        key: "windows-x86_64",
        os_arch: "windows-x64",
        suffix: "-setup.exe",
    },
];

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckResult {
    Disabled,
    Current { version: String },
    Available { version: String },
}

/// Serializes as `{"kind": "<code>"}`; the codes are the UI's contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateError {
    GhMissing,
    GithubUnavailable,
    InvalidRelease,
    OfferChanged,
    DownloadFailed,
    VerificationFailed,
    DaemonBusy,
    DaemonUnavailable,
    InstallFailed,
    Busy,
}

/// Serializes checks and installs: an overlapping call answers `busy`.
#[derive(Default)]
pub struct Operation(tauri::async_runtime::Mutex<()>);

// ---- gh subprocess ------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum RunError {
    Missing,
    Failed,
    Timeout,
    TooLarge,
}

/// Runs `gh` and returns its stdout. The seam tests replace.
trait Gh: Sync {
    fn run(
        &self,
        args: &[&str],
        max: u64,
        deadline: Duration,
    ) -> impl Future<Output = Result<Vec<u8>, RunError>> + Send;
}

struct GhCli;

impl Gh for GhCli {
    async fn run(&self, args: &[&str], max: u64, deadline: Duration) -> Result<Vec<u8>, RunError> {
        let program =
            find_gh(std::env::var_os("PATH"), &fallback_dirs()).ok_or(RunError::Missing)?;
        run_program(&program, args, max, deadline).await
    }
}

async fn run_program(
    program: &Path,
    args: &[&str],
    max: u64,
    deadline: Duration,
) -> Result<Vec<u8>, RunError> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1");
    // Without these `gh` would use a token from the environment instead of the stored account.
    for variable in [
        "GH_DEBUG",
        "GH_FORCE_TTY",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
    ] {
        command.env_remove(variable);
    }
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RunError::Missing
        } else {
            RunError::Failed
        }
    })?;
    let mut stdout = child.stdout.take().ok_or(RunError::Failed)?;
    let read = async {
        let mut output = Vec::new();
        (&mut stdout)
            .take(max + 1)
            .read_to_end(&mut output)
            .await
            .map_err(|_| RunError::Failed)?;
        if output.len() as u64 > max {
            return Err(RunError::TooLarge);
        }
        let status = child.wait().await.map_err(|_| RunError::Failed)?;
        status.success().then_some(output).ok_or(RunError::Failed)
    };
    // Dropping the child on timeout or overflow kills it.
    tokio::time::timeout(deadline, read)
        .await
        .map_err(|_| RunError::Timeout)?
}

/// First `gh` in the absolute `PATH` entries, then the usual install folders:
/// a Finder or Start-menu launch has a minimal `PATH`.
fn find_gh(path: Option<OsString>, fallback: &[PathBuf]) -> Option<PathBuf> {
    let name = if cfg!(windows) { "gh.exe" } else { "gh" };
    std::env::split_paths(&path.unwrap_or_default())
        .chain(fallback.iter().cloned())
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn fallback_dirs() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        return vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into()];
    }
    let mut dirs = Vec::new();
    if cfg!(windows) {
        if let Some(files) = std::env::var_os("ProgramFiles") {
            dirs.push(PathBuf::from(files).join("GitHub CLI"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links"));
        }
    }
    dirs
}

fn check_error(error: RunError) -> UpdateError {
    match error {
        RunError::Missing => UpdateError::GhMissing,
        _ => UpdateError::GithubUnavailable,
    }
}

fn download_error(error: RunError) -> UpdateError {
    match error {
        RunError::Missing => UpdateError::GhMissing,
        _ => UpdateError::DownloadFailed,
    }
}

async fn fetch_asset<G: Gh>(
    gh: &G,
    id: u64,
    max: u64,
    deadline: Duration,
) -> Result<Vec<u8>, RunError> {
    let path = format!("repos/{REPO}/releases/assets/{id}");
    gh.run(
        &[
            "api",
            "--hostname",
            HOST,
            "-H",
            "Accept: application/octet-stream",
            &path,
        ],
        max,
        deadline,
    )
    .await
}

// ---- discovery ----------------------------------------------------------

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    published_at: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    id: u64,
    name: String,
    state: String,
    size: u64,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    platforms: BTreeMap<String, ManifestPlatform>,
}

#[derive(Deserialize)]
struct ManifestPlatform {
    url: String,
    signature: String,
}

/// An offer that passed every check; `manifest` is what `restore_update` takes.
#[derive(Debug)]
struct Resolved {
    version: Version,
    manifest: Value,
    payload_id: u64,
    payload_size: u64,
}

/// `MAJOR.MINOR.PATCH` exactly as written: no prerelease, build or padding.
fn bare_version(text: &str) -> Option<Version> {
    let version = Version::parse(text).ok()?;
    (version.pre.is_empty() && version.build.is_empty() && version.to_string() == text)
        .then_some(version)
}

fn is_manifest(asset: &Asset) -> bool {
    asset.name == MANIFEST_NAME && asset.state == "uploaded"
}

/// The highest published release above `installed` that carries a manifest.
/// Two such releases for one version are ambiguous, not a tie to break.
fn select<'a>(
    releases: &'a [Release],
    installed: &Version,
) -> Result<Option<(Version, &'a Release)>, UpdateError> {
    let eligible: Vec<(Version, &Release)> = releases
        .iter()
        .filter(|release| !release.draft && release.published_at.is_some())
        .filter(|release| release.assets.iter().any(is_manifest))
        .filter_map(|release| {
            Some((
                release.tag_name.strip_prefix('v').and_then(bare_version)?,
                release,
            ))
        })
        .filter(|(version, _)| version > installed)
        .collect();
    let Some(highest) = eligible.iter().map(|(version, _)| version).max() else {
        return Ok(None);
    };
    let mut top = eligible.iter().filter(|(version, _)| version == highest);
    match (top.next(), top.next()) {
        (Some((version, release)), None) => Ok(Some((version.clone(), release))),
        _ => Err(UpdateError::InvalidRelease),
    }
}

/// Checks the manifest of the selected release against its own asset list and
/// returns the payload asset for `target`.
fn validate(
    version: &Version,
    tag: &str,
    release: &Release,
    manifest: &[u8],
    target: &str,
) -> Result<Resolved, UpdateError> {
    let invalid = UpdateError::InvalidRelease;
    let raw: Value = serde_json::from_slice(manifest).map_err(|_| invalid)?;
    let parsed: Manifest = serde_json::from_value(raw.clone()).map_err(|_| invalid)?;
    if parsed.version != version.to_string() || parsed.platforms.len() != PAYLOADS.len() {
        return Err(invalid);
    }
    let mut chosen = None;
    for payload in &PAYLOADS {
        let mut named = release
            .assets
            .iter()
            .filter(|asset| asset.state == "uploaded" && asset.name.ends_with(payload.suffix));
        let (Some(asset), None) = (named.next(), named.next()) else {
            return Err(invalid);
        };
        let prefix = format!("MCAO-{version}-{}-", payload.os_arch);
        let label = asset
            .name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(payload.suffix));
        if !matches!(label, Some("signed" | "unsigned")) {
            return Err(invalid);
        }
        let entry = parsed.platforms.get(payload.key).ok_or(invalid)?;
        let url = format!(
            "https://{HOST}/{REPO}/releases/download/{tag}/{}",
            asset.name
        );
        if entry.url != url || entry.signature.is_empty() {
            return Err(invalid);
        }
        if payload.key == target {
            if asset.id == 0 || asset.size == 0 || asset.size > PAYLOAD_MAX {
                return Err(invalid);
            }
            chosen = Some((asset.id, asset.size));
        }
    }
    let (payload_id, payload_size) = chosen.ok_or(invalid)?;
    Ok(Resolved {
        version: version.clone(),
        manifest: raw,
        payload_id,
        payload_size,
    })
}

/// Lists releases and returns the validated offer above `installed`, if any.
async fn resolve<G: Gh>(
    gh: &G,
    installed: &Version,
    target: &str,
) -> Result<Option<Resolved>, UpdateError> {
    let list_path = format!("repos/{REPO}/releases?per_page=100");
    let list = gh
        .run(
            &[
                "api",
                "--hostname",
                HOST,
                "--paginate",
                "--slurp",
                &list_path,
            ],
            LIST_MAX,
            API_DEADLINE,
        )
        .await
        .map_err(check_error)?;
    let pages: Vec<Vec<Release>> =
        serde_json::from_slice(&list).map_err(|_| UpdateError::GithubUnavailable)?;
    let releases: Vec<Release> = pages.into_iter().flatten().collect();
    let Some((version, release)) = select(&releases, installed)? else {
        return Ok(None);
    };
    let mut manifests = release.assets.iter().filter(|asset| is_manifest(asset));
    let (Some(manifest), None) = (manifests.next(), manifests.next()) else {
        return Err(UpdateError::InvalidRelease);
    };
    if manifest.id == 0 {
        return Err(UpdateError::InvalidRelease);
    }
    let bytes = fetch_asset(gh, manifest.id, MANIFEST_MAX, API_DEADLINE)
        .await
        .map_err(check_error)?;
    validate(&version, &release.tag_name, release, &bytes, target).map(Some)
}

// ---- Tauri layer --------------------------------------------------------

/// Updates exist only in a release build of a shipped platform that was given
/// a public key at packaging time.
pub fn enabled(config: &tauri::Config) -> bool {
    !cfg!(debug_assertions) && TARGET.is_some() && has_pubkey(config)
}

fn has_pubkey(config: &tauri::Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(Value::as_str)
        .is_some_and(|key| !key.is_empty())
}

/// Resolves the current offer and restores it into the plugin's `Update`.
async fn offer<G: Gh>(
    app: &AppHandle,
    gh: &G,
    target: &str,
) -> Result<Option<(Resolved, Update)>, UpdateError> {
    let installed = app.package_info().version.clone();
    let Some(mut resolved) = resolve(gh, &installed, target).await? else {
        return Ok(None);
    };
    let updater = app.updater().map_err(|_| UpdateError::InstallFailed)?;
    let update = updater
        .restore_update(std::mem::take(&mut resolved.manifest))
        .ok()
        .flatten()
        .ok_or(UpdateError::InvalidRelease)?;
    Ok(Some((resolved, update)))
}

#[tauri::command]
pub async fn desktop_update_check(
    app: AppHandle,
    operation: State<'_, Operation>,
) -> Result<CheckResult, UpdateError> {
    let Some(target) = TARGET.filter(|_| enabled(app.config())) else {
        return Ok(CheckResult::Disabled);
    };
    let _busy = operation.0.try_lock().map_err(|_| UpdateError::Busy)?;
    Ok(match offer(&app, &GhCli, target).await? {
        Some((resolved, _)) => CheckResult::Available {
            version: resolved.version.to_string(),
        },
        None => CheckResult::Current {
            version: app.package_info().version.to_string(),
        },
    })
}

#[tauri::command]
pub async fn desktop_update_install(
    app: AppHandle,
    operation: State<'_, Operation>,
    bridge: State<'_, Bridge>,
    version: String,
) -> Result<(), UpdateError> {
    let Some(target) = TARGET.filter(|_| enabled(app.config())) else {
        return Err(UpdateError::InstallFailed);
    };
    let _busy = operation.0.try_lock().map_err(|_| UpdateError::Busy)?;
    let requested = bare_version(&version)
        .filter(|requested| *requested > app.package_info().version)
        .ok_or(UpdateError::OfferChanged)?;
    let (resolved, update) = match offer(&app, &GhCli, target).await? {
        Some(offer) if offer.0.version == requested => offer,
        _ => return Err(UpdateError::OfferChanged),
    };
    #[cfg(target_os = "macos")]
    bundle_is_writable()?;
    let bytes = fetch_asset(
        &GhCli,
        resolved.payload_id,
        resolved.payload_size,
        PAYLOAD_DEADLINE,
    )
    .await
    .map_err(download_error)?;
    if bytes.len() as u64 != resolved.payload_size {
        return Err(UpdateError::DownloadFailed);
    }
    update
        .verify(&bytes)
        .map_err(|_| UpdateError::VerificationFailed)?;
    let bridge = bridge.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        stop_daemon_and_install(&app, &bridge, update, &bytes)
    })
    .await
    .map_err(|_| UpdateError::InstallFailed)?
}

/// Field order is drop order: the bridge reopens before the lock is released.
struct Handoff {
    _bridge_closed: InstallGuard,
    lock: DataDirectoryLock,
}

fn stop_daemon_and_install(
    app: &AppHandle,
    bridge: &Bridge,
    update: Update,
    bytes: &[u8],
) -> Result<(), UpdateError> {
    let data_dir = launch::data_dir().ok_or(UpdateError::DaemonUnavailable)?;
    let guard = bridge.begin_install().map_err(|_| UpdateError::Busy)?;
    let handoff = Handoff {
        lock: acquire_idle_daemon_lock(&data_dir)?,
        _bridge_closed: guard,
    };
    // On Windows the installer takes over and the process exits inside `install`.
    update
        .restart_after_install(true)
        .install(bytes)
        .map_err(|_| UpdateError::InstallFailed)?;
    let Handoff {
        _bridge_closed: guard,
        lock,
    } = handoff;
    drop(lock);
    app.request_restart();
    guard.keep_closed();
    Ok(())
}

fn is_contention(error: &DataDirectoryLockError) -> bool {
    matches!(
        error,
        DataDirectoryLockError::AlreadyHeld { .. } | DataDirectoryLockError::LegacyLockHeld { .. }
    )
}

/// Takes the data-directory lock. A live holder is asked once to stop, and only
/// an idle daemon agrees; any other holder, or one that cannot be asked, fails closed.
fn acquire_idle_daemon_lock(data_dir: &Path) -> Result<DataDirectoryLock, UpdateError> {
    match DataDirectoryLock::acquire(data_dir) {
        Ok(lock) => return Ok(lock),
        Err(error) if is_contention(&error) => {}
        Err(_) => return Err(UpdateError::DaemonUnavailable),
    }
    let mut client = Client::connect_with_timeout(data_dir, &[Capability::Health], CONNECT_TIMEOUT)
        .map_err(|_| UpdateError::DaemonUnavailable)?;
    if !client
        .shutdown_if_idle()
        .map_err(|_| UpdateError::DaemonUnavailable)?
    {
        return Err(UpdateError::DaemonBusy);
    }
    drop(client);
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match DataDirectoryLock::acquire(data_dir) {
            Ok(lock) => return Ok(lock),
            Err(error) if is_contention(&error) && Instant::now() < deadline => {
                std::thread::sleep(LOCK_RETRY);
            }
            Err(_) => return Err(UpdateError::DaemonUnavailable),
        }
    }
}

/// Tauri swaps the bundle with a privileged helper when it cannot write to it;
/// refuse that path, and anything that is not an installed `MCAO.app`.
#[cfg(target_os = "macos")]
fn bundle_is_writable() -> Result<(), UpdateError> {
    let exe = std::env::current_exe().map_err(|_| UpdateError::InstallFailed)?;
    let bundle = macos_bundle(&exe).ok_or(UpdateError::InstallFailed)?;
    let parent = bundle.parent().ok_or(UpdateError::InstallFailed)?;
    if effective_write_access(bundle) && effective_write_access(parent) {
        Ok(())
    } else {
        Err(UpdateError::InstallFailed)
    }
}

#[cfg(target_os = "macos")]
fn macos_bundle(exe: &Path) -> Option<&Path> {
    exe.ends_with("MCAO.app/Contents/MacOS/MCAO")
        .then(|| exe.ancestors().nth(3))
        .flatten()
}

#[cfg(target_os = "macos")]
fn effective_write_access(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a valid NUL-terminated string that outlives the call.
    unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::W_OK, libc::AT_EACCESS) == 0 }
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
