//! The agent-backed conversation runtime (M7-25).
//!
//! Plan: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`
//! section 11. The design record is `docs/m7-08-conversation-service.md`.
//!
//! [`AgentRuntime`] answers one conversation turn by launching an installed
//! agent CLI (Codex, then Claude Code) through the same adapters tasks use.
//! The agent is picked from the M7-09 catalog: it must be installed, signed in
//! and launchable, and an optional configured model and effort must be offered
//! by its catalog. A runtime that is missing or not launchable never receives
//! a message; the turn fails closed with `runtime_unavailable`.
//!
//! A conversation has no authority. The agent runs read-only in an empty
//! scratch directory with no tools (Claude Code) or a read-only sandbox
//! (Codex), is given only the transcript, and its output is text that is
//! stored as a reply. Nothing it says is parsed as a command, and this module
//! has no store handle and calls no task, approval or delivery code.
//!
//! Bounds: one turn lives at most `turn_timeout`; at most [`MAX_RUNNING_TURNS`]
//! turns run at once; the reply is capped by the sink; control characters are
//! removed from everything that reaches the transcript or the prompt. The
//! child is launched by the shared process supervisor, which kills the whole
//! process group on Unix and the whole Job Object on Windows.
//!
//! Selection is resolved by [`AgentRuntime::prepare`], called before the
//! daemon takes its request gate, because reading a catalog can take a second.
//! [`ConversationRuntime::identity`] only reads the result.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orchestrator_agents::{
    AgentAdapter, AgentCatalog, AgentEvent, AgentExit, AgentLaunchRequest, CatalogState,
    ClaudeCodeAdapter, CodexAdapter,
};
use orchestrator_core::workstream::Workstream;
use orchestrator_ipc::RpcError;
use orchestrator_store::{ConversationTarget, SqliteStore, WorkstreamStoreError};

use super::conversation_runtime::{
    ConversationRuntime, HistoryTurn, ReplySink, RuntimeFailure, RuntimeIdentity, TurnControl,
    TurnRequest, WorkstreamDirectory, UNAVAILABLE_RUNTIME_NAME,
};
use super::models::CatalogCache;
use crate::executor::{log_safe, ExecutorConfig};

pub(crate) const CODEX: &str = "codex";
pub(crate) const CLAUDE: &str = "claude-code";

/// Versions of the agent CLIs this runtime has been proven against. A CLI at
/// any other version is refused (`version_mismatch`) until an operator adds it
/// to `AGENT_ORCH_CONVERSATION_PINS`.
const KNOWN_PINS: [(&str, &str); 2] = [(CODEX, "0.167.0"), (CLAUDE, "2.1.311")];

const AGENT_ENV: &str = "AGENT_ORCH_CONVERSATION_AGENT";
const MODEL_ENV: &str = "AGENT_ORCH_CONVERSATION_MODEL";
const EFFORT_ENV: &str = "AGENT_ORCH_CONVERSATION_EFFORT";
const PINS_ENV: &str = "AGENT_ORCH_CONVERSATION_PINS";
const TIMEOUT_MS_ENV: &str = "AGENT_ORCH_CONVERSATION_TIMEOUT_MS";

const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(180);
const MIN_TURN_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_TURN_TIMEOUT: Duration = Duration::from_secs(600);
/// Turns running at once across both conversation scopes.
const MAX_RUNNING_TURNS: usize = 4;
/// How long a resolved selection (and the version read with it) is reused.
const SELECTION_TTL: Duration = Duration::from_secs(30);
/// Leaves headroom under the 2 s client I/O timeout after the catalog wait.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_VERSION_LEN: usize = 64;
/// How often a running turn looks at its cancel flag and deadline.
const POLL: Duration = Duration::from_millis(50);
const MAX_CONTEXT_BYTES: usize = 2 * 1024;

