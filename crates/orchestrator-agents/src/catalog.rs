//! Provider, model and effort catalog for the daemon-launchable adapters
//! (board item M7-09).
//!
//! Each catalog reports only what the installed CLI itself exposes, read by
//! running its own read-only commands: Codex lists models and per-model
//! reasoning levels (`codex debug models`) and reports login state
//! (`codex login status`); Claude Code documents its `--effort` levels in
//! `--help` and reports login state (`claude auth status`) but cannot
//! enumerate models. Nothing here is a hard-coded model list. A fact the CLI
//! does not expose is reported as `unsupported`, and a fact that could not be
//! read (timeout, unparseable output) is `unknown`; neither is ever treated as
//! "none" or "all". The provider is the adapter family's fixed vendor, not a
//! selectable value: choosing a provider means choosing the agent.
//!
//! Probes run the user's own installed binary with a null stdin, bounded
//! time and bounded output. Only booleans and catalog fields are read from
//! the output; account identifiers in it are never retained.

use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::claude_code_adapter::ClaudeCodeAdapter;
use crate::codex_adapter::CodexAdapter;

const CODEX_AGENT: &str = "codex";
const CLAUDE_AGENT: &str = "claude-code";
const PI_AGENT: &str = "pi";
const CODEX_PROVIDER: &str = "openai";
const CLAUDE_PROVIDER: &str = "anthropic";

/// The vendor behind an adapter family, or `None` when the family is not
/// tied to one. A static fact of the adapter, not something read from a CLI.
pub fn provider_of(agent: &str) -> Option<&'static str> {
    match agent {
        CODEX_AGENT => Some(CODEX_PROVIDER),
        CLAUDE_AGENT => Some(CLAUDE_PROVIDER),
        _ => None,
    }
}
/// `codex debug models` prints the whole catalog on one line (hundreds of
/// KiB), so it cannot go through the line-capped process supervisor.
const MAX_STDOUT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: u64 = 64 * 1024;
const MAX_MODELS: usize = 256;
const MAX_LEVELS: usize = 16;
const MAX_ID_LEN: usize = 128;
const MAX_TOKEN_LEN: usize = 32;
const MAX_DISPLAY_LEN: usize = 80;

/// How much of a catalog fact is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogState {
    /// Read from the installed CLI; `items` is complete.
    Available,
    /// Could not be read (timeout, unreadable output). Not "none".
    Unknown,
    /// The adapter's executable is not installed.
    Unavailable,
    /// The CLI offers no way to read this fact.
    Unsupported,
    /// Credentials are missing, so nothing is offered.
    AuthenticationFailed,
}

/// One selectable model with the reasoning levels it supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelOption {
    pub id: String,
    pub display_name: String,
    pub efforts: Vec<String>,
    /// The model's own default level when the CLI reports one; `None` means
    /// unknown.
    pub default_effort: Option<String>,
}

/// A list-valued catalog fact together with how it was learned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facet<T> {
    pub state: CatalogState,
    pub detail: String,
    pub items: Vec<T>,
}

impl<T> Facet<T> {
    fn without_items(state: CatalogState, detail: &str) -> Self {
        Self {
            state,
            detail: detail.to_owned(),
            items: Vec::new(),
        }
    }
}

/// What one agent offers for selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCatalog {
    pub agent: String,
    /// The adapter family's vendor, or `None` when the adapter is not tied
    /// to one.
    pub provider: Option<String>,
    pub state: CatalogState,
    pub detail: String,
    /// Selectable models. Empty unless its state is `available`.
    pub models: Facet<ModelOption>,
    /// Adapter-wide effort levels, for adapters whose levels do not depend
    /// on the model. Codex reports levels per model instead.
    pub efforts: Facet<String>,
}

/// Why a selection was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionReason {
    AgentUnavailable,
    AuthenticationFailed,
    AgentStateUnknown,
    AgentUnsupported,
    ModelsUnsupported,
    ModelsUnknown,
    UnknownModel,
    EffortRequiresModel,
    EffortUnsupported,
    EffortUnknown,
}

/// A refused selection with a precise, non-sensitive explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectionError {
    pub reason: SelectionReason,
    pub detail: String,
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for SelectionError {}

impl SelectionReason {
    /// Whether the request itself is wrong (as opposed to the adapter being
    /// in a state where no selection can be checked).
    pub fn is_invalid_request(self) -> bool {
        matches!(
            self,
            Self::UnknownModel
                | Self::EffortRequiresModel
                | Self::EffortUnsupported
                | Self::ModelsUnsupported
        )
    }
}

fn refuse(reason: SelectionReason, detail: String) -> SelectionError {
    SelectionError { reason, detail }
}

impl AgentCatalog {
    fn uniform(agent: &str, provider: Option<&str>, state: CatalogState, detail: &str) -> Self {
        Self {
            agent: agent.to_owned(),
            provider: provider.map(str::to_owned),
            state,
            detail: detail.to_owned(),
            models: Facet::without_items(state, detail),
            efforts: Facet::without_items(state, detail),
        }
    }

