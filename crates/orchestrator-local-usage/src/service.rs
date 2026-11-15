//! The collector: opt-in state, a background worker that runs the reader, and
//! the cache the RPCs read. A read never runs the reader. Opt-out kills a
//! running reader within one poll interval and drops the cache.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::capture::{self, Limits, RunError};
use crate::locate;
use crate::model::{
    Amount, DataWindow, ProviderState, ProviderUsage, ReaderState, ReaderStatus, Snapshot, Tokens,
    Window, PROVIDER_IDS, SCHEMA_VERSION,
};
use crate::parse::{self, format_nanos, Reading};
use crate::pin::{hash_file, parse_version, pin, Pin, SHEBANG_STRIPPED};

/// Largest stdout the reader may produce. A "today" report is a few KiB.
const MAX_STDOUT: u64 = 8 * 1024 * 1024;
const OPT_IN_FILE: &str = "usage-local-tools.json";
/// Reader cache, inside the daemon's private data directory.
const READER_CACHE: &str = "usage-local/cache";
const DAY_MS: i64 = 86_400_000;
const NODE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// How often and how long the collector runs. Production values are
/// [`Timing::PRODUCTION`]; tests use shorter ones.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Time between successful reads.
    pub interval: Duration,
    /// Time before retrying after a failed read.
    pub retry_gap: Duration,
    /// Minimum time between explicit refreshes.
    pub refresh_gap: Duration,
    /// Limit for one reader process.
    pub timeout: Duration,
}

impl Timing {
    pub const PRODUCTION: Timing = Timing {
        interval: Duration::from_secs(300),
        retry_gap: Duration::from_secs(60),
        refresh_gap: Duration::from_secs(30),
        timeout: Duration::from_secs(90),
    };
}

/// Where a reader was found. Only `Bundled` is used in release builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderOrigin {
    Bundled,
    /// Development builds: `AGENT_ORCH_CODEBURN_BIN`.
    Override,
    /// Development builds: `PATH` and the common global folders.
    Path,
}

impl ReaderOrigin {
    fn as_str(self) -> &'static str {
        match self {
            ReaderOrigin::Bundled => "bundled",
            ReaderOrigin::Override => "override",
            ReaderOrigin::Path => "path",
        }
    }
}

/// A reader that is ready to run: the program, the arguments that come before
/// the reader's own command (the path to `dist/cli.js`), and the versions found.
#[derive(Debug, Clone)]
pub struct ResolvedReader {
    pub program: PathBuf,
    pub prefix: Vec<OsString>,
    pub version: String,
    pub node_version: String,
    pub origin: ReaderOrigin,
}

/// Why a reader is not ready, with what was found.
#[derive(Debug, Clone)]
pub struct ReaderFault {
    pub state: ReaderState,
    pub version: Option<String>,
    pub node_version: Option<String>,
    pub exit_status: Option<i32>,
    pub origin: Option<ReaderOrigin>,
}

impl ReaderFault {
    fn new(state: ReaderState) -> Self {
        Self {
            state,
            version: None,
            node_version: None,
            exit_status: None,
            origin: None,
        }
    }

    fn status(&self) -> ReaderStatus {
        let mut status = ReaderStatus::new(self.state);
        status.version = self.version.clone();
        status.node_version = self.node_version.clone();
        status.exit_status = self.exit_status;
        status.source = self.origin.map(ReaderOrigin::as_str);
        status
    }
}

/// Finds and verifies the reader. Called before every collection, so a
/// changed or removed install is noticed without a restart.
pub trait ReaderSource: Send + Sync + 'static {
    fn resolve(&self) -> Result<ResolvedReader, ReaderFault>;

    /// True when the source may use development-only locations (debug builds).
    fn development_build(&self) -> bool {
        false
    }
}

/// Name of the bundled reader folder, next to the daemon executable.
/// Layout: `<dir>/node[.exe]` and `<dir>/pkg/{package.json, dist/cli.js}`.
pub const BUNDLE_DIR: &str = "codeburn";
const BUNDLE_PACKAGE: &str = "pkg";