/// Marks where the operator's current message starts in the prompt. The last
/// occurrence wins, so a transcript that quotes it cannot move the boundary.
pub(crate) const CURRENT_MESSAGE_MARKER: &str = "Current message:";

/// Settings that decide which agent answers and how long it may take.
#[derive(Clone)]
pub(crate) struct ConversationConfig {
    /// Restricts selection to one agent. Unset means Codex, then Claude Code.
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Pins added to [`KNOWN_PINS`].
    pub extra_pins: Vec<RuntimeIdentity>,
    pub turn_timeout: Duration,
    /// Empty directory the agent runs in; it never sees a repository.
    pub scratch: PathBuf,
}

impl ConversationConfig {
    pub(crate) fn for_data_dir(data_dir: &Path) -> Self {
        let text = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let turn_timeout = text(TIMEOUT_MS_ENV)
            .and_then(|value| value.parse::<u64>().ok())
            .map_or(DEFAULT_TURN_TIMEOUT, Duration::from_millis)
            .clamp(MIN_TURN_TIMEOUT, MAX_TURN_TIMEOUT);
        Self {
            agent: text(AGENT_ENV),
            model: text(MODEL_ENV),
            effort: text(EFFORT_ENV),
            extra_pins: text(PINS_ENV).map_or_else(Vec::new, |value| parse_pins(&value)),
            turn_timeout,
            scratch: data_dir.join("conversation-scratch"),
        }
    }
}

/// `name@version` entries, comma separated. Malformed entries are ignored.
fn parse_pins(text: &str) -> Vec<RuntimeIdentity> {
    text.split(',')
        .filter_map(|entry| {
            let (name, version) = entry.trim().split_once('@')?;
            let known = name == CODEX || name == CLAUDE;
            (known && valid_version(version)).then(|| RuntimeIdentity {
                name: name.to_owned(),
                version: version.to_owned(),
            })
        })
        .collect()
}

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= MAX_VERSION_LEN
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '_' | '-'))
}

/// The version in `--version` output: `codex-cli 0.167.0` and
/// `2.1.311 (Claude Code)` both yield the first token that starts with a digit.
fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))
        .filter(|token| valid_version(token))
        .map(str::to_owned)
}

/// An agent chosen for conversations.
#[derive(Clone)]
struct Selected {
    agent: &'static str,
    program: PathBuf,
    identity: RuntimeIdentity,
    model: Option<String>,
    effort: Option<String>,
}

#[derive(Clone)]
enum Selection {
    Ready(Selected),
    /// No agent can answer. Every turn fails with this code.
    Missing(RuntimeFailure),
}

pub(crate) struct AgentRuntime {
    exec: ExecutorConfig,
    cache: Arc<CatalogCache>,
    config: ConversationConfig,
    selection: Mutex<Option<(Instant, Selection)>>,
    running: Arc<AtomicUsize>,
}

