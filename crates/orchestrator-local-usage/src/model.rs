//! The snapshot the daemon returns over `usage.localTools*` (schema version 1).
//! Every figure is an estimate read from local session files.

use serde::Serialize;

/// Wire schema version of [`Snapshot`].
pub const SCHEMA_VERSION: u32 = 1;

/// Provider ids in report order: MCAO's id, then the reader's `--provider` name.
pub const PROVIDER_IDS: [(&str, &str); 3] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("github-copilot", "copilot"),
];

/// The local day the figures are for: `[start, end)` in Unix milliseconds in a
/// fixed UTC offset, as the client supplies it (the same window `usage.summary` uses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Window {
    pub start_unix_ms: i64,
    pub end_unix_ms: i64,
    pub timezone: String,
    pub utc_offset_seconds: i64,
}

/// The window the provider figures cover, which can differ from the requested
/// window for a while after midnight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataWindow {
    pub start_unix_ms: i64,
    pub end_unix_ms: i64,
    pub timezone: String,
}

/// Why the reader did or did not produce figures. Each non-`ok` state has an
/// honest next step in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderState {
    /// Opt-in is off: nothing is read.
    Off,
    /// Opt-in is on and no read has finished yet.
    NotStarted,
    Ok,
    /// Release build without the bundled reader (PATH is never used there).
    NotBundled,
    NotInstalled,
    NodeMissing,
    NodeTooOld,
    UnsupportedVersion,
    IntegrityMismatch,
    Timeout,
    Failed,
    BadOutput,
    OutputTooLarge,
    WrongDay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReaderStatus {
    pub state: ReaderState,
    /// The installed reader version, when one was found.
    pub version: Option<String>,
    /// The only version MCAO supports.
    pub pinned_version: String,
    /// The Node.js version the reader ran on, when Node was found.
    pub node_version: Option<String>,
    /// The reader's exit status, only for `failed`.
    pub exit_status: Option<i32>,
    /// Where the reader came from: `bundled` (shipped with the app), `override`
    /// or `path` (both development builds only). Null when none was used.
    pub source: Option<&'static str>,
    /// True in debug builds, where the override and PATH lookup exist.
    pub development_build: bool,
}

impl ReaderStatus {
    pub(crate) fn new(state: ReaderState) -> Self {
        Self {
            state,
            version: None,
            pinned_version: crate::pin::pin().version.clone(),
            node_version: None,
            exit_status: None,
            source: None,
            development_build: false,
        }
    }
}

/// Amount in one currency, as a decimal string (never a float).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Amount {
    pub currency: &'static str,
    pub amount: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    /// No figures yet.
    NotRun,
    /// The reader reported at least one call in the window.
    Ok,
    /// The reader reported no calls in the window. This is not zero spend.
    NoCalls,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tokens {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderUsage {
    pub id: &'static str,
    pub state: ProviderState,
    pub sessions: Option<u64>,
    pub calls: Option<u64>,
    pub tokens: Tokens,
    /// Estimated cost from list prices. Empty means unknown, never zero.
    pub estimated_costs: Vec<Amount>,
    /// Models that had tokens but no price. Their cost is not in `estimated_costs`.
    pub unpriced_models: u64,
    pub unpriced_tokens: u64,
    /// True only when every call had a price.
    pub cost_complete: bool,
}

impl ProviderUsage {
    pub(crate) fn not_run(id: &'static str) -> Self {
        Self {
            id,
            state: ProviderState::NotRun,
            sessions: None,
            calls: None,
            tokens: Tokens::default(),
            estimated_costs: Vec::new(),
            unpriced_models: 0,
            unpriced_tokens: 0,
            cost_complete: false,
        }
    }
}

/// The `usage.localTools` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub enabled: bool,
    pub window: Window,
    pub data_window: Option<DataWindow>,
    pub reader: ReaderStatus,
    pub refreshing: bool,
    pub observed_at_unix_ms: Option<i64>,
    pub stale: bool,
    pub providers: Vec<ProviderUsage>,
}