/// Where the daemon looks for the reader, in order: the bundle, then (debug
/// builds only) `AGENT_ORCH_CODEBURN_BIN` and `PATH`. Release builds never
/// consult `PATH`: `path_dirs` and `override_command` are left empty there.
#[derive(Debug, Clone)]
pub struct ReaderLocations {
    /// Folder that holds the bundled reader, beside the daemon executable.
    pub bundle_dir: Option<PathBuf>,
    /// Second folder for the bundle, checked when `bundle_dir` is not complete.
    /// macOS app bundles keep resources in `Contents/Resources`, not beside the
    /// executable in `Contents/MacOS`, so the packaged reader lives there.
    pub bundle_fallback_dir: Option<PathBuf>,
    /// True in debug builds (`tauri dev`).
    pub development: bool,
    /// Development only: the codeburn command named by `AGENT_ORCH_CODEBURN_BIN`.
    pub override_command: Option<PathBuf>,
    /// Development only: the folders searched for `codeburn` and `node`.
    pub path_dirs: Vec<PathBuf>,
}

impl ReaderLocations {
    /// The daemon's real locations, found like the daemon executable itself
    /// (`launch.rs`): the bundle beside the executable, then the development options.
    pub fn for_daemon() -> Self {
        let development = cfg!(debug_assertions);
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let bundle_dir = exe_dir.as_ref().map(|dir| dir.join(BUNDLE_DIR));
        // On macOS the executable is `Contents/MacOS/...` and the resources are in
        // `Contents/Resources/...`. Windows and Linux keep them beside the executable.
        let bundle_fallback_dir = if cfg!(target_os = "macos") {
            exe_dir
                .as_ref()
                .and_then(|dir| dir.parent())
                .map(|contents| contents.join("Resources").join(BUNDLE_DIR))
        } else {
            None
        };
        let override_command = if development {
            std::env::var_os("AGENT_ORCH_CODEBURN_BIN")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        } else {
            None
        };
        let path_dirs = if development {
            locate::search_dirs(&locate::extra_dirs())
        } else {
            Vec::new()
        };
        Self {
            bundle_dir,
            bundle_fallback_dir,
            development,
            override_command,
            path_dirs,
        }
    }

    fn plan(&self) -> Plan {
        let bundles = [self.bundle_dir.as_ref(), self.bundle_fallback_dir.as_ref()];
        if let Some(dir) = bundles
            .into_iter()
            .flatten()
            .find(|dir| bundle_complete(dir))
        {
            return Plan::Bundle(dir.clone());
        }
        if !self.development {
            return Plan::Missing(ReaderState::NotBundled);
        }
        if let Some(command) = &self.override_command {
            return if command.is_file() {
                Plan::Command {
                    path: command.clone(),
                    origin: ReaderOrigin::Override,
                }
            } else {
                Plan::Missing(ReaderState::NotInstalled)
            };
        }
        match locate::find_program(locate::reader_names(), &self.path_dirs) {
            Some(path) => Plan::Command {
                path,
                origin: ReaderOrigin::Path,
            },
            None => Plan::Missing(ReaderState::NotInstalled),
        }
    }
}

fn bundle_complete(dir: &Path) -> bool {
    dir.join(BUNDLE_PACKAGE).join("package.json").is_file()
        && dir.join(locate::node_names()[0]).is_file()
}

#[derive(Debug, PartialEq, Eq)]
enum Plan {
    Bundle(PathBuf),
    Command { path: PathBuf, origin: ReaderOrigin },
    Missing(ReaderState),
}

/// The pinned codeburn reader: the bundle, or in development builds the
/// override or PATH. Every location goes through the same pin checks.
#[derive(Debug, Clone)]
pub struct CodeburnSource {
    locations: ReaderLocations,
}

impl CodeburnSource {
    pub fn new(locations: ReaderLocations) -> Self {
        Self { locations }
    }
}

impl Default for CodeburnSource {
    fn default() -> Self {
        Self::new(ReaderLocations::for_daemon())
    }
}

#[derive(Deserialize)]
struct Manifest {
    name: String,
    version: String,
}

impl ReaderSource for CodeburnSource {
    fn resolve(&self) -> Result<ResolvedReader, ReaderFault> {
        match self.locations.plan() {
            Plan::Missing(state) => Err(ReaderFault::new(state)),
            Plan::Bundle(dir) => {
                let root = dir.join(BUNDLE_PACKAGE);
                let entry = root.join("dist").join("cli.js");
                let node = dir.join(locate::node_names()[0]);
                resolve_installed(&root, entry, node, ReaderOrigin::Bundled, pin())
            }
            Plan::Command { path, origin } => {
                let fault = |state| ReaderFault {
                    origin: Some(origin),
                    ..ReaderFault::new(state)
                };
                let entry =
                    locate::reader_entry(&path).ok_or_else(|| fault(ReaderState::NotInstalled))?;
                let root = entry
                    .parent()
                    .and_then(Path::parent)
                    .ok_or_else(|| fault(ReaderState::NotInstalled))?
                    .to_path_buf();
                let node = locate::find_program(locate::node_names(), &self.locations.path_dirs)
                    .ok_or_else(|| fault(ReaderState::NodeMissing))?;
                resolve_installed(&root, entry, node, origin, pin())
            }
        }
    }