impl AgentRuntime {
    pub(crate) fn new(
        exec: ExecutorConfig,
        cache: Arc<CatalogCache>,
        config: ConversationConfig,
    ) -> Self {
        Self {
            exec,
            cache,
            config,
            selection: Mutex::new(None),
            running: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Every identity a selected agent may report.
    pub(crate) fn pins(&self) -> Vec<RuntimeIdentity> {
        KNOWN_PINS
            .iter()
            .map(|(name, version)| RuntimeIdentity {
                name: (*name).to_owned(),
                version: (*version).to_owned(),
            })
            .chain(self.config.extra_pins.iter().cloned())
            .collect()
    }

    fn current(&self) -> Option<Selection> {
        self.selection
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .map(|(_, selection)| selection.clone())
    }

    fn fresh(&self) -> bool {
        self.selection
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < SELECTION_TTL)
    }

    fn program_of(&self, agent: &str) -> PathBuf {
        let configured = match agent {
            CODEX => self.exec.codex_program.clone(),
            _ => self.exec.claude_program.clone(),
        };
        configured.unwrap_or_else(|| PathBuf::from(if agent == CODEX { "codex" } else { "claude" }))
    }

    /// Picks the first agent that is installed, signed in, launchable and
    /// offers the configured selection. `Conflict` means the state of an agent
    /// could not be established in time: nothing is recorded, so the caller
    /// retries with the same idempotency key. An agent that is simply absent
    /// is not an error here; it becomes `Missing` and fails the turn closed.
    fn resolve(&self) -> Result<Selection, RpcError> {
        let agents: Vec<&'static str> = match self.config.agent.as_deref() {
            None => vec![CODEX, CLAUDE],
            Some(CODEX) => vec![CODEX],
            Some(CLAUDE) => vec![CLAUDE],
            Some(other) => {
                eprintln!(
                    "orchestrator-daemon: conversation agent {} is not supported",
                    log_safe(other)
                );
                return Ok(Selection::Missing(RuntimeFailure::Unavailable));
            }
        };
        let names: Vec<&str> = agents.to_vec();
        let catalogs = self.cache.read(&self.exec, &names);
        let wants_selection = self.config.model.is_some() || self.config.effort.is_some();
        let mut transient = false;
        let mut missing = RuntimeFailure::Unavailable;
        for (agent, catalog) in agents.iter().zip(&catalogs) {
            match catalog.state {
                CatalogState::Available => {}
                CatalogState::Unknown => {
                    transient = true;
                    continue;
                }
                CatalogState::AuthenticationFailed => {
                    missing = RuntimeFailure::CredentialsMissing;
                    continue;
                }
                CatalogState::Unavailable | CatalogState::Unsupported => continue,
            }
            if wants_selection && !self.selection_offered(catalog, &mut transient) {
                continue;
            }
            if !self.launchable(agent) {
                continue;
            }
            let program = self.program_of(agent);
            let Some(version) = read_version(&program) else {
                transient = true;
                continue;
            };
            return Ok(Selection::Ready(Selected {
                agent,
                program,
                identity: RuntimeIdentity {
                    name: (*agent).to_owned(),
                    version,
                },
                model: self.config.model.clone(),
                effort: self.config.effort.clone(),
            }));
        }
        if transient {
            Err(RpcError::Conflict)
        } else {
            Ok(Selection::Missing(missing))
        }
    }

    fn selection_offered(&self, catalog: &AgentCatalog, transient: &mut bool) -> bool {
        if !catalog.is_definitive() {
            *transient = true;
            return false;
        }
        match catalog.validate(self.config.model.as_deref(), self.config.effort.as_deref()) {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "orchestrator-daemon: conversation selection refused: {}",
                    log_safe(&error.to_string())
                );
                false
            }
        }
    }

    /// The adapter can detect the executable and streams structured output
    /// with cancellation, which a bounded turn depends on.
    fn launchable(&self, agent: &str) -> bool {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return false;
        };
        let program = self.program_of(agent);
        let capabilities = runtime.block_on(async {
            if agent == CODEX {
                CodexAdapter::new().with_program(program).detect().await
            } else {
                ClaudeCodeAdapter::new()
                    .with_program(program)
                    .detect()
                    .await
            }
        });
        capabilities
            .map(|caps| caps.streaming && caps.structured_output && caps.cancellation)
            .unwrap_or(false)
    }
}

impl ConversationRuntime for AgentRuntime {
    fn identity(&self) -> RuntimeIdentity {
        match self.current() {
            Some(Selection::Ready(selected)) => selected.identity,
            _ => RuntimeIdentity {
                name: UNAVAILABLE_RUNTIME_NAME.to_owned(),
                version: "none".to_owned(),
            },
        }
    }

    /// Only a selected agent produces output and so needs a pin.
    fn requires_pin(&self) -> bool {
        matches!(self.current(), Some(Selection::Ready(_)))
    }

