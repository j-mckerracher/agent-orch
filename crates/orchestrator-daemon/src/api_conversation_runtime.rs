//! Runtime boundary and turn supervision for orchestrator conversations (M7-08).
//!
//! Plan: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`
//! section 11. The design record is `docs/m7-08-conversation-service.md`.
//!
//! The daemon owns the transcript, turn lifecycle, cancellation and recovery.
//! A [`ConversationRuntime`] only produces reply text for one turn through a
//! [`ReplySink`]. It receives no store handle, no task, approval or delivery
//! capability, and nothing it emits is parsed as a command.
//!
//! No provider is bound here. The daemon binds the agent runtime of
//! `api_conversation_agent.rs` (M7-25), which fails every turn with
//! `runtime_unavailable` when no launchable agent is installed. A runtime that
//! produces output must match one of the identities [`ConversationService`]
//! was given (an exact name and version); a runtime that does not match never
//! receives a message.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use orchestrator_ipc::RpcError;
use orchestrator_store::{
    BegunTurn, ConversationError, ConversationTarget, ConversationTurn, SqliteStore, TurnOutcome,
    TurnState, MAX_REPLY_CHUNK_BYTES,
};

/// Name reported while no agent is selected.
pub(crate) const UNAVAILABLE_RUNTIME_NAME: &str = "unavailable";

/// Who produced replies. Recorded on every turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeIdentity {
    pub name: String,
    pub version: String,
}

/// One earlier, completed exchange of the same conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryTurn {
    pub user: String,
    pub reply: String,
}

/// What a runtime is asked to answer.
pub(crate) struct TurnRequest {
    pub target: ConversationTarget,
    pub text: String,
    /// The identity the service pinned and recorded for this turn. A runtime
    /// launches only this one.
    pub identity: RuntimeIdentity,
    /// Recent completed exchanges, oldest first, bounded in count and size.
    /// Failed and cancelled turns are not included.
    pub history: Vec<HistoryTurn>,
    /// A short description of the workstream a workstream turn is about.
    pub context: Option<String>,
}

/// Why a turn could not be answered. Maps to a stable code; runtime-supplied
/// text is never stored or returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeFailure {
    Unavailable,
    CredentialsMissing,
    PermissionDenied,
    VersionMismatch,
    Failed,
    Timeout,
    Busy,
}

impl RuntimeFailure {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "runtime_unavailable",
            Self::CredentialsMissing => "credentials_missing",
            Self::PermissionDenied => "permission_denied",
            Self::VersionMismatch => "version_mismatch",
            Self::Failed => "runtime_failed",
            Self::Timeout => "runtime_timeout",
            Self::Busy => "runtime_busy",
        }
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
    fn identity(&self) -> RuntimeIdentity;

    /// False only for a runtime that never produces output, so it needs no
    /// version pin.
    fn requires_pin(&self) -> bool {
        true
    }

    /// Settle which agent answers before the request gate is taken, so a slow
    /// probe never blocks other requests. `Conflict` means the state could not
    /// be established in time and nothing was recorded.
    fn prepare(&self) -> Result<(), RpcError> {
        Ok(())
    }

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure>;
}

/// The conversation service only asks whether an identifier names a workstream
/// and, for a turn, for a short description of it. The workstream entity and
/// its store belong to M7-07; fixtures and unknown identifiers never resolve.
pub(crate) trait WorkstreamDirectory: Send + Sync {
    fn exists(&self, workstream_id: &str) -> Result<bool, RpcError>;

    /// A short description for the runtime prompt. Best effort.
    fn describe(&self, _workstream_id: &str) -> Option<String> {
        None
    }
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
    controls: Mutex<HashMap<String, Box<dyn TurnControl>>>,
}

impl Shared {
    fn controls(&self) -> MutexGuard<'_, HashMap<String, Box<dyn TurnControl>>> {
        self.controls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Close a running turn, then release its control. Returns the control
    /// only when this call ended the turn.
    fn finish(&self, turn_id: &str, outcome: &TurnOutcome) -> Option<Box<dyn TurnControl>> {
        match self.store.finish_conversation_turn(turn_id, outcome) {
            Ok(Some(_)) => self.controls().remove(turn_id),
            _ => None,
        }
    }
}

/// The reply channel for one turn. Cloneable and safe to use from any thread.
#[derive(Clone)]
pub(crate) struct ReplySink {
    shared: Arc<Shared>,
    turn_id: String,
}

/// The turn no longer accepts output; the runtime should stop.
#[derive(Debug)]
pub(crate) struct SinkClosed;

