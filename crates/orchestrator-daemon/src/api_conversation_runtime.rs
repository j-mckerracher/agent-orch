//! Runtime boundary and turn supervision for orchestrator conversations (M7-08).
//!
//! Plan: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`
//! section 11. The design record is `docs/m7-08-conversation-service.md`.
//!
//! The daemon owns the transcript, turn lifecycle, cancellation and recovery.
//! A [`ConversationRuntime`] only produces reply text for one turn through a
//! [`ReplySink`]. It receives no store handle, no task, approval or delivery
//! capability, and nothing it emits is run as a command. The one exception is
//! a trailing action block in a completed reply, on a session that negotiated
//! `orchestrator_actions`: it becomes pending proposals (DPES-1105), which do
//! nothing until the user approves them.
//!
//! No provider is bound here. The daemon binds the agent runtime of
//! `api_conversation_agent.rs` (M7-25), which fails every turn with
//! `runtime_unavailable` when no launchable agent is installed. A runtime that
//! produces output must match one of the identities [`ConversationService`]
//! was given (an exact name and version); a runtime that does not match never
//! receives a message.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use orchestrator_agents::{SelectionReason, UsageReport};
use orchestrator_core::orchestrator_action::{
    parse_block, Action, ActionBlock, BlockScope, HeldBlock, ReplyFilter,
};
use orchestrator_core::permission::{PermissionLevel, PERMISSION_LEVEL_UNSUPPORTED};
use orchestrator_ipc::RpcError;
use orchestrator_store::{
    BegunTurn, ConversationError, ConversationTarget, ConversationTurn, ConversationUsageReceipt,
    SqliteStore, TurnBlock, TurnOutcome, TurnPermission, TurnState, TurnTrigger,
    MAX_REPLY_CHUNK_BYTES,
};

use super::actions::promotion_denial;
use super::conversation_snapshot::build_snapshot_with_notes;
use super::home_directory::with_in_place_delivery;
use super::landing_api::{merge_denial, with_project_merge_methods};
use super::load::LoadHandle;
use super::projects_defaults::{denial_of, revise_denial};
use super::wake_events::redact_snapshot;
use crate::executor::unix_now_ms;
use crate::mcp_server::McpView;

/// A wake turn that has nothing for the user replies with exactly this (DPES-1105).
pub(crate) const QUIET_TOKEN: &str = "MCAO_QUIET";
/// Marks a wake exchange in the history the next turn is shown.
const WAKE_HISTORY_PREFIX: &str = "[automatic update] ";
/// Most event lines, and bytes per line, in the opening text of a wake turn.
const WAKE_BRIEF_LINES: usize = 20;
const WAKE_BRIEF_LINE_BYTES: usize = 200;

/// Name reported while no agent is selected.
pub(crate) const UNAVAILABLE_RUNTIME_NAME: &str = "unavailable";

/// Who produced replies. Recorded on every turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeIdentity {
    pub name: String,
    pub version: String,
}

/// A model and effort chosen in the picker for one message (DPES-1105). The
/// service passes `None` when the message carries no choice.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Choice {
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// One earlier, completed exchange of the same conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryTurn {
    pub user: String,
    pub reply: String,
}

/// The events a wake turn reports: the daemon's own notice lines, never agent text.
#[derive(Clone)]
pub(crate) struct WakeBrief {
    pub events: Vec<String>,
    /// Every claimed record is a heartbeat (a work check or a daemon start), so the model is
    /// offered the quiet token. A batch with an outcome is always answered in words.
    pub may_be_quiet: bool,
}

/// The one block of agent text a turn is handed, inside its delimiters: a scout report's relay
/// or a candidate's summary.
pub(crate) struct Evidence {
    pub block: String,
    /// A scout report. The turn is marked as one that read a report, so a proposal says so.
    pub is_report: bool,
}