    fn development_build(&self) -> bool {
        self.locations.development
    }
}

/// Checks an installed package against the pin, then its Node.js, and returns
/// the ready reader. Used for the bundle and for every development location.
fn resolve_installed(
    root: &Path,
    entry: PathBuf,
    node: PathBuf,
    origin: ReaderOrigin,
    pin: &Pin,
) -> Result<ResolvedReader, ReaderFault> {
    let version = verify_installed(root, pin, origin)?;
    let fault = |state| ReaderFault {
        state,
        version: Some(version.clone()),
        node_version: None,
        exit_status: None,
        origin: Some(origin),
    };
    let node_version = node_version(&node).ok_or_else(|| fault(ReaderState::NodeMissing))?;
    let minimum = parse_version(&pin.node_minimum).unwrap_or((22, 15, 0));
    let found = parse_version(&node_version).unwrap_or((0, 0, 0));
    let display = node_version.trim_start_matches('v').to_owned();
    if found < minimum {
        return Err(ReaderFault {
            node_version: Some(display),
            ..fault(ReaderState::NodeTooOld)
        });
    }
    Ok(ResolvedReader {
        program: node,
        prefix: vec![entry.into_os_string()],
        version,
        node_version: display,
        origin,
    })
}

/// The package name, exact version and file hashes of an installed reader.
fn verify_installed(root: &Path, pin: &Pin, origin: ReaderOrigin) -> Result<String, ReaderFault> {
    let origin = Some(origin);
    let manifest: Manifest = fs::read(root.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or(ReaderFault {
            origin,
            ..ReaderFault::new(ReaderState::NotInstalled)
        })?;
    if manifest.name != pin.name {
        return Err(ReaderFault {
            origin,
            ..ReaderFault::new(ReaderState::NotInstalled)
        });
    }
    let installed = |state| ReaderFault {
        state,
        version: Some(manifest.version.clone()),
        node_version: None,
        exit_status: None,
        origin,
    };
    if manifest.version != pin.version {
        return Err(installed(ReaderState::UnsupportedVersion));
    }
    // The release bundle is checked against the bundled hashes; every development
    // location (an npm install found on PATH or by override) against the npm ones.
    let files = match origin {
        Some(ReaderOrigin::Bundled) => &pin.layouts.bundled,
        _ => &pin.layouts.npm,
    };
    for (relative, expected) in files {
        let strip = relative == SHEBANG_STRIPPED;
        let actual = hash_file(&root.join(relative), strip).ok();
        if actual.as_deref() != Some(expected.as_str()) {
            return Err(installed(ReaderState::IntegrityMismatch));
        }
    }
    Ok(manifest.version)
}

fn node_version(node: &Path) -> Option<String> {
    let env = locate::reader_env(&std::env::temp_dir());
    let limits = Limits {
        timeout: NODE_CHECK_TIMEOUT,
        max_stdout: 256,
    };
    let output = capture::run(node, &["--version".into()], &env, limits, &|| false).ok()?;
    if output.code != Some(0) {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let version = text.trim().to_owned();
    parse_version(&version).map(|_| version)
}

#[derive(Serialize, Deserialize)]
struct OptIn {
    schema_version: u32,
    enabled: bool,
}

#[derive(Default)]
struct Cache {
    data_window: Option<Window>,
    observed_at_ms: Option<i64>,
    reader: Option<ReaderStatus>,
    providers: Vec<ProviderUsage>,
}

struct State {
    enabled: bool,
    shutdown: bool,
    /// Bumped on every opt-in change; a run from an older generation is discarded.
    generation: u64,
    requested: Option<Window>,
    refresh_asked: bool,
    running: bool,
    worker_started: bool,
    last_attempt: Option<Instant>,
    next_due: Option<Instant>,
    cache: Cache,
}

struct Shared {
    development: bool,
    cache_dir: PathBuf,
    opt_in_path: PathBuf,
    timing: Timing,
    source: Box<dyn ReaderSource>,
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cancelled(&self, generation: u64) -> bool {
        let state = self.lock();
        state.shutdown || !state.enabled || state.generation != generation
    }
}

/// The local-usage collector. Create one per daemon; it starts no thread until
/// the user opts in.
pub struct LocalUsageService {
    shared: Arc<Shared>,
}

impl LocalUsageService {
    /// `data_dir` is the daemon's private data directory. The opt-in choice is
    /// read from `usage-local-tools.json` there; a missing or unreadable file
    /// means opted out.
    pub fn new(data_dir: &Path, source: Box<dyn ReaderSource>, timing: Timing) -> Self {
        let opt_in_path = data_dir.join(OPT_IN_FILE);
        let enabled = read_opt_in(&opt_in_path);
        let development = source.development_build();
        Self {
            shared: Arc::new(Shared {
                development,
                cache_dir: data_dir.join(READER_CACHE),
                opt_in_path,
                timing,
                source,
                state: Mutex::new(State {
                    enabled,
                    shutdown: false,
                    generation: 0,
                    requested: None,
                    refresh_asked: false,
                    running: false,
                    worker_started: false,
                    last_attempt: None,
                    next_due: None,
                    cache: Cache::default(),
                }),
                wake: Condvar::new(),
            }),
        }
    }

    /// The cached snapshot. Never runs the reader and never waits for it.
    pub fn read(&self, window: Window) -> Snapshot {
        let mut state = self.shared.lock();
        state.requested = Some(window.clone());
        if state.enabled {
            ensure_worker(&self.shared, &mut state);
            self.shared.wake.notify_all();
        }
        snapshot(
            &state,
            &self.shared.timing,
            &window,
            self.shared.development,
        )
    }

    /// Turns the opt-in on or off and persists the choice. Turning it off
    /// stops any running reader, drops the cache and starts no further reads.
    pub fn set_enabled(&self, enabled: bool, window: Window) -> io::Result<Snapshot> {
        let mut state = self.shared.lock();
        persist_opt_in(&self.shared.opt_in_path, enabled)?;
        state.enabled = enabled;
        state.generation += 1;
        state.requested = Some(window.clone());
        state.next_due = None;
        if enabled {
            state.refresh_asked = true;
            ensure_worker(&self.shared, &mut state);
        } else {
            state.refresh_asked = false;
            state.cache = Cache::default();
        }
        self.shared.wake.notify_all();
        Ok(snapshot(
            &state,
            &self.shared.timing,
            &window,
            self.shared.development,
        ))
    }

    /// Asks the worker to read now, at most once per `refresh_gap`.
    pub fn refresh(&self, window: Window) -> Snapshot {
        let mut state = self.shared.lock();
        state.requested = Some(window.clone());
        if state.enabled {
            ensure_worker(&self.shared, &mut state);
            let allowed = state
                .last_attempt
                .is_none_or(|at| at.elapsed() >= self.shared.timing.refresh_gap);
            if allowed && !state.running {
                state.refresh_asked = true;
                self.shared.wake.notify_all();
            }
        }
        snapshot(
            &state,
            &self.shared.timing,
            &window,
            self.shared.development,
        )
    }
}

impl Drop for LocalUsageService {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.shutdown = true;
        self.shared.wake.notify_all();
    }
}

fn ensure_worker(shared: &Arc<Shared>, state: &mut State) {
    if state.worker_started {
        return;
    }
    state.worker_started = true;
    let worker = Arc::clone(shared);
    let spawned = thread::Builder::new()
        .name("local-usage".into())
        .spawn(move || worker_loop(worker));
    if spawned.is_err() {
        // No thread means no reads; the snapshot shows the state, never a figure.
        state.worker_started = false;
    }
}

struct Job {
    window: Window,
    generation: u64,
}

enum Outcome {
    Figures {
        providers: Vec<ProviderUsage>,
        version: String,
        node_version: String,
        origin: ReaderOrigin,
    },
    Fault(ReaderFault),
    Cancelled,
}

fn worker_loop(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut state = shared.lock();
            let window = loop {
                if state.shutdown {
                    return;
                }
                if state.enabled {
                    if let Some(window) = state.requested.clone() {
                        if state.refresh_asked || is_due(&state, &shared.timing, &window) {
                            break window;
                        }
                    }
                }
                let (next, _) = shared
                    .wake
                    .wait_timeout(state, Duration::from_millis(500))
                    .unwrap_or_else(PoisonError::into_inner);
                state = next;
            };
            state.refresh_asked = false;
            state.running = true;
            state.last_attempt = Some(Instant::now());
            Job {
                window,
                generation: state.generation,
            }
        };

        let outcome = collect(&shared, &job);

        let mut state = shared.lock();
        state.running = false;
        if state.generation == job.generation && state.enabled {
            apply(&mut state, &shared.timing, &job.window, outcome);
        }
        shared.wake.notify_all();
    }
}