    fn prepare(&self) -> Result<(), RpcError> {
        if self.fresh() {
            return Ok(());
        }
        let selection = self.resolve()?;
        *self
            .selection
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some((Instant::now(), selection));
        Ok(())
    }

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure> {
        let selected = match self.current() {
            Some(Selection::Ready(selected)) => selected,
            Some(Selection::Missing(failure)) => return Err(failure),
            None => return Err(RuntimeFailure::Unavailable),
        };
        // The service pinned `request.identity`; a selection that changed since
        // then must not receive the message.
        if selected.identity != request.identity {
            return Err(RuntimeFailure::VersionMismatch);
        }
        let slot = Slot::take(&self.running).ok_or(RuntimeFailure::Busy)?;
        std::fs::create_dir_all(&self.config.scratch).map_err(|_| RuntimeFailure::Failed)?;
        let prompt = build_prompt(&request);
        let launch = AgentLaunchRequest {
            program: selected.program.clone(),
            args: conversation_args(&selected, &prompt),
            working_dir: self.config.scratch.clone(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let turn = Turn {
            agent: selected.agent,
            launch,
            timeout: self.config.turn_timeout,
            cancelled: cancelled.clone(),
        };
        let thread_sink = sink.clone();
        std::thread::Builder::new()
            .name("conversation-turn".into())
            .spawn(move || {
                let _slot = slot;
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_turn(turn, &thread_sink);
                }));
                // A turn the runtime leaves open can never finish: close it.
                // A no-op when the turn already ended.
                thread_sink.fail(RuntimeFailure::Failed);
            })
            .map_err(|_| RuntimeFailure::Failed)?;
        Ok(Box::new(Control(cancelled)))
    }
}

/// Holds one of the [`MAX_RUNNING_TURNS`] places until dropped.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(running: &Arc<AtomicUsize>) -> Option<Self> {
        let taken = running.fetch_add(1, Ordering::AcqRel);
        if taken >= MAX_RUNNING_TURNS {
            running.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self(running.clone()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Control(Arc<AtomicBool>);

impl TurnControl for Control {
    /// The turn thread sees the flag within [`POLL`] and kills the process.
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

fn conversation_args(selected: &Selected, prompt: &str) -> Vec<String> {
    let (model, effort) = (selected.model.as_deref(), selected.effort.as_deref());
    if selected.agent == CODEX {
        CodexAdapter::args_for_conversation(prompt, model, effort)
    } else {
        ClaudeCodeAdapter::args_for_conversation(prompt, model, effort)
    }
}

/// `--version` of the agent, read with a hard time bound. `None` when it
/// cannot be run or does not print a version. The child leads its own process
/// group on Unix so a wrapper script cannot outlive the bound.
fn read_version(program: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + VERSION_PROBE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                kill_probe(&mut child);
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut output = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(child.stdout.take()?, 1024),
        &mut output,
    )
    .ok()?;
    parse_version(&String::from_utf8_lossy(&output))
}

fn kill_probe(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Ok(leader) = i32::try_from(child.id()) {
        // SAFETY: signals only the group this probe created.
        let _ = unsafe { libc::kill(-leader, libc::SIGKILL) };
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Removes everything that could forge or hide transcript text: control
/// characters other than newline and tab, and bidirectional overrides. A
/// carriage return is dropped so a reply cannot overwrite its own line.
pub(crate) fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| match c {
            '\n' | '\t' => true,
            c if c.is_control() => false,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' => false,
            _ => true,
        })
        .collect()
}

/// The text sent to the agent: a fixed frame that states it has no authority,
/// the workstream context, recent exchanges, then the current message.
pub(crate) fn build_prompt(request: &TurnRequest) -> String {
    let mut prompt = String::from(
        "You are the orchestrator assistant in the agent-orch desktop app. Reply to the \
         operator in plain text. You can only talk: you cannot start, change or cancel tasks, \
         approve anything, deliver anything or modify files, and nothing you write is executed. \
         If the operator wants an action, tell them which part of the app performs it.\n\n",
    );
    match &request.target {
        ConversationTarget::Global => prompt.push_str("Scope: the global orchestrator.\n"),
        ConversationTarget::Workstream(_) => {
            prompt.push_str("Scope: one workstream.\n");
            if let Some(context) = &request.context {
                prompt.push_str("Workstream context: ");
                prompt.push_str(&sanitize(context).replace('\n', " "));
                prompt.push('\n');
            }
        }
    }
    if !request.history.is_empty() {
        prompt.push_str("\nEarlier in this conversation:\n");
        for HistoryTurn { user, reply } in &request.history {
            prompt.push_str("Operator: ");
            prompt.push_str(&sanitize(user));
            prompt.push_str("\nAssistant: ");
            prompt.push_str(&sanitize(reply));
            prompt.push('\n');
        }
    }
    prompt.push('\n');
    prompt.push_str(CURRENT_MESSAGE_MARKER);
    prompt.push('\n');
    prompt.push_str(&sanitize(&request.text));
    prompt
}

struct Turn {
    agent: &'static str,
    launch: AgentLaunchRequest,
    timeout: Duration,
    cancelled: Arc<AtomicBool>,
}

/// Drives one agent process to the end of the turn and reports through `sink`.
/// A closed sink (cancelled, too large) or a set cancel flag stops the agent.
fn run_turn(turn: Turn, sink: &ReplySink) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        sink.fail(RuntimeFailure::Failed);
        return;
    };
    runtime.block_on(drive(turn, sink));
}