impl WakeBrief {
    /// The opening entry of the turn: the lines, at most 20 of at most 200 bytes.
    fn opening_text(&self) -> String {
        self.events
            .iter()
            .take(WAKE_BRIEF_LINES)
            .map(|line| truncate_to(line, WAKE_BRIEF_LINE_BYTES))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// What a runtime is asked to answer.
pub(crate) struct TurnRequest {
    pub target: ConversationTarget,
    pub text: String,
    /// The identity the service pinned and recorded for this turn. A runtime
    /// launches only this one.
    pub identity: RuntimeIdentity,
    /// The choice the message was sent with, or `None` for the configured default.
    pub choice: Option<Choice>,
    /// The permission level the message was sent with (DPES-1105).
    pub level: PermissionLevel,
    /// Recent completed exchanges, oldest first, bounded in count and size.
    /// Failed and canceled turns are not included.
    pub history: Vec<HistoryTurn>,
    /// The app state when the turn started, built by the service from the
    /// store (DPES-1105). Data for the agent, never instructions. `None` only
    /// in tests.
    pub context: Option<String>,
    /// The session negotiated `orchestrator_actions`, so the agent is told the
    /// action block format (DPES-1105).
    pub actions: bool,
    /// Set only for a wake turn (DPES-1105): the prompt then says the message is an
    /// automatic update.
    pub wake: Option<WakeBrief>,
    /// Set only when the read-only tools may be offered to this turn (see [`turn_mcp`]).
    /// The runtime starts the server only for Claude when its CLI offers the flag, and
    /// clears the field when it did not start one. The view has no write method.
    pub mcp: Option<Arc<McpView>>,
}

/// Why a turn could not be answered. Maps to a stable code; runtime-supplied
/// text is never stored or returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeFailure {
    Unavailable,
    CredentialsMissing,
    PermissionDenied,
    /// The installed agent's help no longer lists a flag this chat needs at its
    /// permission level. The flag is the detail, a daemon-chosen token.
    MissingFlag(String),
    /// No pins are configured, so no selected agent may receive a message.
    Unpinned,
    /// The selected agent changed between the pin check and the launch. The
    /// message is not sent; a new message gets the current selection.
    RuntimeChanged,
    Failed,
    /// The agent's provider refused the turn for a spend, usage, rate or quota limit.
    /// The agent's own text decides it (DPES-1106); only the code is stored.
    ProviderLimit,
    Timeout,
    Busy,
    /// The catalog refused the message's choice; the reason sets the code.
    Refused(SelectionReason),
    /// The agent has no mapping for the message's permission level.
    LevelUnsupported,
}

impl RuntimeFailure {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "runtime_unavailable",
            Self::CredentialsMissing => "credentials_missing",
            Self::PermissionDenied => "permission_denied",
            Self::MissingFlag(_) => "version_mismatch",
            Self::Unpinned => "runtime_unpinned",
            Self::RuntimeChanged => "runtime_changed",
            Self::Failed => "runtime_failed",
            Self::ProviderLimit => "runtime_provider_limit",
            Self::Timeout => "runtime_timeout",
            Self::Busy => "runtime_busy",
            Self::Refused(reason) => refusal_code(*reason),
            Self::LevelUnsupported => PERMISSION_LEVEL_UNSUPPORTED,
        }
    }

    /// The body of the terminal entry: a daemon-chosen token, or empty.
    pub(crate) fn detail(&self) -> String {
        match self {
            Self::MissingFlag(flag) => flag.clone(),
            Self::ProviderLimit => "provider_limit".to_owned(),
            _ => String::new(),
        }
    }
}

/// The stable code of a choice the catalog refused: one per reason, so the
/// desktop can explain it without reading text.
fn refusal_code(reason: SelectionReason) -> &'static str {
    match reason {
        SelectionReason::AgentUnavailable => "selection_agent_unavailable",
        SelectionReason::AuthenticationFailed => "selection_authentication_failed",
        SelectionReason::AgentStateUnknown => "selection_agent_state_unknown",
        SelectionReason::AgentUnsupported => "selection_agent_unsupported",
        SelectionReason::ModelsUnsupported => "selection_models_unsupported",
        SelectionReason::ModelsUnknown => "selection_models_unknown",
        SelectionReason::UnknownModel => "selection_unknown_model",
        SelectionReason::EffortRequiresModel => "selection_effort_requires_model",
        SelectionReason::EffortUnsupported => "selection_effort_unsupported",
        SelectionReason::EffortUnknown => "selection_effort_unknown",
    }
}

/// Handle used to stop a running turn. Best effort: the transcript is already
/// closed when `cancel` runs, so output after it is discarded.
pub(crate) trait TurnControl: Send {
    fn cancel(&self);
}

/// Produces reply text. `start_turn` must return promptly and deliver output
/// asynchronously through `sink`; it runs while the daemon serializes mutating
/// requests.
pub(crate) trait ConversationRuntime: Send + Sync {
    /// The identity that would answer a message sent with `choice`.
    fn identity(&self, choice: Option<&Choice>) -> RuntimeIdentity;

    /// The permission a turn at `level` runs with, recorded on the turn before it
    /// launches. `Err` refuses the level: the turn fails with that code and no
    /// agent runs. The default records nothing.
    fn permission(
        &self,
        _choice: Option<&Choice>,
        _level: PermissionLevel,
    ) -> Result<Option<TurnPermission>, RuntimeFailure> {
        Ok(None)
    }

    /// False only for a runtime that never produces output, so it needs no
    /// version pin.
    fn requires_pin(&self, _choice: Option<&Choice>) -> bool {
        true
    }

    /// Whether `identity` is one the daemon has verified. An unverified identity
    /// still runs (DPES-1106), and the desktop warns about it once per version.
    fn verified(&self, identity: &RuntimeIdentity) -> bool {
        let _ = identity;
        true
    }

    /// Settle which agent answers before the request gate is taken, so a slow
    /// probe never blocks other requests. `Conflict` means the state could not
    /// be established in time and nothing was recorded.
    fn prepare(&self, _choice: Option<&Choice>) -> Result<(), RpcError> {
        Ok(())
    }