fn is_due(state: &State, timing: &Timing, window: &Window) -> bool {
    let now = Instant::now();
    if state.next_due.is_none_or(|due| now >= due) {
        return true;
    }
    // After midnight the cached figures are for yesterday: fetch today's, but
    // no faster than the retry gap.
    let window_changed = state
        .cache
        .data_window
        .as_ref()
        .is_none_or(|cached| cached != window);
    window_changed
        && state
            .last_attempt
            .is_none_or(|at| now.duration_since(at) >= timing.retry_gap)
}

fn collect(shared: &Shared, job: &Job) -> Outcome {
    let resolved = match shared.source.resolve() {
        Ok(resolved) => resolved,
        Err(fault) => return Outcome::Fault(fault),
    };
    let env = locate::reader_env(&shared.cache_dir);
    let limits = Limits {
        timeout: shared.timing.timeout,
        max_stdout: MAX_STDOUT,
    };
    let day = local_day(job.window.start_unix_ms, job.window.utc_offset_seconds);
    let fault = |state, exit_status| ReaderFault {
        state,
        version: Some(resolved.version.clone()),
        node_version: Some(resolved.node_version.clone()),
        exit_status,
        origin: Some(resolved.origin),
    };
    let mut providers = Vec::with_capacity(PROVIDER_IDS.len());
    for (id, reader_name) in PROVIDER_IDS {
        let mut args: Vec<OsString> = resolved.prefix.clone();
        if is_iana(&job.window.timezone) {
            args.push("--timezone".into());
            args.push(job.window.timezone.clone().into());
        }
        for word in [
            "report",
            "--format",
            "json",
            "-p",
            "today",
            "--provider",
            reader_name,
        ] {
            args.push(word.into());
        }
        let cancelled = || shared.cancelled(job.generation);
        match capture::run(&resolved.program, &args, &env, limits, &cancelled) {
            Ok(output) => {
                if output.code != Some(0) {
                    return Outcome::Fault(fault(ReaderState::Failed, output.code));
                }
                match parse::parse_report(&output.stdout, &day) {
                    Ok(reading) => providers.push(provider_usage(id, &reading)),
                    Err(state) => return Outcome::Fault(fault(state, None)),
                }
            }
            Err(RunError::Cancelled) => return Outcome::Cancelled,
            Err(RunError::Timeout) => return Outcome::Fault(fault(ReaderState::Timeout, None)),
            Err(RunError::TooLarge) => {
                return Outcome::Fault(fault(ReaderState::OutputTooLarge, None))
            }
            Err(RunError::Spawn) | Err(RunError::Wait) => {
                return Outcome::Fault(fault(ReaderState::Failed, None))
            }
        }
    }
    Outcome::Figures {
        providers,
        version: resolved.version,
        node_version: resolved.node_version,
        origin: resolved.origin,
    }
}