impl ReplySink {
    /// Append reply text. Fails once the turn is cancelled, failed, completed
    /// or too large.
    pub(crate) fn delta(&self, text: &str) -> Result<(), SinkClosed> {
        for piece in chunks(text) {
            match self.shared.store.append_reply_chunk(&self.turn_id, piece) {
                Ok(_) => {}
                Err(ConversationError::ReplyTooLarge) => {
                    let outcome = TurnOutcome::Failed("reply_too_large".to_owned());
                    if let Some(control) = self.shared.finish(&self.turn_id, &outcome) {
                        control.cancel();
                    }
                    return Err(SinkClosed);
                }
                Err(_) => return Err(SinkClosed),
            }
        }
        Ok(())
    }

    pub(crate) fn complete(&self) {
        self.shared.finish(&self.turn_id, &TurnOutcome::Completed);
    }

    pub(crate) fn fail(&self, failure: RuntimeFailure) {
        let outcome = TurnOutcome::Failed(failure.code().to_owned());
        self.shared.finish(&self.turn_id, &outcome);
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

/// Starts, tracks and cancels turns for both conversation scopes.
pub(crate) struct ConversationService {
    shared: Arc<Shared>,
    runtime: Arc<dyn ConversationRuntime>,
    workstreams: Arc<dyn WorkstreamDirectory>,
    /// Exact identities a runtime may report. Empty means no pinned runtime.
    pins: Vec<RuntimeIdentity>,
}

impl ConversationService {
    pub(crate) fn new(
        store: Arc<SqliteStore>,
        runtime: Arc<dyn ConversationRuntime>,
        workstreams: Arc<dyn WorkstreamDirectory>,
        pins: Vec<RuntimeIdentity>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                store,
                controls: Mutex::new(HashMap::new()),
            }),
            runtime,
            workstreams,
            pins,
        }
    }

    /// Fail turns a previous process left running. Call once at startup,
    /// before any request is served. The daemon owns this: a runtime child
    /// does not outlive the daemon that started it, so an open turn found at
    /// startup can never complete.
    pub(crate) fn recover(&self) -> Result<(), String> {
        self.shared
            .store
            .interrupt_open_conversation_turns()
            .map(|_| ())
            .map_err(|_| "cannot recover conversations".to_owned())
    }

    /// See [`ConversationRuntime::prepare`].
    pub(crate) fn prepare(&self) -> Result<(), RpcError> {
        self.runtime.prepare()
    }

    pub(crate) fn require_target(&self, target: &ConversationTarget) -> Result<(), RpcError> {
        match target {
            ConversationTarget::Global => Ok(()),
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
    pub(crate) fn start(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
    ) -> Result<BegunTurn, ConversationError> {
        let identity = self.runtime.identity();
        let store = &self.shared.store;
        let begun = store.begin_conversation_turn(
            target,
            turn_id,
            text,
            &identity.name,
            &identity.version,
        )?;
        if !begun.created {
            return Ok(begun);
        }
        let failure = self.launch(&identity, target, turn_id, text);
        if let Some(code) = failure {
            store.finish_conversation_turn(turn_id, &TurnOutcome::Failed(code))?;
        }
        let turn = store
            .conversation_turn(turn_id)?
            .ok_or(ConversationError::TurnNotFound)?;
        Ok(BegunTurn { turn, ..begun })
    }

    /// Start the runtime for a begun turn. A failure code means the turn must
    /// be closed as failed; a store error while registering the control is one
    /// too, so no turn is left running with nothing to finish it.
    fn launch(
        &self,
        identity: &RuntimeIdentity,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
    ) -> Option<String> {
        if let Some(code) = self.pin_failure(identity) {
            return Some(code.to_owned());
        }
        let request = TurnRequest {
            target: target.clone(),
            text: text.to_owned(),
            identity: identity.clone(),
            history: self.history(target, turn_id),
            context: match target {
                ConversationTarget::Workstream(id) => self.workstreams.describe(id),
                ConversationTarget::Global => None,
            },
        };
        let sink = ReplySink {
            shared: self.shared.clone(),
            turn_id: turn_id.to_owned(),
        };
        match self.runtime.start_turn(request, sink) {
            Ok(control) => self
                .register(turn_id, control)
                .err()
                .map(|_| RuntimeFailure::Failed.code().to_owned()),
            Err(failure) => Some(failure.code().to_owned()),
        }
    }

    fn pin_failure(&self, identity: &RuntimeIdentity) -> Option<&'static str> {
        if !self.runtime.requires_pin() {
            return None;
        }
        if self.pins.is_empty() {
            Some("runtime_unpinned")
        } else if self.pins.contains(identity) {
            None
        } else {
            Some("version_mismatch")
        }
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

    /// Close a running turn as cancelled. Returns the turn and whether this
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
}