    /// The daemon's settings were saved. A runtime that caches the configured default
    /// agent drops it, so the next selection reads the settings again.
    fn settings_changed(&self) {}

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure>;
}

/// The conversation service only asks whether an identifier names a
/// workstream. The workstream entity and its store belong to M7-07; fixtures
/// and unknown identifiers never resolve.
pub(crate) trait WorkstreamDirectory: Send + Sync {
    fn exists(&self, workstream_id: &str) -> Result<bool, RpcError>;
}

/// No workstream exists, so none can be addressed.
#[cfg(test)]
pub(crate) struct NoWorkstreams;

#[cfg(test)]
impl WorkstreamDirectory for NoWorkstreams {
    fn exists(&self, _workstream_id: &str) -> Result<bool, RpcError> {
        Ok(false)
    }
}

struct Shared {
    store: Arc<SqliteStore>,
    /// Read for the snapshot's machine load line. A closed gate never holds a turn.
    load: Arc<LoadHandle>,
    controls: Mutex<HashMap<String, Box<dyn TurnControl>>>,
}

impl Shared {
    fn controls(&self) -> MutexGuard<'_, HashMap<String, Box<dyn TurnControl>>> {
        self.controls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Close a running turn, then release its control. Returns the control
    /// only when this call ended the turn. `detail` is the terminal entry's body
    /// (see [`RuntimeFailure::detail`]), empty when the outcome has none.
    fn finish(
        &self,
        turn_id: &str,
        outcome: &TurnOutcome,
        detail: &str,
    ) -> Option<Box<dyn TurnControl>> {
        match self
            .store
            .finish_conversation_turn_with_detail(turn_id, outcome, detail)
        {
            Ok(Some(_)) => self.controls().remove(turn_id),
            _ => None,
        }
    }

    /// Complete a running turn and record its action block in the same store
    /// transaction. A store error rolls that back, so the block is retried as
    /// rejected; if that also fails the turn fails, and is never left running.
    fn complete(&self, turn_id: &str, block: TurnBlock) {
        let stored = match self.store.complete_conversation_turn(turn_id, block) {
            Err(_) => self
                .store
                .complete_conversation_turn(turn_id, TurnBlock::Rejected),
            stored => stored,
        };
        match stored {
            Ok(Some(_)) => {
                self.controls().remove(turn_id);
            }
            Ok(None) => {}
            Err(_) => {
                let outcome = TurnOutcome::Failed(RuntimeFailure::Failed.code().to_owned());
                if let Some(control) = self.finish(turn_id, &outcome, "") {
                    control.cancel();
                }
            }
        }
    }
}

/// Whether a turn may carry action proposals (DPES-1105). Only a session that
/// negotiated `orchestrator_actions` and a turn at `Restricted` or `Auto` qualify.
/// Auto has a shell, so a turn there could read the daemon's token. The approver
/// secret keeps a session that only holds the token from calling `actions.resolve`,
/// and every card still needs the user's approval, but a shell can edit the store or
/// replace the daemon, and no credential stops that. The user accepted that limit so
/// the default chat level (Auto) can propose. Extended and Full never carry proposals.
pub(crate) fn turn_actions(negotiated: bool, level: PermissionLevel) -> bool {
    negotiated && level_allows_actions(level)
}

/// The levels whose turns may carry proposals and whose stored proposals may be resolved.
/// Merge cards and standing approvals stay Restricted-only (`api_landing`, `grant_matcher`).
pub(crate) fn level_allows_actions(level: PermissionLevel) -> bool {
    matches!(level, PermissionLevel::Restricted | PermissionLevel::Auto)
}

/// Whether a turn is offered the read-only MCP server (DPES-1105 phase 6b): the setting is
/// on, the turn is at Restricted and it is not a wake turn. Auto, Full and task runs never
/// get it, whatever the setting says.
pub(crate) fn turn_mcp(enabled: bool, level: PermissionLevel, wake: bool) -> bool {
    enabled && level == PermissionLevel::Restricted && !wake
}

/// The reply channel for one turn. Cloneable and safe to use from any thread.
#[derive(Clone)]
pub(crate) struct ReplySink {
    shared: Arc<Shared>,
    turn_id: String,
    target: ConversationTarget,
    /// The session negotiated `orchestrator_actions`. Without it a held block is
    /// stripped, never parsed.
    actions: bool,
    /// The level the turn ran at. A merge card is approvable only from a Restricted turn.
    level: PermissionLevel,
    /// What opened the turn. Recorded on each usage receipt.
    trigger: TurnTrigger,
    /// Holds back an action block as it streams in. Shared by clones, so the
    /// order of output is the order of the lock.
    filter: Arc<Mutex<ReplyFilter>>,
}

/// The turn no longer accepts output; the runtime should stop.
#[derive(Debug)]
pub(crate) struct SinkClosed;