fn provider_usage(id: &'static str, reading: &Reading) -> ProviderUsage {
    let has_calls = reading.calls > 0;
    let estimated_costs = if !has_calls {
        Vec::new()
    } else if reading.unpriced_models == 0 || reading.cost_nanos > 0 {
        // With unpriced models present this is a lower bound; the UI says so.
        vec![Amount {
            currency: "USD",
            amount: format_nanos(reading.cost_nanos),
        }]
    } else {
        Vec::new()
    };
    ProviderUsage {
        id,
        state: if has_calls {
            ProviderState::Ok
        } else {
            ProviderState::NoCalls
        },
        sessions: Some(reading.sessions),
        calls: Some(reading.calls),
        tokens: Tokens {
            input: Some(reading.input),
            output: Some(reading.output),
            cache_read: Some(reading.cache_read),
            cache_write: Some(reading.cache_write),
        },
        estimated_costs,
        unpriced_models: reading.unpriced_models,
        unpriced_tokens: reading.unpriced_tokens,
        cost_complete: has_calls && reading.unpriced_models == 0,
    }
}

fn apply(state: &mut State, timing: &Timing, window: &Window, outcome: Outcome) {
    match outcome {
        Outcome::Figures {
            providers,
            version,
            node_version,
            origin,
        } => {
            state.cache.providers = providers;
            state.cache.data_window = Some(window.clone());
            state.cache.observed_at_ms = Some(now_unix_ms());
            state.cache.reader = Some(ReaderStatus {
                state: ReaderState::Ok,
                version: Some(version),
                pinned_version: pin().version.clone(),
                node_version: Some(node_version),
                exit_status: None,
                source: Some(origin.as_str()),
                development_build: false,
            });
            state.next_due = Some(Instant::now() + timing.interval);
        }
        Outcome::Fault(fault) => {
            // The last complete figures stay, marked stale by the snapshot.
            state.cache.reader = Some(fault.status());
            state.next_due = Some(Instant::now() + timing.retry_gap);
        }
        Outcome::Cancelled => {}
    }
}