async fn drive(turn: Turn, sink: &ReplySink) {
    let spawned = if turn.agent == CODEX {
        CodexAdapter::new().spawn(turn.launch).await
    } else {
        ClaudeCodeAdapter::new().spawn(turn.launch).await
    };
    let mut handle = match spawned {
        Ok(handle) => handle,
        Err(error) => {
            let text = error.to_string();
            eprintln!(
                "orchestrator-daemon: conversation launch failed: {}",
                log_safe(&text)
            );
            let denied = text.contains("ermission denied") || text.contains("ccess is denied");
            sink.fail(if denied {
                RuntimeFailure::PermissionDenied
            } else {
                RuntimeFailure::Unavailable
            });
            return;
        }
    };
    let deadline = Instant::now() + turn.timeout;
    let mut messages = 0usize;
    loop {
        if turn.cancelled.load(Ordering::Acquire) {
            let _ = handle.cancel().await;
            return;
        }
        if Instant::now() >= deadline {
            let _ = handle.cancel().await;
            sink.fail(RuntimeFailure::Timeout);
            return;
        }
        let event = match tokio::time::timeout(POLL, handle.next_event()).await {
            Err(_) => continue,
            Ok(Ok(Some(event))) => event,
            Ok(Ok(None)) => break,
            Ok(Err(_)) => {
                let _ = handle.cancel().await;
                sink.fail(RuntimeFailure::Failed);
                return;
            }
        };
        match event {
            AgentEvent::MessageDelta { text } => {
                let mut piece = sanitize(&text);
                // Codex reports whole messages; Claude Code reports fragments.
                if turn.agent == CODEX && messages > 0 && !piece.is_empty() {
                    piece.insert_str(0, "\n\n");
                }
                if !piece.is_empty() {
                    messages += 1;
                    if sink.delta(&piece).is_err() {
                        let _ = handle.cancel().await;
                        return;
                    }
                }
            }
            AgentEvent::SessionExited { .. } => break,
            AgentEvent::ProtocolError { .. } => {
                let _ = handle.cancel().await;
                sink.fail(RuntimeFailure::Failed);
                return;
            }
            _ => {}
        }
    }
    match handle.wait().await {
        Ok(AgentExit::Completed { .. }) => sink.complete(),
        Ok(AgentExit::Cancelled { .. }) => {}
        _ => sink.fail(RuntimeFailure::Failed),
    }
}