impl ReplySink {
    /// Append reply text. Fails once the turn is canceled, failed, completed
    /// or too large. Text that may be an action block is held back until the
    /// turn completes (DPES-1105).
    pub(crate) fn delta(&self, text: &str) -> Result<(), SinkClosed> {
        let mut filter = self.filter();
        let shown = filter.push(text);
        if shown.is_empty() {
            return Ok(());
        }
        self.append(&shown)
    }

    /// Stores already shown text as reply chunks. Held text never reaches here.
    fn append(&self, text: &str) -> Result<(), SinkClosed> {
        for piece in chunks(text) {
            match self.shared.store.append_reply_chunk(&self.turn_id, piece) {
                Ok(_) => {}
                Err(ConversationError::ReplyTooLarge) => {
                    let outcome = TurnOutcome::Failed("reply_too_large".to_owned());
                    if let Some(control) = self.shared.finish(&self.turn_id, &outcome, "") {
                        control.cancel();
                    }
                    return Err(SinkClosed);
                }
                Err(_) => return Err(SinkClosed),
            }
        }
        Ok(())
    }

    /// Records provider-reported usage for this turn. A failure is logged and never
    /// fails or delays the turn. Works after the turn closed, since the tokens were spent.
    pub(crate) fn usage(&self, reports: Vec<UsageReport>) {
        for report in reports {
            let receipt = ConversationUsageReceipt {
                conversation_id: super::wake::scope_of(&self.target),
                turn_id: self.turn_id.clone(),
                trigger: self.trigger,
                receipt_key: report.receipt_key,
                reported_at_ms: unix_now_ms(),
                input_tokens: report.input_tokens,
                cache_read_tokens: report.cache_read_tokens,
                cache_write_tokens: report.cache_write_tokens,
                output_tokens: report.output_tokens,
                reasoning_tokens: report.reasoning_tokens,
                cost: report.cost.map(|cost| (cost.nanos, cost.currency)),
                model: report.model,
            };
            if let Err(error) = self.shared.store.record_conversation_usage(&receipt) {
                eprintln!("orchestrator-daemon: cannot record chat usage: {error}");
            }
        }
    }

    fn filter(&self) -> MutexGuard<'_, ReplyFilter> {
        self.filter
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Ends the turn as completed. The held block, if any, is parsed here and
    /// recorded with the turn; a block that does not pass is rejected, and the
    /// reply text is kept either way. A completion on a turn that is already
    /// terminal records nothing.
    pub(crate) fn complete(&self) {
        let end = std::mem::take(&mut *self.filter()).finish();
        // A failed tail append means the turn is already terminal, or the store
        // is failing. A held block is never present with a tail, so the turn
        // still completes without one.
        if !end.tail.is_empty() {
            let _ = self.append(&end.tail);
        }
        let block = self.block(end.block);
        self.shared.complete(&self.turn_id, block);
    }

    /// Decides what the held block, if any, does to the turn. An accepted block
    /// has each `task.propose` checked against the chat envelope: a refused task
    /// is stored as a failed card, never a pending one.
    fn block(&self, held: Option<HeldBlock>) -> TurnBlock {
        let text = match held {
            None => return TurnBlock::None,
            Some(_) if !self.actions => return TurnBlock::Stripped,
            Some(HeldBlock::Overflow) => return TurnBlock::Rejected,
            Some(HeldBlock::Text(text)) => text,
        };
        let scope = match &self.target {
            ConversationTarget::Global => BlockScope::Global,
            ConversationTarget::Workstream(id) => BlockScope::Workstream(id.as_str()),
            ConversationTarget::Project(_) => return TurnBlock::Stripped,
        };
        match parse_block(&text, scope) {
            // Each `task.propose` is checked against its own project's envelope as it is
            // now. `resolve_as` checks a task again on approve, because the project's mode
            // or `chat_pr_delivery` may change before the user decides (DPES-1105).
            Ok(block) if self.targets_in_scope(&block) => {
                let store = &*self.shared.store;
                // A merge without a method gets the project's method now, so the card shows
                // and the digest binds the method that will run.
                let block = with_project_merge_methods(store, block);
                // A task for the home directory can only end in a report.
                let block = with_in_place_delivery(store, block);
                TurnBlock::Accepted(block.checked_with(|action| match action {
                    Action::TaskPropose(propose) => {
                        denial_of(store, propose).or_else(|| promotion_denial(store, propose))
                    }
                    Action::TaskRevise(revise) => revise_denial(store, revise),
                    // A merge from a turn above Restricted, for a pull request the app has not
                    // read, at another head, or that would be refused now is stored already
                    // `failed`, never approvable.
                    Action::DeliveryMerge(_) if self.level != PermissionLevel::Restricted => {
                        Some(orchestrator_core::task_envelope::Denial::Delivery)
                    }
                    Action::DeliveryMerge(merge) => merge_denial(
                        store,
                        merge,
                        i64::try_from(crate::executor::unix_now_ms()).unwrap_or(i64::MAX),
                    ),
                    _ => None,
                }))
            }
            _ => TurnBlock::Rejected,
        }
    }

