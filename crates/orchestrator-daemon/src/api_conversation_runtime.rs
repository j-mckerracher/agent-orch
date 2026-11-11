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
//! No provider is bound here. The shipped runtime is [`UnavailableRuntime`],
//! which fails every turn with `runtime_unavailable`. A real adapter (M7-24)
//! must be pinned to an exact name and version by [`ConversationService`]; a
//! runtime that does not match its pin never receives a message.
//!
//! The runtime-facing items below have no non-test caller until a provider is
//! bound, so dead-code analysis is relaxed for this module only.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use orchestrator_ipc::RpcError;
use orchestrator_store::{
    BegunTurn, ConversationError, ConversationTarget, ConversationTurn, SqliteStore, TurnOutcome,
    TurnState, MAX_REPLY_CHUNK_BYTES,
};

/// Name reported by [`UnavailableRuntime`].
pub(crate) const UNAVAILABLE_RUNTIME_NAME: &str = "unavailable";

/// Who produced replies. Recorded on every turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeIdentity {
    pub name: String,
    pub version: String,
}

/// What a runtime is asked to answer.
pub(crate) struct TurnRequest {
    pub turn_id: String,
    pub target: ConversationTarget,
    pub text: String,
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
}

impl RuntimeFailure {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "runtime_unavailable",
            Self::CredentialsMissing => "credentials_missing",
            Self::PermissionDenied => "permission_denied",
            Self::VersionMismatch => "version_mismatch",
            Self::Failed => "runtime_failed",
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

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure>;
}

/// Fail-closed default until a provider adapter is bound.
pub(crate) struct UnavailableRuntime;

impl ConversationRuntime for UnavailableRuntime {
    fn identity(&self) -> RuntimeIdentity {
        RuntimeIdentity {
            name: UNAVAILABLE_RUNTIME_NAME.to_owned(),
            version: "none".to_owned(),
        }
    }

    fn requires_pin(&self) -> bool {
        false
    }

    fn start_turn(
        &self,
        _request: TurnRequest,
        _sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure> {
        Err(RuntimeFailure::Unavailable)
    }
}

/// Seam for M7-07. The workstream entity and its store belong to that item;
/// the conversation service only asks whether an identifier names one.
///
/// Fixtures and unknown identifiers never resolve. M7-07 replaces
/// [`NoWorkstreams`] with a store-backed implementation.
pub(crate) trait WorkstreamDirectory: Send + Sync {
    fn exists(&self, workstream_id: &str) -> Result<bool, RpcError>;
}

/// Default until M7-07 lands: no workstream exists, so none can be addressed.
pub(crate) struct NoWorkstreams;

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

/// Starts, tracks and cancels turns for both conversation scopes.
pub(crate) struct ConversationService {
    shared: Arc<Shared>,
    runtime: Arc<dyn ConversationRuntime>,
    workstreams: Arc<dyn WorkstreamDirectory>,
    /// Exact identity the runtime must report. Absent means no pinned runtime.
    pin: Option<RuntimeIdentity>,
}

impl ConversationService {
    pub(crate) fn new(
        store: Arc<SqliteStore>,
        runtime: Arc<dyn ConversationRuntime>,
        workstreams: Arc<dyn WorkstreamDirectory>,
        pin: Option<RuntimeIdentity>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                store,
                controls: Mutex::new(HashMap::new()),
            }),
            runtime,
            workstreams,
            pin,
        }
    }

    /// Fail turns a previous process left running. Call once at startup,
    /// before any request is served.
    pub(crate) fn recover(&self) -> Result<(), String> {
        self.shared
            .store
            .interrupt_open_conversation_turns()
            .map(|_| ())
            .map_err(|_| "cannot recover conversations".to_owned())
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
        let failure = match self.pin_failure(&identity) {
            Some(code) => Some(code.to_owned()),
            None => {
                let request = TurnRequest {
                    turn_id: turn_id.to_owned(),
                    target: target.clone(),
                    text: text.to_owned(),
                };
                let sink = ReplySink {
                    shared: self.shared.clone(),
                    turn_id: turn_id.to_owned(),
                };
                match self.runtime.start_turn(request, sink) {
                    Ok(control) => {
                        self.register(turn_id, control)?;
                        None
                    }
                    Err(failure) => Some(failure.code().to_owned()),
                }
            }
        };
        if let Some(code) = failure {
            store.finish_conversation_turn(turn_id, &TurnOutcome::Failed(code))?;
        }
        let turn = store
            .conversation_turn(turn_id)?
            .ok_or(ConversationError::TurnNotFound)?;
        Ok(BegunTurn { turn, ..begun })
    }

    fn pin_failure(&self, identity: &RuntimeIdentity) -> Option<&'static str> {
        if !self.runtime.requires_pin() {
            return None;
        }
        match &self.pin {
            None => Some("runtime_unpinned"),
            Some(pin) if pin != identity => Some("version_mismatch"),
            Some(_) => None,
        }
    }

    /// Keep the control only while the turn is still running; a runtime that
    /// finished synchronously has nothing left to cancel.
    fn register(
        &self,
        turn_id: &str,
        control: Box<dyn TurnControl>,
    ) -> Result<(), ConversationError> {
        let mut controls = self.shared.controls();
        let running = self
            .shared
            .store
            .conversation_turn(turn_id)?
            .is_some_and(|turn| turn.state == TurnState::Running);
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