    /// A catalog that could not be obtained, for example because the read is
    /// still running. Not "none": nothing can be selected from it.
    pub fn unknown(agent: &str, detail: &str) -> Self {
        Self::uniform(agent, provider_of(agent), CatalogState::Unknown, detail)
    }

    /// Whether every part of this catalog is a fact rather than a failed
    /// read. Only a definitive catalog is worth reusing.
    pub fn is_definitive(&self) -> bool {
        [self.state, self.models.state, self.efforts.state]
            .iter()
            .all(|state| *state != CatalogState::Unknown)
    }

    /// An adapter the daemon cannot launch with a selection.
    pub fn unsupported_agent(agent: &str, detail: &str) -> Self {
        Self::uniform(agent, None, CatalogState::Unsupported, detail)
    }

    /// Check a requested selection against this catalog. A model or effort
    /// that the catalog does not list is refused, and so is any selection for
    /// an adapter whose catalog could not be established.
    pub fn validate(
        &self,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<(), SelectionError> {
        let agent = &self.agent;
        match self.state {
            CatalogState::Available => {}
            CatalogState::Unavailable => {
                return Err(refuse(
                    SelectionReason::AgentUnavailable,
                    format!("agent {agent} is not installed"),
                ))
            }
            CatalogState::AuthenticationFailed => {
                return Err(refuse(
                    SelectionReason::AuthenticationFailed,
                    format!("agent {agent} has no usable credentials; sign in first"),
                ))
            }
            CatalogState::Unknown => {
                return Err(refuse(
                    SelectionReason::AgentStateUnknown,
                    format!("the state of agent {agent} could not be determined"),
                ))
            }
            CatalogState::Unsupported => {
                return Err(refuse(
                    SelectionReason::AgentUnsupported,
                    format!("agent {agent} does not support model or effort selection"),
                ))
            }
        }
        let chosen = match model {
            None => None,
            Some(model) => match self.models.state {
                CatalogState::Available => {
                    match self.models.items.iter().find(|option| option.id == model) {
                        Some(option) => Some(option),
                        None => {
                            return Err(refuse(
                                SelectionReason::UnknownModel,
                                format!("agent {agent} does not offer model {model:?}"),
                            ))
                        }
                    }
                }
                CatalogState::Unknown => {
                    return Err(refuse(
                        SelectionReason::ModelsUnknown,
                        format!("the models of agent {agent} could not be determined"),
                    ))
                }
                _ => {
                    return Err(refuse(
                        SelectionReason::ModelsUnsupported,
                        format!("agent {agent} exposes no model list; a model cannot be selected"),
                    ))
                }
            },
        };
        let Some(effort) = effort else {
            return Ok(());
        };
        if let Some(option) = chosen {
            return if option.efforts.iter().any(|level| level == effort) {
                Ok(())
            } else {
                Err(refuse(
                    SelectionReason::EffortUnsupported,
                    format!(
                        "model {:?} of agent {agent} does not support effort {effort:?}; it supports {:?}",
                        option.id, option.efforts
                    ),
                ))
            };
        }
        match self.efforts.state {
            CatalogState::Available => {
                if self.efforts.items.iter().any(|level| level == effort) {
                    Ok(())
                } else {
                    Err(refuse(
                        SelectionReason::EffortUnsupported,
                        format!(
                            "agent {agent} does not support effort {effort:?}; it supports {:?}",
                            self.efforts.items
                        ),
                    ))
                }
            }
            CatalogState::Unknown => Err(refuse(
                SelectionReason::EffortUnknown,
                format!("the effort levels of agent {agent} could not be determined"),
            )),
            _ if self.models.state == CatalogState::Available => Err(refuse(
                SelectionReason::EffortRequiresModel,
                format!(
                    "agent {agent} reports effort levels per model and its default model is unknown; select a model with the effort"
                ),
            )),
            _ => Err(refuse(
                SelectionReason::EffortUnsupported,
                format!("agent {agent} does not support effort selection"),
            )),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Captured {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureFailure {
    Launch,
    TimedOut,
    TooLarge,
}

/// Everything a probe started, not just the process the daemon spawned.
///
/// Unix: the probe leads its own process group and the whole group is sent
/// `SIGKILL`, so a wrapper script or shim cannot leave a grandchild running
/// (or holding the output pipes open) after the probe ends or times out.
///
/// Windows: only the direct child is killed (`kill_on_drop`); a `.cmd` or npm
/// shim's descendants can outlive a timed-out probe. Closing a kill-on-close
/// Job Object that holds the child would cover them, as the process
/// supervisor in `orchestrator-runtime` does for agent launches, but that
/// needs a suspended spawn and handle plumbing that is out of scope for a
/// read-only probe. Tracked as a Windows follow-up.
struct ProcessGroup {
    #[cfg(unix)]
    leader: Option<u32>,
    armed: AtomicBool,
}

impl ProcessGroup {
    fn of(child: &tokio::process::Child) -> Self {
        #[cfg(not(unix))]
        let _ = child;
        Self {
            #[cfg(unix)]
            leader: child.id(),
            armed: AtomicBool::new(true),
        }
    }

    /// Kill every member. Harmless when the group is already empty.
    fn kill(&self) {
        #[cfg(unix)]
        if let Some(leader) = self.leader.and_then(|pid| i32::try_from(pid).ok()) {
            // SAFETY: `kill` with a negative pid only signals the group this
            // probe created; it touches no memory.
            let _ = unsafe { libc::kill(-leader, libc::SIGKILL) };
        }
    }

    /// The group has been dealt with; dropping must not signal a pid the
    /// system may have reused.
    fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if self.armed.load(Ordering::Acquire) {
            self.kill();
        }
    }
}

/// Run one read-only command to completion within `limit`. Dropping the
/// future (including on timeout) kills the child and its process group.
/// Once the child exits, anything it left behind is killed too, so a
/// background process holding the output pipes cannot stall the read.
async fn capture(
    program: &Path,
    args: &[&str],
    working_dir: &Path,
    limit: Duration,
) -> Result<Captured, CaptureFailure> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|_| CaptureFailure::Launch)?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(CaptureFailure::Launch);
    };
    let run = async {
        let group = ProcessGroup::of(&child);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut out_reader = stdout.take(MAX_STDOUT_BYTES + 1);
        let mut err_reader = stderr.take(MAX_STDERR_BYTES);
        let reads = async {
            let (read_out, read_err) = tokio::join!(
                out_reader.read_to_end(&mut out),
                err_reader.read_to_end(&mut err),
            );
            // The child may be blocked writing to a pipe nobody reads now.
            if out.len() as u64 > MAX_STDOUT_BYTES {
                group.kill();
                return Err(CaptureFailure::TooLarge);
            }
            if read_out.is_err() || read_err.is_err() {
                group.kill();
                return Err(CaptureFailure::Launch);
            }
            Ok(())
        };
        let exit = async {
            let status = child.wait().await;
            group.kill();
            group.disarm();
            status
        };
        let (read_result, status) = tokio::join!(reads, exit);
        read_result?;
        let status = status.map_err(|_| CaptureFailure::Launch)?;
        Ok(Captured {
            code: status.code(),
            stdout: out,
            stderr: err,
        })
    };
    tokio::time::timeout(limit, run)
        .await
        .unwrap_or(Err(CaptureFailure::TimedOut))
}

/// A token an adapter command line may carry: no leading dash, no spaces.
fn safe_token(value: &str, max: usize, extra: &[char]) -> bool {
    let mut chars = value.chars();
    value.len() <= max
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

fn safe_model_id(value: &str) -> bool {
    safe_token(value, MAX_ID_LEN, &['.', '_', ':', '/', '@', '+', '-'])
}

fn safe_effort(value: &str) -> bool {
    value.chars().all(|c| !c.is_ascii_uppercase()) && safe_token(value, MAX_TOKEN_LEN, &['-', '_'])
}

fn display_name(raw: Option<&str>, fallback: &str) -> String {
    let cleaned: String = raw
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DISPLAY_LEN)
        .collect();
    if cleaned.trim().is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

/// Selectable models from `codex debug models` output. Only models the CLI
/// marks `visibility: "list"` are offered. `None` means the output was not
/// the expected catalog.
pub(crate) fn parse_codex_models(stdout: &[u8]) -> Option<Vec<ModelOption>> {
    let root: Value = serde_json::from_slice(stdout).ok()?;
    let entries = root.get("models")?.as_array()?;
    let mut options: Vec<ModelOption> = Vec::new();
    for entry in entries {
        if entry.get("visibility").and_then(Value::as_str) != Some("list") {
            continue;
        }
        let Some(id) = entry.get("slug").and_then(Value::as_str) else {
            continue;
        };
        if !safe_model_id(id) || options.iter().any(|option| option.id == id) {
            continue;
        }
        let mut efforts: Vec<String> = Vec::new();
        for level in entry
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(MAX_LEVELS)
        {
            if let Some(name) = level.get("effort").and_then(Value::as_str) {
                if safe_effort(name) && !efforts.iter().any(|existing| existing == name) {
                    efforts.push(name.to_owned());
                }
            }
        }
        let default_effort = entry
            .get("default_reasoning_level")
            .and_then(Value::as_str)
            .filter(|name| efforts.iter().any(|level| level == name))
            .map(str::to_owned);
        options.push(ModelOption {
            id: id.to_owned(),
            display_name: display_name(entry.get("display_name").and_then(Value::as_str), id),
            efforts,
            default_effort,
        });
        if options.len() >= MAX_MODELS {
            break;
        }
    }
    Some(options)
}

/// Effort levels from the `--effort <level>` entry of `claude --help`.
pub(crate) fn parse_claude_efforts(help: &str) -> Option<Vec<String>> {
    let mut entry = String::new();
    // The entry's description may wrap; it ends at the next flag.
    for line in help
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("--effort"))
        .take(4)
    {
        if !entry.is_empty() && line.trim_start().starts_with("--") {
            break;
        }
        entry.push_str(line);
        entry.push(' ');
    }
    let open = entry.find('(')?;
    let close = open + entry[open..].find(')')?;
    let levels: Vec<String> = entry[open + 1..close]
        .split(',')
        .map(|level| level.trim().to_owned())
        .collect();
    if levels.is_empty() || levels.len() > MAX_LEVELS || !levels.iter().all(|l| safe_effort(l)) {
        return None;
    }
    Some(levels)
}

/// The `loggedIn` field of `claude auth status`; every other field (account
/// identifiers included) is ignored.
pub(crate) fn parse_claude_logged_in(stdout: &[u8]) -> Option<bool> {
    serde_json::from_slice::<Value>(stdout)
        .ok()?
        .get("loggedIn")?
        .as_bool()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Login {
    SignedIn,
    SignedOut,
    Unknown,
}

fn codex_login(result: &Result<Captured, CaptureFailure>) -> Login {
    match result {
        Ok(captured) if captured.code == Some(0) => Login::SignedIn,
        Ok(captured) => {
            let text = String::from_utf8_lossy(&captured.stdout).to_lowercase()
                + &String::from_utf8_lossy(&captured.stderr).to_lowercase();
            if text.contains("not logged in") {
                Login::SignedOut
            } else {
                Login::Unknown
            }
        }
        Err(_) => Login::Unknown,
    }
}

fn claude_login(result: &Result<Captured, CaptureFailure>) -> Login {
    match result {
        Ok(captured) => match parse_claude_logged_in(&captured.stdout) {
            Some(true) => Login::SignedIn,
            Some(false) => Login::SignedOut,
            None => Login::Unknown,
        },
        Err(_) => Login::Unknown,
    }
}

/// Catalog states that apply before any list is read: a missing login or an
/// unreadable login state offers nothing.
fn gate(agent: &str, provider: &str, login: Login) -> Option<AgentCatalog> {
    match login {
        Login::SignedIn => None,
        Login::SignedOut => Some(AgentCatalog::uniform(
            agent,
            Some(provider),
            CatalogState::AuthenticationFailed,
            "the CLI reports that no credentials are configured",
        )),
        Login::Unknown => Some(AgentCatalog::uniform(
            agent,
            Some(provider),
            CatalogState::Unknown,
            "the CLI's login state could not be read",
        )),
    }
}

fn codex_catalog(
    login: &Result<Captured, CaptureFailure>,
    models: Result<Captured, CaptureFailure>,
) -> AgentCatalog {
    if let Some(blocked) = gate(CODEX_AGENT, CODEX_PROVIDER, codex_login(login)) {
        return blocked;
    }
    let parsed = match &models {
        Ok(captured) if captured.code == Some(0) => parse_codex_models(&captured.stdout),
        _ => None,
    };
    let models = match parsed {
        Some(items) if !items.is_empty() => Facet {
            state: CatalogState::Available,
            detail: "listed by `codex debug models`".to_owned(),
            items,
        },
        Some(_) => Facet::without_items(
            CatalogState::Unknown,
            "the CLI reported no selectable models",
        ),
        None => Facet::without_items(
            CatalogState::Unknown,
            "the CLI's model catalog could not be read",
        ),
    };
    AgentCatalog {
        agent: CODEX_AGENT.to_owned(),
        provider: Some(CODEX_PROVIDER.to_owned()),
        state: CatalogState::Available,
        detail: "installed and signed in".to_owned(),
        models,
        efforts: Facet::without_items(
            CatalogState::Unsupported,
            "effort levels are reported per model",
        ),
    }
}

fn claude_catalog(
    auth: &Result<Captured, CaptureFailure>,
    help: Result<Captured, CaptureFailure>,
) -> AgentCatalog {
    if let Some(blocked) = gate(CLAUDE_AGENT, CLAUDE_PROVIDER, claude_login(auth)) {
        return blocked;
    }
    let levels = match &help {
        Ok(captured) if captured.code == Some(0) => {
            parse_claude_efforts(&String::from_utf8_lossy(&captured.stdout))
        }
        _ => None,
    };
    let efforts = match levels {
        Some(items) => Facet {
            state: CatalogState::Available,
            detail: "documented by `claude --help`".to_owned(),
            items,
        },
        None => Facet::without_items(
            CatalogState::Unknown,
            "the CLI's effort levels could not be read",
        ),
    };
    AgentCatalog {
        agent: CLAUDE_AGENT.to_owned(),
        provider: Some(CLAUDE_PROVIDER.to_owned()),
        state: CatalogState::Available,
        detail: "installed and signed in".to_owned(),
        models: Facet::without_items(
            CatalogState::Unsupported,
            "the CLI cannot enumerate the models available to the account",
        ),
        efforts,
    }
}

impl CodexAdapter {
    /// Read this adapter's catalog from the installed CLI. Each probe is
    /// bounded by `limit`; the probes run together.
    pub async fn catalog(&self, working_dir: &Path, limit: Duration) -> AgentCatalog {
        let Some(program) = self.locate() else {
            return AgentCatalog::uniform(
                CODEX_AGENT,
                Some(CODEX_PROVIDER),
                CatalogState::Unavailable,
                "the codex executable was not found",
            );
        };
        let (login, models) = tokio::join!(
            capture(&program, &["login", "status"], working_dir, limit),
            capture(&program, &["debug", "models"], working_dir, limit),
        );
        codex_catalog(&login, models)
    }

    /// Arguments that select a model and effort for `codex exec`; they go
    /// before the `--` separator. Both values must already be validated
    /// against the catalog.
    pub fn selection_args(model: Option<&str>, effort: Option<&str>) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(model) = model {
            args.push("-m".to_owned());
            args.push(model.to_owned());
        }
        if let Some(effort) = effort {
            args.push("-c".to_owned());
            args.push(format!("model_reasoning_effort=\"{effort}\""));
        }
        args
    }

    /// Arguments for a text-only conversation turn: a read-only sandbox, no
    /// session file and no repository requirement. The prompt is one
    /// argument after `--`; no shell is involved.
    pub fn args_for_conversation(
        prompt: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Vec<String> {
        [
            "exec",
            "--json",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--ephemeral",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain(Self::selection_args(model, effort))
        .chain(["--".to_owned(), prompt.to_owned()])
        .collect()
    }
}

impl ClaudeCodeAdapter {
    /// Read this adapter's catalog from the installed CLI. Each probe is
    /// bounded by `limit`; the probes run together.
    pub async fn catalog(&self, working_dir: &Path, limit: Duration) -> AgentCatalog {
        let Some(program) = self.locate() else {
            return AgentCatalog::uniform(
                CLAUDE_AGENT,
                Some(CLAUDE_PROVIDER),
                CatalogState::Unavailable,
                "the claude executable was not found",
            );
        };
        let (auth, help) = tokio::join!(
            capture(&program, &["auth", "status"], working_dir, limit),
            capture(&program, &["--help"], working_dir, limit),
        );
        claude_catalog(&auth, help)
    }

    /// [`Self::args_for_prompt`] plus the selected model and effort. The
    /// restricted base invocation is unchanged.
    pub fn args_for_prompt_with_selection(
        prompt: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Vec<String> {
        let mut args = Self::args_for_prompt(prompt);
        if let Some(model) = model {
            args.push("--model".into());
            args.push(model.into());
        }
        if let Some(effort) = effort {
            args.push("--effort".into());
            args.push(effort.into());
        }
        args
    }

    /// Arguments for a text-only conversation turn: every tool is disabled
    /// (`--tools ""`), nothing is persisted, and permission prompts are
    /// refused. The prompt stays one argument; no shell is involved.
    pub fn args_for_conversation(
        prompt: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Vec<String> {
        let mut args: Vec<String> = [
            "-p",
            prompt,
            "--verbose",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--restricted",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--tools",
            "",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        if let Some(model) = model {
            args.push("--model".into());
            args.push(model.into());
        }
        if let Some(effort) = effort {
            args.push("--effort".into());
            args.push(effort.into());
        }
        args
    }
}

/// The catalog entry for the Pi adapter: the daemon has no launcher for it,
/// so no selection can take effect.
pub fn pi_catalog() -> AgentCatalog {
    AgentCatalog::unsupported_agent(
        PI_AGENT,
        "the daemon cannot launch pi, so model and effort selection is unsupported",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured(code: i32, stdout: &str) -> Result<Captured, CaptureFailure> {
        Ok(Captured {
            code: Some(code),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        })
    }

    const CODEX_MODELS: &str = r#"{"models":[
        {"slug":"gpt-6-luna","display_name":"GPT-6 Luna","visibility":"list",
         "default_reasoning_level":"medium",
         "supported_reasoning_levels":[{"effort":"low","description":"x"},{"effort":"medium","description":"y"},{"effort":"max","description":"z"}]},
        {"slug":"gpt-6-sol","display_name":"GPT-6 Sol","visibility":"list",
         "default_reasoning_level":"unlisted",
         "supported_reasoning_levels":[{"effort":"high","description":"x"}]},
        {"slug":"hidden-model","visibility":"hide","supported_reasoning_levels":[]},
        {"slug":"--evil","visibility":"list","supported_reasoning_levels":[]},
        {"slug":"gpt-6-luna","visibility":"list","supported_reasoning_levels":[]}
    ]}"#;

    const CLAUDE_HELP: &str = "Options:\n  --effort <level>                      Effort level for the current session\n                                        (low, medium, high, xhigh, max)\n  --environment <environment_id>        Create a new cloud session\n";

    fn codex() -> AgentCatalog {
        codex_catalog(
            &captured(0, "Logged in using ChatGPT"),
            captured(0, CODEX_MODELS),
        )
    }

    fn claude() -> AgentCatalog {
        claude_catalog(
            &captured(0, r#"{"loggedIn":true,"email":"a@b.c"}"#),
            captured(0, CLAUDE_HELP),
        )
    }

    #[test]
    fn codex_models_keep_only_listed_safe_unique_entries() {
        let catalog = codex();
        assert_eq!(catalog.state, CatalogState::Available);
        assert_eq!(catalog.provider.as_deref(), Some("openai"));
        let ids: Vec<_> = catalog.models.items.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gpt-6-luna", "gpt-6-sol"]);
        let luna = &catalog.models.items[0];
        assert_eq!(luna.efforts, ["low", "medium", "max"]);
        assert_eq!(luna.default_effort.as_deref(), Some("medium"));
        // A default that the model does not list is reported unknown.
        assert_eq!(catalog.models.items[1].default_effort, None);
        assert_eq!(catalog.efforts.state, CatalogState::Unsupported);
    }

    #[test]
    fn codex_states_never_turn_unreadable_into_empty() {
        let signed_in = captured(0, "Logged in using ChatGPT");
        let garbage = codex_catalog(&signed_in, captured(0, "not json"));
        assert_eq!(garbage.state, CatalogState::Available);
        assert_eq!(garbage.models.state, CatalogState::Unknown);
        assert!(garbage.models.items.is_empty());
        let empty = codex_catalog(&signed_in, captured(0, r#"{"models":[]}"#));
        assert_eq!(empty.models.state, CatalogState::Unknown);
        let failed = codex_catalog(&signed_in, captured(1, CODEX_MODELS));
        assert_eq!(failed.models.state, CatalogState::Unknown);
        let timed_out = codex_catalog(&signed_in, Err(CaptureFailure::TimedOut));
        assert_eq!(timed_out.models.state, CatalogState::Unknown);
    }

    #[test]
    fn codex_without_credentials_offers_nothing() {
        let signed_out = codex_catalog(&captured(1, "Not logged in"), captured(0, CODEX_MODELS));
        assert_eq!(signed_out.state, CatalogState::AuthenticationFailed);
        assert!(signed_out.models.items.is_empty());
        assert_eq!(signed_out.models.state, CatalogState::AuthenticationFailed);
        let unreadable = codex_catalog(&Err(CaptureFailure::TimedOut), captured(0, CODEX_MODELS));
        assert_eq!(unreadable.state, CatalogState::Unknown);
        assert!(unreadable.models.items.is_empty());
        let other_failure = codex_catalog(&captured(2, "boom"), captured(0, CODEX_MODELS));
        assert_eq!(other_failure.state, CatalogState::Unknown);
    }

    #[test]
    fn claude_reports_efforts_but_cannot_enumerate_models() {
        let catalog = claude();
        assert_eq!(catalog.state, CatalogState::Available);
        assert_eq!(catalog.provider.as_deref(), Some("anthropic"));
        assert_eq!(
            catalog.efforts.items,
            ["low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(catalog.models.state, CatalogState::Unsupported);
        assert!(catalog.models.items.is_empty());
    }

    #[test]
    fn claude_states_follow_login_and_help() {
        let signed_in = captured(0, r#"{"loggedIn":true}"#);
        let no_help = claude_catalog(&signed_in, captured(0, "Usage: claude"));
        assert_eq!(no_help.efforts.state, CatalogState::Unknown);
        let out = claude_catalog(
            &captured(1, r#"{"loggedIn":false}"#),
            captured(0, CLAUDE_HELP),
        );
        assert_eq!(out.state, CatalogState::AuthenticationFailed);
        assert!(out.efforts.items.is_empty());
        let garbled = claude_catalog(&captured(0, "{"), captured(0, CLAUDE_HELP));
        assert_eq!(garbled.state, CatalogState::Unknown);
        assert_eq!(parse_claude_logged_in(br#"{"loggedIn":"yes"}"#), None);
    }

    #[test]
    fn only_a_catalog_with_every_facet_known_is_definitive() {
        let timed_out_models = codex_catalog(&captured(0, "ok"), Err(CaptureFailure::TimedOut));
        assert_eq!(timed_out_models.state, CatalogState::Available);
        assert_eq!(timed_out_models.models.state, CatalogState::Unknown);
        assert!(!timed_out_models.is_definitive());
        let no_help = claude_catalog(&captured(0, r#"{"loggedIn":true}"#), captured(0, "Usage"));
        assert!(!no_help.is_definitive());
        assert!(!AgentCatalog::unknown("codex", "still reading").is_definitive());
        assert_eq!(
            AgentCatalog::unknown("codex", "x").provider.as_deref(),
            Some("openai")
        );
        assert!(claude().is_definitive());
        let signed_out = claude_catalog(&captured(1, r#"{"loggedIn":false}"#), captured(0, ""));
        assert!(signed_out.is_definitive());
        assert!(pi_catalog().is_definitive());
    }

    #[test]
    fn claude_effort_parser_rejects_unsafe_levels() {
        assert_eq!(parse_claude_efforts("  --effort <l>  x (low, -rf)"), None);
        assert_eq!(parse_claude_efforts("  --effort <l>  x (Low)"), None);
        assert_eq!(parse_claude_efforts("  --effort <l>  no parentheses"), None);
        assert_eq!(parse_claude_efforts("nothing here"), None);
    }

    #[test]
    fn validation_matrix_for_codex() {
        let catalog = codex();
        assert_eq!(catalog.validate(None, None), Ok(()));
        assert_eq!(catalog.validate(Some("gpt-6-luna"), None), Ok(()));
        assert_eq!(catalog.validate(Some("gpt-6-luna"), Some("low")), Ok(()));
        let reason = |model, effort| catalog.validate(model, effort).unwrap_err().reason;
        assert_eq!(reason(Some("nope"), None), SelectionReason::UnknownModel);
        assert_eq!(
            reason(Some("hidden-model"), None),
            SelectionReason::UnknownModel
        );
        assert_eq!(
            reason(Some("gpt-6-luna"), Some("high")),
            SelectionReason::EffortUnsupported
        );
        assert_eq!(
            reason(None, Some("low")),
            SelectionReason::EffortRequiresModel
        );
        let error = catalog
            .validate(Some("gpt-6-luna"), Some("high"))
            .unwrap_err();
        assert!(
            error.detail.contains("gpt-6-luna") && error.detail.contains("high"),
            "{error}"
        );
    }

    #[test]
    fn validation_matrix_for_claude() {
        let catalog = claude();
        assert_eq!(catalog.validate(None, Some("max")), Ok(()));
        let reason = |model, effort| catalog.validate(model, effort).unwrap_err().reason;
        assert_eq!(
            reason(None, Some("ultra")),
            SelectionReason::EffortUnsupported
        );
        assert_eq!(
            reason(Some("opus"), None),
            SelectionReason::ModelsUnsupported
        );
        assert_eq!(
            reason(Some("opus"), Some("low")),
            SelectionReason::ModelsUnsupported
        );
    }

    #[test]
    fn validation_refuses_every_unestablished_state() {
        let reason = |state, detail: &str| {
            AgentCatalog::uniform("codex", None, state, detail)
                .validate(Some("m"), None)
                .unwrap_err()
                .reason
        };
        assert_eq!(
            reason(CatalogState::Unavailable, ""),
            SelectionReason::AgentUnavailable
        );
        assert_eq!(
            reason(CatalogState::AuthenticationFailed, ""),
            SelectionReason::AuthenticationFailed
        );
        assert_eq!(
            reason(CatalogState::Unknown, ""),
            SelectionReason::AgentStateUnknown
        );
        assert_eq!(
            reason(CatalogState::Unsupported, ""),
            SelectionReason::AgentUnsupported
        );
        let pi = pi_catalog();
        assert_eq!(pi.state, CatalogState::Unsupported);
        assert_eq!(
            pi.validate(None, Some("low")).unwrap_err().reason,
            SelectionReason::AgentUnsupported
        );
        // Unknown model lists are not an empty list.
        let signed_in = captured(0, "ok");
        let unknown = codex_catalog(&signed_in, captured(0, "{"));
        assert_eq!(
            unknown
                .validate(Some("gpt-6-luna"), None)
                .unwrap_err()
                .reason,
            SelectionReason::ModelsUnknown
        );
    }

    #[test]
    fn selection_args_build_exact_vectors() {
        assert_eq!(
            CodexAdapter::selection_args(Some("gpt-6-luna"), Some("low")),
            ["-m", "gpt-6-luna", "-c", "model_reasoning_effort=\"low\""]
        );
        assert!(CodexAdapter::selection_args(None, None).is_empty());
        let args = ClaudeCodeAdapter::args_for_prompt_with_selection("hi", None, Some("max"));
        assert_eq!(&args[args.len() - 2..], ["--effort", "max"]);
        assert_eq!(
            ClaudeCodeAdapter::args_for_prompt_with_selection("hi", None, None),
            ClaudeCodeAdapter::args_for_prompt("hi")
        );
    }

    #[test]
    fn conversation_args_are_read_only_and_keep_the_prompt_whole() {
        let prompt = "line one\n--dangerously; $(rm -rf /)";
        let codex = CodexAdapter::args_for_conversation(prompt, Some("m"), Some("low"));
        assert_eq!(codex.last().map(String::as_str), Some(prompt));
        assert_eq!(codex[codex.len() - 2], "--");
        for flag in ["--json", "--ephemeral", "--skip-git-repo-check"] {
            assert!(codex.iter().any(|arg| arg == flag), "{flag}");
        }
        let sandbox = codex.iter().position(|arg| arg == "--sandbox").unwrap();
        assert_eq!(codex[sandbox + 1], "read-only");
        assert!(codex.iter().any(|arg| arg == "-m"));

        let claude = ClaudeCodeAdapter::args_for_conversation(prompt, None, Some("max"));
        assert_eq!(&claude[..2], ["-p", prompt]);
        let tools = claude.iter().position(|arg| arg == "--tools").unwrap();
        assert_eq!(claude[tools + 1], "");
        assert!(claude.iter().any(|arg| arg == "--no-session-persistence"));
        assert!(!claude.iter().any(|arg| arg == "--allowedTools"));
        assert_eq!(&claude[claude.len() - 2..], ["--effort", "max"]);
    }

    #[test]
    fn catalog_serializes_snake_case_states() {
        let value = serde_json::to_value(codex()).unwrap();
        assert_eq!(value["state"], "available");
        assert_eq!(value["efforts"]["state"], "unsupported");
        let value = serde_json::to_value(pi_catalog()).unwrap();
        assert_eq!(value["provider"], Value::Null);
    }

    #[cfg(unix)]
    mod unix_probes {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn script(dir: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
            let path = dir.path().join("fake-cli");
            std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        #[tokio::test]
        async fn codex_catalog_reads_a_large_single_line_catalog() {
            let dir = tempfile::tempdir().unwrap();
            let catalog = dir.path().join("models.json");
            let padding = "x".repeat(200_000);
            let models = CODEX_MODELS.replace(
                "\"hide\"",
                &format!("\"hide\",\"base_instructions\":\"{padding}\""),
            );
            std::fs::write(&catalog, models).unwrap();
            let program = script(
                &dir,
                &format!(
                    "case \"$1\" in login) echo 'Logged in'; exit 0;; debug) cat '{}'; exit 0;; esac\nexit 9\n",
                    catalog.display()
                ),
            );
            let result = CodexAdapter::new()
                .with_program(&program)
                .catalog(dir.path(), Duration::from_secs(10))
                .await;
            assert_eq!(result.models.state, CatalogState::Available);
            assert_eq!(result.models.items.len(), 2);
        }

        #[tokio::test]
        async fn probes_are_bounded_and_missing_binaries_are_unavailable() {
            let dir = tempfile::tempdir().unwrap();
            let slow = script(&dir, "sleep 30\n");
            let started = std::time::Instant::now();
            let result = ClaudeCodeAdapter::new()
                .with_program(&slow)
                .catalog(dir.path(), Duration::from_millis(300))
                .await;
            assert!(started.elapsed() < Duration::from_secs(10));
            assert_eq!(result.state, CatalogState::Unknown);
            let missing = CodexAdapter::new()
                .with_program(dir.path().join("absent"))
                .catalog(dir.path(), Duration::from_secs(1))
                .await;
            assert_eq!(missing.state, CatalogState::Unavailable);
        }

        /// Waits for every process whose pid the fake CLI recorded to exit,
        /// stopping any that did not so the test never leaks them.
        fn all_recorded_processes_exit(path: &std::path::Path) -> bool {
            let recorded = std::fs::read_to_string(path)
                .expect("the fake CLI records its background processes");
            let pids: Vec<i32> = recorded
                .lines()
                .map(|pid| pid.trim().parse().unwrap())
                .collect();
            assert!(!pids.is_empty());
            let mut all_exited = true;
            for pid in pids {
                let mut exited = false;
                for _ in 0..100 {
                    // SAFETY: signal 0 only checks that the process exists.
                    if unsafe { libc::kill(pid, 0) } != 0 {
                        exited = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                if !exited {
                    all_exited = false;
                    // SAFETY: stops the leaked fake process this test started.
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            }
            all_exited
        }

        #[tokio::test]
        async fn a_timed_out_probe_leaves_no_descendant_running() {
            let dir = tempfile::tempdir().unwrap();
            let pid_file = dir.path().join("background.pids");
            let program = script(
                &dir,
                &format!("sleep 300 &\necho $! >> '{}'\nwait\n", pid_file.display()),
            );
            let result = ClaudeCodeAdapter::new()
                .with_program(&program)
                .catalog(dir.path(), Duration::from_secs(3))
                .await;
            assert_eq!(result.state, CatalogState::Unknown);
            assert!(
                all_recorded_processes_exit(&pid_file),
                "a probe's background process survived the timeout"
            );
        }

        #[tokio::test]
        async fn a_background_process_holding_the_pipe_does_not_stall_a_finished_probe() {
            let dir = tempfile::tempdir().unwrap();
            let pid_file = dir.path().join("background.pids");
            let help = dir.path().join("help.txt");
            std::fs::write(
                &help,
                "Options:\n  --effort <level>  x\n                    (low, high)\n",
            )
            .unwrap();
            let program = script(
                &dir,
                &format!(
                    "case \"$1\" in\n\
                     auth) echo '{{\"loggedIn\":true}}'; exit 0;;\n\
                     --help) sleep 300 &\n echo $! >> '{pid}'\n cat '{help}'; exit 0;;\n\
                     esac\n",
                    pid = pid_file.display(),
                    help = help.display(),
                ),
            );
            let started = std::time::Instant::now();
            let result = ClaudeCodeAdapter::new()
                .with_program(&program)
                .catalog(dir.path(), Duration::from_secs(60))
                .await;
            let elapsed = started.elapsed();
            assert_eq!(result.efforts.state, CatalogState::Available);
            assert_eq!(result.efforts.items, ["low", "high"]);
            assert!(
                all_recorded_processes_exit(&pid_file),
                "a finished probe's background process survived"
            );
            assert!(
                elapsed < Duration::from_secs(30),
                "the read waited for the background process"
            );
        }

        #[tokio::test]
        async fn oversized_output_is_unknown_not_truncated() {
            let dir = tempfile::tempdir().unwrap();
            let program = script(&dir, "case \"$1\" in login) exit 0;; debug) head -c 9000000 /dev/zero | tr '\\0' 'a'; exit 0;; esac\n");
            let result = CodexAdapter::new()
                .with_program(&program)
                .catalog(dir.path(), Duration::from_secs(20))
                .await;
            assert_eq!(result.models.state, CatalogState::Unknown);
        }
    }
}