    /// In a workstream conversation, an objective revision must name one of that
    /// workstream's objectives, and a task cancel or retry one of its tasks. The
    /// lookups need the store, so they are checked here.
    fn targets_in_scope(&self, block: &ActionBlock) -> bool {
        let ConversationTarget::Workstream(workstream_id) = &self.target else {
            return true;
        };
        let store = &self.shared.store;
        block.actions().iter().all(|action| {
            let objective_in_scope = action.revised_objective().is_none_or(|objective_id| {
                store
                    .get_objective(objective_id)
                    .is_ok_and(|objective| objective.workstream_id == *workstream_id)
            });
            let task_in_scope = action.target_task().is_none_or(|task_id| {
                store
                    .workstream_task_link(task_id)
                    .ok()
                    .flatten()
                    .is_some_and(|link| link.workstream_id == *workstream_id)
            });
            objective_in_scope && task_in_scope
        })
    }

    pub(crate) fn fail(&self, failure: RuntimeFailure) {
        let outcome = TurnOutcome::Failed(failure.code().to_owned());
        self.shared
            .finish(&self.turn_id, &outcome, &failure.detail());
    }

    /// The turn this sink writes to.
    pub(crate) fn turn_id(&self) -> &str {
        &self.turn_id
    }
}

fn truncate_to(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn chunks(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut end = rest.len().min(MAX_REPLY_CHUNK_BYTES);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (head, tail) = rest.split_at(end);
        rest = tail;
        Some(head)
    })
}

/// Most exchanges, bytes per reply and bytes in total that a runtime is told
/// about. A prompt larger than this would only slow every turn down.
const MAX_HISTORY_TURNS: usize = 6;
const MAX_HISTORY_REPLY_BYTES: usize = 4 * 1024;
/// Kept well under the Windows 32,767-character command line, with room for
/// an 8 KiB message and the frame.
const MAX_HISTORY_BYTES: usize = 12 * 1024;
/// How many of the newest transcript entries are scanned for history.
const HISTORY_SCAN_ENTRIES: i64 = 400;

/// Starts, tracks and cancels turns for both conversation scopes. Cloning shares the
/// running-turn registry, so the wake service drives the same turns the API does.
#[derive(Clone)]
pub(crate) struct ConversationService {
    shared: Arc<Shared>,
    runtime: Arc<dyn ConversationRuntime>,
    workstreams: Arc<dyn WorkstreamDirectory>,
    /// Whether the daemon has any pin at all. Without one no agent may answer.
    /// Which versions are verified is decided by the runtime, not here.
    pinned: bool,
}

