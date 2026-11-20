use super::*;
use orchestrator_ipc::{IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tempfile::TempDir;

const MAC: &str = "darwin-aarch64";
const WIN: &str = "windows-x86_64";

fn mac_name(version: &str) -> String {
    format!("MCAO-{version}-macos-arm64-unsigned.app.tar.gz")
}

fn win_name(version: &str) -> String {
    format!("MCAO-{version}-windows-x64-unsigned-setup.exe")
}

fn asset(id: u64, name: &str, state: &str, size: u64) -> Value {
    json!({"id": id, "name": name, "state": state, "size": size})
}

/// Manifest (id 1), macOS archive (id 2, 100 bytes) and Windows installer (id 3, 200 bytes).
fn assets(version: &str) -> Vec<Value> {
    vec![
        asset(1, "latest.json", "uploaded", 500),
        asset(2, &mac_name(version), "uploaded", 100),
        asset(3, &win_name(version), "uploaded", 200),
    ]
}

fn release(tag: &str, assets: Vec<Value>) -> Value {
    json!({"tag_name": tag, "draft": false, "published_at": "2026-11-19T00:00:00Z", "prerelease": true, "assets": assets})
}

fn manifest(version: &str) -> Value {
    let url = |name: String| {
        format!("https://github.com/mc-innersource/motif-mcao/releases/download/v{version}/{name}")
    };
    json!({
        "version": version,
        "platforms": {
            MAC: {"url": url(mac_name(version)), "signature": "c2lnLW1hYw=="},
            WIN: {"url": url(win_name(version)), "signature": "c2lnLXdpbg=="},
        }
    })
}

/// Replays canned stdout and remembers the arguments of every call.
struct Fake {
    replies: Mutex<VecDeque<Result<Vec<u8>, RunError>>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl Fake {
    fn new(replies: Vec<Result<Vec<u8>, RunError>>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// The list of releases (one page per inner vec) and then the manifest bytes.
    fn listing(pages: &[Vec<Value>], manifest: &Value) -> Self {
        Self::new(vec![
            Ok(serde_json::to_vec(pages).unwrap()),
            Ok(serde_json::to_vec(manifest).unwrap()),
        ])
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Gh for Fake {
    async fn run(&self, args: &[&str], _: u64, _: Duration) -> Result<Vec<u8>, RunError> {
        self.calls
            .lock()
            .unwrap()
            .push(args.iter().map(|arg| (*arg).to_owned()).collect());
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(RunError::Failed))
    }
}

fn run<T>(future: impl Future<Output = T>) -> T {
    tauri::async_runtime::block_on(future)
}

fn installed() -> Version {
    Version::new(0, 1, 28)
}

fn resolve_with(fake: &Fake, target: &str) -> Result<Option<Resolved>, UpdateError> {
    run(resolve(fake, &installed(), target))
}

fn offered(pages: &[Vec<Value>], manifest: &Value) -> Result<Option<Resolved>, UpdateError> {
    resolve_with(&Fake::listing(pages, manifest), MAC)
}

#[test]
fn selects_the_highest_eligible_version_whatever_the_order_or_paging() {
    let noise = vec![
        release("v0.1.27", assets("0.1.27")),
        release("v0.1.28", assets("0.1.28")),
        release("v0.1.29", assets("0.1.29")),
        release("v0.1.30", assets("0.1.30")),
        // A higher draft, an unpublished entry, a manual-only tag and tags that are not bare SemVer.
        json!({"tag_name": "v0.1.40", "draft": true, "published_at": null, "assets": assets("0.1.40")}),
        json!({"tag_name": "v0.1.50", "draft": false, "published_at": null, "assets": assets("0.1.50")}),
        release("v0.1.60", vec![asset(9, "SHA256SUMS", "uploaded", 10)]),
        release("v0.1.70", vec![asset(1, "latest.json", "starter", 5)]),
        release("0.1.80", assets("0.1.80")),
        release("v0.1.90-rc.1", assets("0.1.90-rc.1")),
        release("v0.1.95.1", assets("0.1.95.1")),
        release("v1.2", assets("1.2")),
    ];
    for rotation in 0..noise.len() {
        for split in [0, 1, 5, noise.len()] {
            let mut flat = noise.clone();
            flat.rotate_left(rotation);
            let pages = vec![flat[..split].to_vec(), flat[split..].to_vec()];
            let resolved = offered(&pages, &manifest("0.1.30")).unwrap().unwrap();
            assert_eq!(resolved.version, Version::new(0, 1, 30));
        }
    }
}

#[test]
fn a_prerelease_is_offered_and_its_payload_is_the_targets_asset() {
    let pages = vec![vec![release("v0.1.29", assets("0.1.29"))]];
    let mac = resolve_with(&Fake::listing(&pages, &manifest("0.1.29")), MAC)
        .unwrap()
        .unwrap();
    assert_eq!((mac.payload_id, mac.payload_size), (2, 100));
    assert_eq!(mac.manifest, manifest("0.1.29"));
    let win = resolve_with(&Fake::listing(&pages, &manifest("0.1.29")), WIN)
        .unwrap()
        .unwrap();
    assert_eq!((win.payload_id, win.payload_size), (3, 200));
}

#[test]
fn gh_is_called_with_literal_arguments_for_the_list_and_the_asset() {
    let fake = Fake::listing(
        &[vec![release("v0.1.29", assets("0.1.29"))]],
        &manifest("0.1.29"),
    );
    resolve_with(&fake, MAC).unwrap();
    assert_eq!(
        fake.calls(),
        vec![
            vec![
                "api",
                "--hostname",
                "github.com",
                "--paginate",
                "--slurp",
                "repos/mc-innersource/motif-mcao/releases?per_page=100"
            ],
            vec![
                "api",
                "--hostname",
                "github.com",
                "-H",
                "Accept: application/octet-stream",
                "repos/mc-innersource/motif-mcao/releases/assets/1"
            ],
        ]
    );
}

#[test]
fn an_equal_or_older_release_is_current_without_fetching_a_manifest() {
    let pages = vec![vec![
        release("v0.1.28", assets("0.1.28")),
        release("v0.1.27", assets("0.1.27")),
    ]];
    let fake = Fake::listing(&pages, &manifest("0.1.28"));
    assert!(resolve_with(&fake, MAC).unwrap().is_none());
    assert_eq!(fake.calls().len(), 1);
    assert!(offered(&[], &manifest("0.1.28")).unwrap().is_none());
}

#[test]
fn a_duplicate_release_for_the_highest_version_is_invalid_not_a_tie_break() {
    let pages = vec![
        vec![release("v0.1.29", assets("0.1.29"))],
        vec![release("v0.1.29", assets("0.1.29"))],
    ];
    assert_eq!(
        offered(&pages, &manifest("0.1.29")).unwrap_err(),
        UpdateError::InvalidRelease
    );
}

#[test]
fn a_duplicate_of_a_lower_version_does_not_matter() {
    let pages = vec![vec![
        release("v0.1.29", assets("0.1.29")),
        release("v0.1.29", assets("0.1.29")),
        release("v0.1.30", assets("0.1.30")),
    ]];
    let resolved = offered(&pages, &manifest("0.1.30")).unwrap().unwrap();
    assert_eq!(resolved.version, Version::new(0, 1, 30));
}

fn invalid_assets(mutate: impl FnOnce(&mut Vec<Value>)) -> UpdateError {
    let mut list = assets("0.1.29");
    mutate(&mut list);
    offered(&[vec![release("v0.1.29", list)]], &manifest("0.1.29")).unwrap_err()
}

#[test]
fn bad_asset_sets_in_the_selected_release_are_invalid() {
    let duplicate_manifest =
        invalid_assets(|list| list.push(asset(7, "latest.json", "uploaded", 5)));
    let missing_windows = invalid_assets(|list| {
        list.pop();
    });
    let missing_mac = invalid_assets(|list| {
        list.remove(1);
    });
    let two_archives = invalid_assets(|list| {
        list.push(asset(
            8,
            "MCAO-0.1.29-macos-arm64-signed.app.tar.gz",
            "uploaded",
            5,
        ));
    });
    let two_installers = invalid_assets(|list| {
        list.push(asset(
            8,
            "MCAO-0.1.29-windows-x64-signed-setup.exe",
            "uploaded",
            5,
        ));
    });
    let unfinished = invalid_assets(|list| list[2]["state"] = json!("starter"));
    let wrong_version = invalid_assets(|list| list[1]["name"] = json!(mac_name("0.1.30")));
    let wrong_label = invalid_assets(|list| {
        list[1]["name"] = json!("MCAO-0.1.29-macos-arm64-beta.app.tar.gz");
    });
    let wrong_arch = invalid_assets(|list| {
        list[1]["name"] = json!("MCAO-0.1.29-macos-x64-unsigned.app.tar.gz");
    });
    for error in [
        duplicate_manifest,
        missing_windows,
        missing_mac,
        two_archives,
        two_installers,
        unfinished,
        wrong_version,
        wrong_label,
        wrong_arch,
    ] {
        assert_eq!(error, UpdateError::InvalidRelease);
    }
}

#[test]
fn payload_ids_and_sizes_outside_the_bounds_are_invalid() {
    for (field, value) in [
        ("id", json!(0)),
        ("size", json!(0)),
        ("size", json!(512 * 1024 * 1024 + 1)),
    ] {
        assert_eq!(
            invalid_assets(|list| list[1][field] = value),
            UpdateError::InvalidRelease,
            "{field}"
        );
    }
    let mut list = assets("0.1.29");
    list[1]["size"] = json!(512 * 1024 * 1024);
    let resolved = offered(&[vec![release("v0.1.29", list)]], &manifest("0.1.29"))
        .unwrap()
        .unwrap();
    assert_eq!(resolved.payload_size, 512 * 1024 * 1024);
}

fn invalid_manifest(manifest: Value) -> UpdateError {
    offered(&[vec![release("v0.1.29", assets("0.1.29"))]], &manifest).unwrap_err()
}

#[test]
fn a_manifest_that_does_not_match_its_release_is_invalid() {
    let mut cases = Vec::new();
    cases.push(manifest("0.1.30"));
    let mut extra = manifest("0.1.29");
    extra["platforms"]["linux-x86_64"] = extra["platforms"][MAC].clone();
    cases.push(extra);
    let mut missing = manifest("0.1.29");
    missing["platforms"].as_object_mut().unwrap().remove(WIN);
    cases.push(missing);
    let mut unsigned = manifest("0.1.29");
    unsigned["platforms"][MAC]["signature"] = json!("");
    cases.push(unsigned);
    let mut prefixed = manifest("0.1.29");
    prefixed["version"] = json!("v0.1.29");
    cases.push(prefixed);
    cases.push(json!({"version": "0.1.29"}));
    cases.push(json!([]));
    for case in cases {
        assert_eq!(
            invalid_manifest(case.clone()),
            UpdateError::InvalidRelease,
            "{case}"
        );
    }
    let fake = Fake::new(vec![
        Ok(serde_json::to_vec(&[vec![release("v0.1.29", assets("0.1.29"))]]).unwrap()),
        Ok(b"not json".to_vec()),
    ]);
    assert_eq!(
        resolve_with(&fake, MAC).unwrap_err(),
        UpdateError::InvalidRelease
    );
}

#[test]
fn cross_repo_or_non_canonical_urls_are_rejected() {
    let good = "https://github.com/mc-innersource/motif-mcao/releases/download/v0.1.29/";
    let mac = mac_name("0.1.29");
    for url in [
        format!("https://github.com/other-org/motif-mcao/releases/download/v0.1.29/{mac}"),
        format!("https://github.com/mc-innersource/other/releases/download/v0.1.29/{mac}"),
        format!("https://example.com/mc-innersource/motif-mcao/releases/download/v0.1.29/{mac}"),
        format!("http://github.com/mc-innersource/motif-mcao/releases/download/v0.1.29/{mac}"),
        format!("https://github.com/mc-innersource/motif-mcao/releases/download/v0.1.28/{mac}"),
        format!("https://github.com/mc-innersource/motif-mcao/releases/download/0.1.29/{mac}"),
        format!("{good}{}", win_name("0.1.29")),
        format!("{good}{mac}?token=1"),
        format!("{good}{mac}/"),
        format!("{good}%4dCAO-0.1.29-macos-arm64-unsigned.app.tar.gz"),
        format!("{good}{}", mac.replace("MCAO", "mcao")),
    ] {
        let mut broken = manifest("0.1.29");
        broken["platforms"][MAC]["url"] = json!(url);
        assert_eq!(
            invalid_manifest(broken),
            UpdateError::InvalidRelease,
            "{url}"
        );
    }
    // The other platform's URL is checked too, even though this build never installs it.
    let mut broken = manifest("0.1.29");
    broken["platforms"][WIN]["url"] = json!(format!("{good}other-setup.exe"));
    assert_eq!(invalid_manifest(broken), UpdateError::InvalidRelease);
}

#[test]
fn an_invalid_higher_release_does_not_fall_back_to_an_older_one() {
    let pages = vec![vec![
        release("v0.1.29", assets("0.1.29")),
        release("v0.1.30", assets("0.1.30")),
    ]];
    // The manifest served for v0.1.30 names the older version.
    assert_eq!(
        offered(&pages, &manifest("0.1.29")).unwrap_err(),
        UpdateError::InvalidRelease
    );
}

#[test]
fn a_missing_gh_is_classified_apart_from_every_other_failure() {
    let missing = Fake::new(vec![Err(RunError::Missing)]);
    assert_eq!(
        resolve_with(&missing, MAC).unwrap_err(),
        UpdateError::GhMissing
    );
    for failure in [RunError::Failed, RunError::Timeout, RunError::TooLarge] {
        let fake = Fake::new(vec![Err(failure)]);
        assert_eq!(
            resolve_with(&fake, MAC).unwrap_err(),
            UpdateError::GithubUnavailable
        );
    }
    assert_eq!(download_error(RunError::Missing), UpdateError::GhMissing);
    for failure in [RunError::Failed, RunError::Timeout, RunError::TooLarge] {
        assert_eq!(download_error(failure), UpdateError::DownloadFailed);
    }
}

#[test]
fn api_failures_while_discovering_are_github_unavailable() {
    let garbage = Fake::new(vec![Ok(b"<html>".to_vec())]);
    assert_eq!(
        resolve_with(&garbage, MAC).unwrap_err(),
        UpdateError::GithubUnavailable
    );
    let not_paged = Fake::new(vec![Ok(b"{\"message\":\"Not Found\"}".to_vec())]);
    assert_eq!(
        resolve_with(&not_paged, MAC).unwrap_err(),
        UpdateError::GithubUnavailable
    );

    let list = serde_json::to_vec(&[vec![release("v0.1.29", assets("0.1.29"))]]).unwrap();
    let manifest_fails = Fake::new(vec![Ok(list.clone()), Err(RunError::Timeout)]);
    assert_eq!(
        resolve_with(&manifest_fails, MAC).unwrap_err(),
        UpdateError::GithubUnavailable
    );
    let manifest_gh_gone = Fake::new(vec![Ok(list), Err(RunError::Missing)]);
    assert_eq!(
        resolve_with(&manifest_gh_gone, MAC).unwrap_err(),
        UpdateError::GhMissing
    );
}

#[test]
fn versions_must_be_bare_semver_exactly_as_written() {
    assert_eq!(bare_version("0.1.29"), Some(Version::new(0, 1, 29)));
    for text in [
        "v0.1.29",
        "0.1.29-rc.1",
        "0.1.29+build",
        "0.1",
        "01.2.3",
        " 0.1.29",
        "0.1.29 ",
        "",
    ] {
        assert_eq!(bare_version(text), None, "{text:?}");
    }
}

#[test]
fn results_and_errors_serialize_to_the_shapes_the_ui_reads() {
    let value = |result: &CheckResult| serde_json::to_value(result).unwrap();
    assert_eq!(value(&CheckResult::Disabled), json!({"kind": "disabled"}));
    assert_eq!(
        value(&CheckResult::Current {
            version: "0.1.28".into()
        }),
        json!({"kind": "current", "version": "0.1.28"})
    );
    assert_eq!(
        value(&CheckResult::Available {
            version: "0.1.29".into()
        }),
        json!({"kind": "available", "version": "0.1.29"})
    );
    for (error, kind) in [
        (UpdateError::GhMissing, "gh_missing"),
        (UpdateError::GithubUnavailable, "github_unavailable"),
        (UpdateError::InvalidRelease, "invalid_release"),
        (UpdateError::OfferChanged, "offer_changed"),
        (UpdateError::DownloadFailed, "download_failed"),
        (UpdateError::VerificationFailed, "verification_failed"),
        (UpdateError::DaemonBusy, "daemon_busy"),
        (UpdateError::DaemonUnavailable, "daemon_unavailable"),
        (UpdateError::InstallFailed, "install_failed"),
        (UpdateError::Busy, "busy"),
    ] {
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({ "kind": kind })
        );
    }
}

fn checked_in_config() -> tauri::Config {
    serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
}

#[test]
fn the_checked_in_updater_config_is_inert_until_a_key_is_merged_in() {
    let config = checked_in_config();
    let plugin: tauri_plugin_updater::Config =
        serde_json::from_value(config.plugins.0["updater"].clone()).unwrap();
    assert_eq!(plugin.pubkey, "");
    assert!(plugin.require_signed_version);
    assert!(!plugin.allow_downgrades);
    assert_eq!(
        plugin
            .endpoints
            .iter()
            .map(|url| url.as_str())
            .collect::<Vec<_>>(),
        ["https://api.github.com/repos/mc-innersource/motif-mcao/releases"]
    );
    assert_eq!(plugin.windows.unwrap().install_mode.nsis_args(), ["/P"]);
    assert!(!has_pubkey(&config));
    assert!(!enabled(&config));
}

#[test]
fn a_merged_public_key_is_what_turns_updates_on() {
    let mut merged: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    merged["plugins"]["updater"]["pubkey"] = json!("a2V5");
    let config: tauri::Config = serde_json::from_value(merged).unwrap();
    assert!(has_pubkey(&config));
    // Debug builds, which run these tests, never enable updates.
    assert!(!enabled(&config));
}

#[cfg(unix)]
fn script(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("gh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn the_subprocess_runner_sanitizes_the_environment_and_bounds_output_and_time() {
    let dir = TempDir::new().unwrap();
    std::env::set_var("GH_TOKEN", "leak");
    std::env::set_var("GITHUB_TOKEN", "leak");
    let env = script(
        dir.path(),
        r#"printf '%s|%s|%s|%s|%s|%s' "${GH_TOKEN-unset}" "${GITHUB_TOKEN-unset}" "$GH_PROMPT_DISABLED" "$GH_NO_UPDATE_NOTIFIER" "$GH_NO_EXTENSION_UPDATE_NOTIFIER" "$(pwd -P)""#,
    );
    let out = run(run_program(&env, &["api"], 1024, Duration::from_secs(10))).unwrap();
    let cwd = std::env::temp_dir().canonicalize().unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("unset|unset|1|1|1|{}", cwd.display())
    );

    let args = script(dir.path(), r#"printf '%s,' "$@""#);
    let out = run(run_program(
        &args,
        &["a b", "--x"],
        1024,
        Duration::from_secs(10),
    ))
    .unwrap();
    assert_eq!(out, b"a b,--x,");

    let big = script(dir.path(), "printf 123456789");
    assert_eq!(
        run(run_program(&big, &[], 8, Duration::from_secs(10))).unwrap_err(),
        RunError::TooLarge
    );
    assert_eq!(
        run(run_program(&big, &[], 9, Duration::from_secs(10))).unwrap(),
        b"123456789"
    );

    let fail = script(dir.path(), "printf partial; exit 1");
    assert_eq!(
        run(run_program(&fail, &[], 1024, Duration::from_secs(10))).unwrap_err(),
        RunError::Failed
    );

    let slow = script(dir.path(), "sleep 30");
    let started = Instant::now();
    assert_eq!(
        run(run_program(&slow, &[], 1024, Duration::from_millis(200))).unwrap_err(),
        RunError::Timeout
    );
    assert!(started.elapsed() < Duration::from_secs(10));

    assert_eq!(
        run(run_program(
            &dir.path().join("none"),
            &[],
            1,
            Duration::from_secs(1)
        ))
        .unwrap_err(),
        RunError::Missing
    );
}

#[test]
fn gh_is_found_in_absolute_path_entries_then_the_fallback_folders() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let fallback = TempDir::new().unwrap();
    let name = if cfg!(windows) { "gh.exe" } else { "gh" };
    std::fs::write(second.path().join(name), "").unwrap();
    std::fs::write(fallback.path().join(name), "").unwrap();
    let path = std::env::join_paths([first.path(), second.path()]).unwrap();
    let fallback_dirs = [fallback.path().to_path_buf()];
    assert_eq!(
        find_gh(Some(path), &fallback_dirs),
        Some(second.path().join(name))
    );
    assert_eq!(
        find_gh(None, &fallback_dirs),
        Some(fallback.path().join(name))
    );
    // A relative entry would resolve against the working tree.
    let relative = std::env::join_paths(["bin", ""]).unwrap();
    assert_eq!(find_gh(Some(relative), &[]), None);
    assert_eq!(find_gh(None, &[]), None);
}

struct Idle {
    keep_alive: bool,
}

impl RpcHandler for Idle {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Health]
    }

    fn dispatch(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        Err(RpcError::InvalidParams)
    }

    fn events_after(&self, _: i64, _: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }

    fn keep_alive(&self) -> bool {
        self.keep_alive
    }
}

/// A "daemon" that holds the data-directory lock for as long as it serves.
fn daemon(dir: &Path, keep_alive: bool) -> JoinHandle<()> {
    let served = dir.to_path_buf();
    let (ready, started) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let _lock = DataDirectoryLock::acquire(&served).unwrap();
        ready.send(()).unwrap();
        Server::serve_with_handler(
            ServerConfig::new(served, "updates-test").with_stream_timeout(Duration::from_secs(1)),
            Arc::new(Idle { keep_alive }),
        )
        .unwrap();
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Client::connect_with_timeout(dir, &[Capability::Health], CONNECT_TIMEOUT).is_err() {
        assert!(Instant::now() < deadline, "daemon not ready");
        thread::sleep(Duration::from_millis(20));
    }
    handle
}

#[test]
fn a_free_data_directory_is_locked_without_contacting_a_daemon() {
    let dir = TempDir::new().unwrap();
    assert!(acquire_idle_daemon_lock(dir.path()).is_ok());
}

#[test]
fn an_idle_daemon_is_stopped_and_its_lock_taken() {
    let dir = TempDir::new().unwrap();
    let daemon = daemon(dir.path(), false);
    let lock = acquire_idle_daemon_lock(dir.path()).unwrap();
    daemon.join().unwrap();
    drop(lock);
}

#[test]
fn a_busy_daemon_is_left_running_and_reported_busy() {
    let dir = TempDir::new().unwrap();
    let daemon = daemon(dir.path(), true);
    assert_eq!(
        acquire_idle_daemon_lock(dir.path()).err(),
        Some(UpdateError::DaemonBusy)
    );
    assert!(!daemon.is_finished());
    Client::connect_with_timeout(dir.path(), &[Capability::Health], CONNECT_TIMEOUT)
        .unwrap()
        .shutdown()
        .unwrap();
    daemon.join().unwrap();
}

#[test]
fn a_lock_holder_that_cannot_be_asked_to_stop_is_daemon_unavailable() {
    let dir = TempDir::new().unwrap();
    let _held = DataDirectoryLock::acquire(dir.path()).unwrap();
    assert_eq!(
        acquire_idle_daemon_lock(dir.path()).err(),
        Some(UpdateError::DaemonUnavailable)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn only_an_installed_mcao_app_bundle_is_replaceable() {
    assert_eq!(
        macos_bundle(Path::new("/Applications/MCAO.app/Contents/MacOS/MCAO")),
        Some(Path::new("/Applications/MCAO.app"))
    );
    for exe in [
        "/Applications/Other.app/Contents/MacOS/MCAO",
        "/Applications/MCAO.app/Contents/MacOS/other",
        "/work/target/debug/MCAO",
        "/MCAO",
    ] {
        assert_eq!(macos_bundle(Path::new(exe)), None, "{exe}");
    }
    let dir = TempDir::new().unwrap();
    assert!(effective_write_access(dir.path()));
    assert!(!effective_write_access(Path::new("/System/Library")));
}