/// Workstream lookup backed by the M7-07 store.
pub(crate) struct StoreWorkstreams(pub(crate) Arc<SqliteStore>);

impl WorkstreamDirectory for StoreWorkstreams {
    fn exists(&self, workstream_id: &str) -> Result<bool, RpcError> {
        match self.0.get_workstream(workstream_id) {
            Ok(_) => Ok(true),
            Err(WorkstreamStoreError::NotFound { .. }) => Ok(false),
            Err(_) => Err(RpcError::Internal),
        }
    }

    fn describe(&self, workstream_id: &str) -> Option<String> {
        let Workstream {
            title,
            summary,
            state,
            ..
        } = self.0.get_workstream(workstream_id).ok()?;
        let mut text = format!("{title} (state: {}). {summary}", state.tag());
        let mut end = text.len().min(MAX_CONTEXT_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str, history: Vec<HistoryTurn>) -> TurnRequest {
        TurnRequest {
            target: ConversationTarget::Global,
            text: text.into(),
            identity: RuntimeIdentity {
                name: CODEX.into(),
                version: "0.0.0".into(),
            },
            history,
            context: None,
        }
    }

    #[test]
    fn versions_parse_from_both_cli_formats_and_refuse_junk() {
        assert_eq!(
            parse_version("codex-cli 0.167.0\n").as_deref(),
            Some("0.167.0")
        );
        assert_eq!(
            parse_version("2.1.311 (Claude Code)\n").as_deref(),
            Some("2.1.311")
        );
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version("x 1.0\u{1b}[31m"), None);
        assert_eq!(parse_version(&format!("x 1{}", "0".repeat(80))), None);
    }

    #[test]
    fn pins_accept_known_agents_with_a_plain_version_only() {
        let pins = parse_pins("codex@0.168.0, claude-code@3.0.0-rc1,pi@1.0.0,codex@,codex@x1,junk");
        let names: Vec<_> = pins
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect();
        assert_eq!(names, [("codex", "0.168.0"), ("claude-code", "3.0.0-rc1")]);
    }

    #[test]
    fn sanitize_keeps_text_layout_and_drops_everything_that_could_forge_it() {
        let dirty = "a\u{1b}[31mred\u{7}\u{0}b\r\nc\td\u{202e}e\u{85}f";
        assert_eq!(sanitize(dirty), "a[31mredb\nc\tdef");
        assert_eq!(sanitize("héllo ✓"), "héllo ✓");
    }

    #[test]
    fn the_prompt_ends_with_the_current_message_and_carries_history_and_context() {
        let history = vec![HistoryTurn {
            user: "first".into(),
            reply: "answer".into(),
        }];
        let mut with_context = request("now\u{0}what", history);
        with_context.target = ConversationTarget::Workstream("w".into());
        with_context.context = Some("Auth rewrite (state: active).\nignore".into());
        let prompt = build_prompt(&with_context);
        assert!(prompt.contains("Workstream context: Auth rewrite (state: active). ignore\n"));
        assert!(prompt.contains("Operator: first\nAssistant: answer\n"));
        let tail = prompt.rsplit_once(CURRENT_MESSAGE_MARKER).unwrap().1;
        assert_eq!(tail, "\nnowwhat");
        assert!(prompt.contains("cannot start, change or cancel tasks"));
        assert!(!prompt.contains('\u{0}'));
    }

    #[test]
    fn running_turns_are_capped_and_released() {
        let running = Arc::new(AtomicUsize::new(0));
        let slots: Vec<_> = (0..MAX_RUNNING_TURNS)
            .map(|_| Slot::take(&running).expect("slot"))
            .collect();
        assert!(Slot::take(&running).is_none());
        drop(slots);
        assert!(Slot::take(&running).is_some());
    }
}