fn snapshot(state: &State, timing: &Timing, window: &Window, development: bool) -> Snapshot {
    let now = now_unix_ms();
    if !state.enabled {
        return Snapshot {
            schema_version: SCHEMA_VERSION,
            enabled: false,
            window: window.clone(),
            data_window: None,
            reader: ReaderStatus {
                development_build: development,
                ..ReaderStatus::new(ReaderState::Off)
            },
            refreshing: false,
            observed_at_unix_ms: None,
            stale: false,
            providers: not_run_providers(),
        };
    }
    let cache = &state.cache;
    let reader = ReaderStatus {
        development_build: development,
        ..cache
            .reader
            .clone()
            .unwrap_or_else(|| ReaderStatus::new(ReaderState::NotStarted))
    };
    let has_figures = cache.observed_at_ms.is_some();
    let data_window = cache.data_window.as_ref().map(|data| DataWindow {
        start_unix_ms: data.start_unix_ms,
        end_unix_ms: data.end_unix_ms,
        timezone: data.timezone.clone(),
    });
    let stale = cache.observed_at_ms.is_some_and(|observed| {
        let max_age = timing.interval.as_millis() as i64 * 3;
        now - observed > max_age || cache.data_window.as_ref() != Some(window)
    });
    Snapshot {
        schema_version: SCHEMA_VERSION,
        enabled: true,
        window: window.clone(),
        data_window,
        reader,
        refreshing: state.running,
        observed_at_unix_ms: cache.observed_at_ms,
        stale,
        providers: if has_figures {
            cache.providers.clone()
        } else {
            not_run_providers()
        },
    }
}

fn not_run_providers() -> Vec<ProviderUsage> {
    PROVIDER_IDS
        .iter()
        .map(|(id, _)| ProviderUsage::not_run(id))
        .collect()
}