impl ConversationService {
    pub(crate) fn new(
        store: Arc<SqliteStore>,
        runtime: Arc<dyn ConversationRuntime>,
        workstreams: Arc<dyn WorkstreamDirectory>,
        pinned: bool,
        load: Arc<LoadHandle>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                store,
                load,
                controls: Mutex::new(HashMap::new()),
            }),
            runtime,
            workstreams,
            pinned,
        }
    }

    /// Fail turns a previous process left running. Call once at startup,
    /// before any request is served. The daemon owns this: a turn left open
    /// can never complete. A runtime child can outlive a killed daemon; the
    /// runtime reaps those at startup (see `api_conversation_orphans`).
    pub(crate) fn recover(&self) -> Result<(), String> {
        self.shared
            .store
            .interrupt_open_conversation_turns()
            .map(|_| ())
            .map_err(|_| "cannot recover conversations".to_owned())
    }

    /// See [`ConversationRuntime::prepare`].
    pub(crate) fn prepare(&self, choice: Option<&Choice>) -> Result<(), RpcError> {
        self.runtime.prepare(choice)
    }

    /// See [`ConversationRuntime::settings_changed`]. Called after the daemon's settings are
    /// saved, so the configured default agent is read again on the next selection.
    pub(crate) fn settings_changed(&self) {
        self.runtime.settings_changed();
    }

    /// Whether a turn that recorded `identity` ran a verified version. Read when a
    /// turn is shown, so the desktop can warn about an unverified one (DPES-1106).
    pub(crate) fn verified(&self, identity: &RuntimeIdentity) -> bool {
        self.runtime.verified(identity)
    }

    pub(crate) fn require_target(&self, target: &ConversationTarget) -> Result<(), RpcError> {
        match target {
            ConversationTarget::Global => Ok(()),
            ConversationTarget::Project(id) => {
                self.shared
                    .store
                    .get_project(id)
                    .map_err(super::map_store_error)?;
                let scope = self
                    .shared
                    .store
                    .coordinator_scope(id)
                    .map_err(super::map_store_error)?;
                if scope.removed {
                    Err(RpcError::NotFound)
                } else {
                    Ok(())
                }
            }
            ConversationTarget::Workstream(id) => {
                if self.workstreams.exists(id)? {
                    Ok(())
                } else {
                    Err(RpcError::NotFound)
                }
            }
        }
    }

    /// Persist a message and hand it to the runtime. A runtime that cannot
    /// answer fails the turn, not the call: the message stays in the transcript.
    /// A level the agent refuses fails the turn with its code and no agent runs.
    /// The level is recorded on the turn only once the agent has started, so a
    /// turn that never launched records none. `actions` is the session's
    /// negotiated `orchestrator_actions`; it counts only at `Restricted`
    /// (`turn_actions`).
    ///
    /// A user message that meets a running wake turn cancels that turn first, so the
    /// user's turn always wins (DPES-1105). Any other running turn still refuses it.
    pub(crate) fn start(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
        choice: Option<&Choice>,
        level: PermissionLevel,
        negotiated: bool,
    ) -> Result<BegunTurn, ConversationError> {
        self.start_as(target, turn_id, text, choice, level, negotiated, None, None)
    }

    /// Starts a wake turn: Restricted always, the daemon's default model choice, actions
    /// allowed (a block becomes pending proposals, which need the user's approval like any
    /// other). `turn_id` is stable per batch, so a second call returns the stored turn
    /// (`created == false`). A turn already running in the conversation is an error, never
    /// cancelled: a wake never takes a turn from the user.
    pub(crate) fn start_wake(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        brief: &WakeBrief,
        evidence: Option<&Evidence>,
    ) -> Result<BegunTurn, ConversationError> {
        let text = brief.opening_text();
        self.start_as(
            target,
            turn_id,
            &text,
            None,
            PermissionLevel::Restricted,
            true,
            Some(brief),
            evidence,
        )
    }

    /// Starts the turn of a user's relay of a scout report (DPES-1105 phase 4a): Restricted
    /// always and the daemon's default model choice, whatever the chat was last set to, so
    /// the turn can only propose. `block` is the report inside its delimiters, handed to this
    /// one turn as data in its context; the stored opening text is `text`, written by the
    /// daemon, never the report. The turn is marked as one that read a report, so a proposal
    /// it makes says so. `turn_id` is stable per request, so a second call returns the stored
    /// turn (`created == false`). Like a user message it cancels a running wake turn.
    pub(crate) fn start_relay(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
        block: &str,
        negotiated: bool,
    ) -> Result<BegunTurn, ConversationError> {
        let evidence = Evidence {
            block: block.to_owned(),
            is_report: true,
        };
        self.start_as(
            target,
            turn_id,
            text,
            None,
            PermissionLevel::Restricted,
            negotiated,
            None,
            Some(&evidence),
        )
    }

    /// The one start path. A `wake` brief makes the turn a wake turn and forces the level
    /// to Restricted whatever the caller passed, so no other level is reachable. Evidence (a
    /// delimited block of agent text) does the same and drops the caller's model choice, so a
    /// turn that read agent text always runs at the least authority.
    #[allow(clippy::too_many_arguments)]
    fn start_as(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
        choice: Option<&Choice>,
        level: PermissionLevel,
        negotiated: bool,
        wake: Option<&WakeBrief>,
        evidence: Option<&Evidence>,
    ) -> Result<BegunTurn, ConversationError> {
        let level = if wake.is_some() || evidence.is_some() {
            PermissionLevel::Restricted
        } else {
            level
        };
        let choice = if evidence.is_some() { None } else { choice };
        let trigger = wake.map_or(TurnTrigger::User, |_| TurnTrigger::Wake);
        // Project coordinators gain replies, not the Global/workstream action authority.
        let actions =
            !matches!(target, ConversationTarget::Project(_)) && turn_actions(negotiated, level);
        let identity = self.runtime.identity(choice);
        let store = &self.shared.store;
        let permission = self.runtime.permission(choice, level);
        let begin = || {
            store.begin_conversation_turn_as(
                target,
                turn_id,
                text,
                &identity.name,
                &identity.version,
                trigger,
            )
        };
        let begun = match begin() {
            Err(ConversationError::TurnRunning { turn_id: running })
                if trigger == TurnTrigger::User =>
            {
                if !self.cancel_wake_turn(target, Some(&running))? {
                    return Err(ConversationError::TurnRunning { turn_id: running });
                }
                begin()?
            }
            other => other?,
        };
        if !begun.created {
            return Ok(begun);
        }
        // The mark comes before the launch, so a proposal of this turn always says it read a
        // report. A turn that cannot be marked never launches. A candidate's summary is not a
        // report, so its turn is not marked.
        let reads_report = evidence.is_some_and(|evidence| evidence.is_report);
        let unmarked = reads_report && store.mark_turn_read_report(turn_id).is_err();
        let block = evidence.map(|evidence| evidence.block.as_str());
        let failure = match permission {
            _ if unmarked => Some(RuntimeFailure::Failed),
            Err(failure) => Some(failure),
            Ok(permission) => self
                .launch(
                    &identity, choice, level, target, turn_id, text, actions, wake, block,
                )
                .or_else(|| {
                    permission.and_then(|permission| self.record_launched(turn_id, permission))
                }),
        };
        if let Some(failure) = failure {
            store.finish_conversation_turn_with_detail(
                turn_id,
                &TurnOutcome::Failed(failure.code().to_owned()),
                &failure.detail(),
            )?;
        }
        let turn = store
            .conversation_turn(turn_id)?
            .ok_or(ConversationError::TurnNotFound)?;
        Ok(BegunTurn { turn, ..begun })
    }

    /// Cancels the running turn of `target` when it is a wake turn (`only` names the turn
    /// the caller saw, when it knows one). Returns true when nothing is left running that
    /// blocks the caller: no turn, or a wake turn this call closed. A running user turn is
    /// left alone and gives false.
    pub(crate) fn cancel_wake_turn(
        &self,
        target: &ConversationTarget,
        only: Option<&str>,
    ) -> Result<bool, ConversationError> {
        let Some(turn) = self.shared.store.running_conversation_turn(target)? else {
            return Ok(true);
        };
        if turn.trigger != TurnTrigger::Wake || only.is_some_and(|id| id != turn.id) {
            return Ok(false);
        }
        self.cancel(target, &turn.id)?;
        Ok(true)
    }

    /// Start the runtime for a begun turn. A failure code means the turn must
    /// be closed as failed; a store error while registering the control is one
    /// too, so no turn is left running with nothing to finish it.
    #[allow(clippy::too_many_arguments)]
    fn launch(
        &self,
        identity: &RuntimeIdentity,
        choice: Option<&Choice>,
        level: PermissionLevel,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
        actions: bool,
        wake: Option<&WakeBrief>,
        relay: Option<&str>,
    ) -> Option<RuntimeFailure> {
        if let Some(failure) = self.pin_failure(choice) {
            return Some(failure);
        }
        let request = TurnRequest {
            target: target.clone(),
            text: text.to_owned(),
            identity: identity.clone(),
            choice: choice.cloned(),
            level,
            history: self.history(target, turn_id),
            context: Some({
                let snapshot = build_snapshot_with_notes(
                    &self.shared.store,
                    target,
                    actions,
                    &self.shared.load.read(Instant::now()),
                    wake.is_none(),
                );
                // A turn nobody asked for sees no path or token a task title may hold.
                let snapshot = if wake.is_some() {
                    redact_snapshot(&snapshot)
                } else {
                    snapshot
                };
                // The relayed report follows the snapshot, inside its own delimiters.
                match relay {
                    Some(block) => format!("{snapshot}\n\n{block}"),
                    None => snapshot,
                }
            }),
            actions,
            wake: wake.cloned(),
            mcp: self.mcp_view(level, wake.is_some(), relay.is_some()),
        };
        let sink = ReplySink {
            shared: self.shared.clone(),
            turn_id: turn_id.to_owned(),
            target: target.clone(),
            actions,
            level,
            trigger: wake.map_or(TurnTrigger::User, |_| TurnTrigger::Wake),
            filter: Arc::new(Mutex::new(ReplyFilter::default())),
        };
        match self.runtime.start_turn(request, sink) {
            Ok(control) => self
                .register(turn_id, control)
                .err()
                .map(|_| RuntimeFailure::Failed),
            Err(failure) => Some(failure),
        }
    }

    /// The read-only view for a turn that may be offered the MCP server. One read of the
    /// settings; a read error means off. A turn that relays a scout report is never
    /// offered it: it already runs at the least authority.
    fn mcp_view(&self, level: PermissionLevel, wake: bool, relay: bool) -> Option<Arc<McpView>> {
        let enabled = !relay
            && self
                .shared
                .store
                .orchestrator_settings()
                .map(|settings| settings.mcp.enabled)
                .unwrap_or(false);
        turn_mcp(enabled, level, wake).then(|| Arc::new(McpView::new(self.shared.store.clone())))
    }

    /// Refuses a selected agent only when the daemon has no pins at all. A
    /// version that is not pinned is not refused here: the runtime checks that
    /// the agent still offers what the chat needs, and the turn runs with an
    /// unverified-version warning otherwise (DPES-1106).
    fn pin_failure(&self, choice: Option<&Choice>) -> Option<RuntimeFailure> {
        if !self.runtime.requires_pin(choice) {
            return None;
        }
        (!self.pinned).then_some(RuntimeFailure::Unpinned)
    }

    /// The newest completed exchanges before `current_turn`, oldest first.
    /// Best effort: an unreadable transcript yields no history, not a failure.
    fn history(&self, target: &ConversationTarget, current_turn: &str) -> Vec<HistoryTurn> {
        let store = &self.shared.store;
        let Ok(latest) = store.conversation_page(target, 0, 1) else {
            return Vec::new();
        };
        let after = (latest.latest_sequence - HISTORY_SCAN_ENTRIES).max(0);
        let limit = usize::try_from(HISTORY_SCAN_ENTRIES).map_or(400, |n| n + 1);
        let Ok(page) = store.conversation_page(target, after, limit) else {
            return Vec::new();
        };
        let mut turns: Vec<(String, Option<String>, String, bool)> = Vec::new();
        for entry in &page.entries {
            if turns.last().map(|turn| &turn.0) != Some(&entry.turn_id) {
                turns.push((entry.turn_id.clone(), None, String::new(), false));
            }
            let Some(turn) = turns.last_mut() else {
                continue;
            };
            match entry.kind.as_str() {
                "user_message" => turn.1 = Some(entry.body.clone()),
                "reply_chunk" => turn.2.push_str(&entry.body),
                "turn_completed" => turn.3 = true,
                _ => {}
            }
        }
        let mut kept: Vec<HistoryTurn> = Vec::new();
        let mut bytes = 0usize;
        for (id, user, reply, completed) in turns.into_iter().rev() {
            let (true, Some(user)) = (completed && id != current_turn, user) else {
                continue;
            };
            if reply.is_empty() {
                continue;
            }
            let wake = store
                .conversation_turn(&id)
                .ok()
                .flatten()
                .is_some_and(|turn| turn.trigger == TurnTrigger::Wake);
            // A quiet wake turn said nothing, so the next turn is not shown it.
            if wake && reply.trim() == QUIET_TOKEN {
                continue;
            }
            let user = if wake {
                format!("{WAKE_HISTORY_PREFIX}{user}")
            } else {
                user
            };
            let reply = truncate_to(&reply, MAX_HISTORY_REPLY_BYTES);
            bytes += user.len() + reply.len();
            if kept.len() >= MAX_HISTORY_TURNS || bytes > MAX_HISTORY_BYTES {
                break;
            }
            kept.push(HistoryTurn { user, reply });
        }
        kept.reverse();
        kept
    }

    /// Record the level of a turn whose agent has just started. A store error
    /// stops the agent and fails the turn, so nothing runs unrecorded. Returns
    /// the failure code, or `None` when the level was recorded.
    fn record_launched(&self, turn_id: &str, permission: TurnPermission) -> Option<RuntimeFailure> {
        if self
            .shared
            .store
            .record_conversation_turn_permission(turn_id, permission)
            .is_ok()
        {
            return None;
        }
        let failure = RuntimeFailure::Failed;
        if let Some(control) =
            self.shared
                .finish(turn_id, &TurnOutcome::Failed(failure.code().to_owned()), "")
        {
            control.cancel();
        }
        Some(failure)
    }

    /// Keep the control only while the turn is still running; a runtime that
    /// finished synchronously has nothing left to cancel. On a store error the
    /// runtime is stopped, because nothing could cancel it later.
    fn register(
        &self,
        turn_id: &str,
        control: Box<dyn TurnControl>,
    ) -> Result<(), ConversationError> {
        let mut controls = self.shared.controls();
        let running = match self.shared.store.conversation_turn(turn_id) {
            Ok(turn) => turn.is_some_and(|turn| turn.state == TurnState::Running),
            Err(error) => {
                control.cancel();
                return Err(error);
            }
        };
        if running {
            controls.insert(turn_id.to_owned(), control);
        }
        Ok(())
    }

    /// Close a running turn as canceled. Returns the turn and whether this
    /// call applied the cancellation. Stopping the runtime is best effort.
    pub(crate) fn cancel(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
    ) -> Result<(ConversationTurn, bool), ConversationError> {
        let store = &self.shared.store;
        let turn = store
            .conversation_turn(turn_id)?
            .ok_or(ConversationError::TurnNotFound)?;
        let page = store.conversation_page(target, 0, 1)?;
        if page.conversation_id.as_deref() != Some(turn.conversation_id.as_str()) {
            return Err(ConversationError::TurnNotFound);
        }
        let applied = store
            .finish_conversation_turn(turn_id, &TurnOutcome::Cancelled)?
            .is_some();
        if applied {
            let control = self.shared.controls().remove(turn_id);
            if let Some(control) = control {
                control.cancel();
            }
        }
        let turn = store
            .conversation_turn(turn_id)?
            .ok_or(ConversationError::TurnNotFound)?;
        Ok((turn, applied))
    }

    /// The identity the default agent reports, which wake turns record. It is `unavailable`
    /// until `prepare` has settled the default.
    pub(crate) fn default_identity(&self) -> RuntimeIdentity {
        self.runtime.identity(None)
    }
}

#[cfg(test)]
mod mcp_gate_tests {
    use super::*;

    #[test]
    fn turn_mcp_is_true_only_for_a_user_turn_at_restricted_with_the_setting_on() {
        for level in [
            PermissionLevel::Restricted,
            PermissionLevel::Auto,
            PermissionLevel::Full,
            PermissionLevel::Extended,
        ] {
            for wake in [false, true] {
                for enabled in [false, true] {
                    let expected = enabled && !wake && level == PermissionLevel::Restricted;
                    assert_eq!(
                        turn_mcp(enabled, level, wake),
                        expected,
                        "{level:?} wake={wake} enabled={enabled}"
                    );
                }
            }
        }
    }
}