/// IANA names (`Area/City`, `UTC`) are passed to the reader; anything else
/// (a bare offset label) is left to the reader's system timezone, and the day
/// check then catches a mismatch.
fn is_iana(zone: &str) -> bool {
    zone == "UTC"
        || (zone.contains('/')
            && zone.len() <= 64
            && zone
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/_+-".contains(&byte)))
}

/// `YYYY-MM-DD` of the local day that starts at `start_ms` in a fixed offset.
fn local_day(start_ms: i64, offset_seconds: i64) -> String {
    let days = (start_ms + offset_seconds * 1000).div_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn read_opt_in(path: &Path) -> bool {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<OptIn>(&bytes).ok())
        .is_some_and(|opt_in| opt_in.schema_version == 1 && opt_in.enabled)
}

fn persist_opt_in(path: &Path, enabled: bool) -> io::Result<()> {
    let body = serde_json::to_vec(&OptIn {
        schema_version: 1,
        enabled,
    })
    .map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, body)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pin::Layouts;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(local_day(1_763_164_800_000, 0), "2025-11-15");
    }

    #[test]
    fn local_day_follows_the_offset() {
        // 2026-01-01T00:00:00Z is still Dec 31 at UTC-5.
        let utc_midnight = 1_767_225_600_000;
        assert_eq!(local_day(utc_midnight, 0), "2026-01-01");
        assert_eq!(
            local_day(utc_midnight + 5 * 3_600_000, -5 * 3600),
            "2026-01-01"
        );
        // 19:00 on Dec 31 at UTC-5.
        assert_eq!(
            local_day(utc_midnight - 5 * 3_600_000, -5 * 3600),
            "2025-12-31"
        );
    }

    #[test]
    fn only_iana_names_are_passed_to_the_reader() {
        assert!(is_iana("America/Chicago"));
        assert!(is_iana("UTC"));
        assert!(!is_iana("UTC+05:30"));
        assert!(!is_iana("Bad Zone; rm"));
    }

    fn locations(dir: &Path, development: bool) -> ReaderLocations {
        ReaderLocations {
            bundle_dir: Some(dir.join("bundle")),
            bundle_fallback_dir: None,
            development,
            override_command: None,
            path_dirs: vec![dir.join("bin")],
        }
    }

    fn put_codeburn_on_path(dir: &Path) {
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join(locate::reader_names()[0]), b"x").unwrap();
    }

    #[test]
    fn a_release_build_never_looks_at_path() {
        let dir = tempfile::tempdir().unwrap();
        put_codeburn_on_path(dir.path());
        let plan = locations(dir.path(), false).plan();
        assert_eq!(plan, Plan::Missing(ReaderState::NotBundled));
    }

    #[test]
    fn a_complete_bundle_wins_over_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        put_codeburn_on_path(dir.path());
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(bundle.join(BUNDLE_PACKAGE)).unwrap();
        std::fs::write(bundle.join(BUNDLE_PACKAGE).join("package.json"), b"{}").unwrap();
        std::fs::write(bundle.join(locate::node_names()[0]), b"x").unwrap();
        assert_eq!(
            locations(dir.path(), true).plan(),
            Plan::Bundle(bundle.clone())
        );
        assert_eq!(locations(dir.path(), false).plan(), Plan::Bundle(bundle));
    }

    #[test]
    fn an_incomplete_bundle_is_not_bundled_in_release_and_falls_through_in_dev() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join(locate::node_names()[0]), b"x").unwrap();
        assert_eq!(
            locations(dir.path(), false).plan(),
            Plan::Missing(ReaderState::NotBundled)
        );
        put_codeburn_on_path(dir.path());
        assert_eq!(
            locations(dir.path(), true).plan(),
            Plan::Command {
                path: dir.path().join("bin").join(locate::reader_names()[0]),
                origin: ReaderOrigin::Path,
            }
        );
    }

    #[test]
    fn development_uses_the_override_before_path() {
        let dir = tempfile::tempdir().unwrap();
        put_codeburn_on_path(dir.path());
        let override_file = dir.path().join("elsewhere").join("codeburn");
        std::fs::create_dir_all(override_file.parent().unwrap()).unwrap();
        std::fs::write(&override_file, b"x").unwrap();
        let mut dev = locations(dir.path(), true);
        dev.override_command = Some(override_file.clone());
        assert_eq!(
            dev.plan(),
            Plan::Command {
                path: override_file,
                origin: ReaderOrigin::Override,
            }
        );
        dev.override_command = Some(dir.path().join("missing"));
        assert_eq!(dev.plan(), Plan::Missing(ReaderState::NotInstalled));
        dev.override_command = None;
        dev.path_dirs = Vec::new();
        assert_eq!(dev.plan(), Plan::Missing(ReaderState::NotInstalled));
    }

    /// A fake installed package with the given version, and a pin that matches its files.
    fn fake_package(root: &Path, version: &str) -> Pin {
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(
            root.join("package.json"),
            format!(r#"{{"name":"codeburn","version":"{version}"}}"#),
        )
        .unwrap();
        let mut files = std::collections::BTreeMap::new();
        for (relative, body) in [
            (
                "dist/cli.js",
                &b"#!/usr/bin/env node\nimport('./main.js')\n"[..],
            ),
            ("dist/main.js", &b"module code"[..]),
            ("dist/parse-worker.js", &b"worker code"[..]),
        ] {
            std::fs::write(root.join(relative), body).unwrap();
            files.insert(
                relative.to_owned(),
                hash_file(&root.join(relative), relative == SHEBANG_STRIPPED).unwrap(),
            );
        }
        Pin {
            name: "codeburn".into(),
            version: version.into(),
            node_minimum: "22.15.0".into(),
            tarball_integrity: "sha512-test".into(),
            layouts: Layouts {
                npm: files.clone(),
                bundled: files,
            },
        }
    }

    #[test]
    fn an_installed_reader_must_match_the_pin_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pkg");
        let pin = fake_package(&root, "0.9.28");
        assert_eq!(
            verify_installed(&root, &pin, ReaderOrigin::Path).unwrap(),
            "0.9.28"
        );

        let other = dir.path().join("other");
        let mut wanted = fake_package(&other, "0.9.10");
        wanted.version = "0.9.28".into();
        let fault = verify_installed(&other, &wanted, ReaderOrigin::Path).unwrap_err();
        assert_eq!(fault.state, ReaderState::UnsupportedVersion);
        assert_eq!(fault.version.as_deref(), Some("0.9.10"));

        std::fs::write(root.join("dist").join("main.js"), b"tampered").unwrap();
        let fault = verify_installed(&root, &pin, ReaderOrigin::Bundled).unwrap_err();
        assert_eq!(fault.state, ReaderState::IntegrityMismatch);
        assert_eq!(fault.origin, Some(ReaderOrigin::Bundled));
    }

    #[test]
    fn the_macos_resources_bundle_is_found_when_the_executable_folder_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let resources = dir
            .path()
            .join("Contents")
            .join("Resources")
            .join(BUNDLE_DIR);
        std::fs::create_dir_all(resources.join(BUNDLE_PACKAGE)).unwrap();
        std::fs::write(resources.join(BUNDLE_PACKAGE).join("package.json"), b"{}").unwrap();
        std::fs::write(resources.join(locate::node_names()[0]), b"x").unwrap();
        let locations = ReaderLocations {
            bundle_dir: Some(dir.path().join("Contents").join("MacOS").join(BUNDLE_DIR)),
            bundle_fallback_dir: Some(resources.clone()),
            development: false,
            override_command: None,
            path_dirs: Vec::new(),
        };
        assert_eq!(locations.plan(), Plan::Bundle(resources));
    }

    #[cfg(unix)]
    #[test]
    fn a_bundle_resolves_to_its_node_and_its_launcher_and_checks_its_own_layout() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join(BUNDLE_DIR);
        let root = bundle.join(BUNDLE_PACKAGE);
        let mut pin = fake_package(&root, "0.9.28");
        let node = bundle.join("node");
        std::fs::write(&node, b"#!/bin/sh\necho v22.23.5\n").unwrap();
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
        let entry = root.join("dist").join("cli.js");

        let resolved = resolve_installed(
            &root,
            entry.clone(),
            node.clone(),
            ReaderOrigin::Bundled,
            &pin,
        )
        .expect("a complete bundle with a supported node resolves");
        assert_eq!(resolved.program, node);
        assert_eq!(resolved.prefix, vec![entry.clone().into_os_string()]);
        assert_eq!(resolved.node_version, "22.23.5");
        assert_eq!(resolved.origin, ReaderOrigin::Bundled);

        // A bundle whose main.js differs from the bundled hash is refused, even when the npm
        // hash for the same file would match.
        pin.layouts
            .bundled
            .insert("dist/main.js".into(), "0".repeat(64));
        let fault = resolve_installed(&root, entry, node, ReaderOrigin::Bundled, &pin).unwrap_err();
        assert_eq!(fault.state, ReaderState::IntegrityMismatch);
    }

    #[test]
    fn an_npm_install_is_checked_against_the_npm_layout_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pkg");
        let mut pin = fake_package(&root, "0.9.28");
        pin.layouts
            .bundled
            .insert("dist/main.js".into(), "0".repeat(64));
        assert_eq!(
            verify_installed(&root, &pin, ReaderOrigin::Path).unwrap(),
            "0.9.28",
            "an npm install matches the npm layout"
        );
        let fault = verify_installed(&root, &pin, ReaderOrigin::Bundled).unwrap_err();
        assert_eq!(fault.state, ReaderState::IntegrityMismatch);
    }

    #[test]
    fn a_shebang_rewrite_does_not_change_the_pinned_hash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pkg");
        let pin = fake_package(&root, "0.9.28");
        std::fs::write(
            root.join("dist").join("cli.js"),
            b"#!/opt/homebrew/opt/node/bin/node\nimport('./main.js')\n",
        )
        .unwrap();
        assert_eq!(
            verify_installed(&root, &pin, ReaderOrigin::Path).unwrap(),
            "0.9.28"
        );
    }

    #[test]
    fn the_opt_in_file_defaults_to_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(OPT_IN_FILE);
        assert!(!read_opt_in(&path));
        persist_opt_in(&path, true).unwrap();
        assert!(read_opt_in(&path));
        fs::write(&path, b"{\"schema_version\":2,\"enabled\":true}").unwrap();
        assert!(!read_opt_in(&path), "an unknown schema is treated as off");
    }
}
