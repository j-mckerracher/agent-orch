//! Task state machine and agent session/attempt model.
//!
//! Design rules (docs/technical-implementation-plan.md, sections 5.1 and 5.3):
//!
//! - A [`Task`] is a unit of requested work; an [`Attempt`] is one execution
//!   of that work; a [`Session`] is one agent process within one attempt.
//!   Three separate lifetimes: a completed agent session never implies a
//!   completed task, and a task may outlive many attempts.
//! - [`transition`] is the single validator for task states. Callers pass the
//!   current state and an event and receive the next state or a typed error.
//!   Every accepted transition names exactly one [`AuditEvent`]; persistence
//!   of the state change and its audit event must be one transaction
//!   (enforced by `orchestrator-store`, not here — this crate has no I/O).
//! - Irreversible states ([`TaskState::Succeeded`],
//!   [`TaskState::Cancelled`]) accept no events. Quiescent-failure states
//!   ([`TaskState::Failed`], [`TaskState::VerificationFailed`]) accept only
//!   [`TaskEvent::RetryRequested`], which re-queues the task with a fresh
//!   [`Attempt`]; the failed attempt is preserved, never erased. There is no
//!   path from any state back to `Running` except `Queued -> Running`, and
//!   no transition mutates a recorded attempt.
//!
//! The state machine is a table, not scattered conditionals: [`MOVE`] is the
//! exhaustive set of legal edges and the tests walk it.

/// Lifecycle state of a task.
///
/// Invariant: the engine, not the UI or any agent, is the only writer of this
/// field. The frontend may observe and request; only [`transition`] decides.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TaskState {
    /// Contract validated and persisted; not yet admitted to scheduling.
    Created,
    /// Admitted to the scheduler; dependencies and capacity may still be
    /// outstanding.
    Queued,
    /// An attempt is executing in a task-owned worktree.
    Running,
    /// Execution produced a candidate commit; verification is running or its
    /// outcome is pending. The candidate commit is recorded here, and only the
    /// verified commit may be delivered.
    Verifying,
    /// Verification passed. Irreversible.
    Succeeded,
    /// Verification failed and the bounded repair allowance is exhausted.
    /// Quiescent: accepts only [`TaskEvent::RetryRequested`].
    VerificationFailed,
    /// Execution failed and the retry allowance is exhausted.
    /// Quiescent: accepts only [`TaskEvent::RetryRequested`].
    Failed,
    /// Cancelled by an authorized request. Irreversible.
    Cancelled,
}

impl TaskState {
    /// Irreversible states accept no further events. Quiescent-failure
    /// states are *not* terminal: they still accept [`TaskEvent::RetryRequested`].
    pub fn is_terminal(self) -> bool {
        matches!(self, TaskState::Succeeded | TaskState::Cancelled)
    }
}

/// Events the engine may apply to a task.
///
/// Every variant names its allowed origin. Agent-origin events are *reports*
/// about an attempt the engine launched; they are never accepted while the
/// task is not `Running`, and they never reach task policy fields.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TaskEvent {
    /// Origin: engine (authorization step).
    Admit,
    /// Origin: engine (scheduler). Requires the attempt to be owned by the
    /// task and the worktree to be exclusively allocated.
    ExecutionStarted,
    /// Origin: engine (supervisor), after recording a candidate commit.
    CandidateReady,
    /// Origin: engine (supervisor), after independent verification of the
    /// recorded candidate. `passed` carries the verified outcome.
    VerificationOutcome { passed: bool },
    /// Origin: engine (supervisor), on an uncaught execution failure.
    ExecutionFailed,
    /// Origin: engine (recovery path) or authorized user request. Creates a
    /// fresh [`Attempt`]; the failed attempt is preserved. Only legal from
    /// `Failed` or `VerificationFailed`.
    RetryRequested,
    /// Origin: engine before an agent process is started. Legal only from
    /// `Created` or `Queued`; a request for a running task is not cancellation
    /// confirmation and must not use this event.
    CancelRequested,
    /// Origin: the process owner, after its agent handle has cancelled and
    /// waited for the managed child to exit. A separate cancellation request
    /// remains pending until this event is committed.
    CancellationConfirmed,
}

/// Audit events. One is emitted per accepted transition; the pair (event,
/// new state) is persisted atomically with the state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AuditEvent {
    /// `Created -> Queued` on [`TaskEvent::Admit`].
    TaskAdmitted,
    /// `Queued -> Running` on [`TaskEvent::ExecutionStarted`].
    ExecutionStarted,
    /// `Running -> Verifying` on [`TaskEvent::CandidateReady`].
    CandidateRecorded,
    /// `Verifying -> Succeeded` on a passing [`TaskEvent::VerificationOutcome`].
    VerificationPassed,
    /// `Verifying -> VerificationFailed` on a failing
    /// [`TaskEvent::VerificationOutcome`] with no repair allowance left.
    VerificationFailed,
    /// `Running -> Failed` on [`TaskEvent::ExecutionFailed`] with no retry
    /// allowance left.
    ExecutionFailed,
    /// `Failed | VerificationFailed -> Queued` on [`TaskEvent::RetryRequested`].
    RetryScheduled,
    /// `Created | Queued -> Cancelled` on [`TaskEvent::CancelRequested`], or
    /// `Running -> Cancelled` on [`TaskEvent::CancellationConfirmed`].
    TaskCancelled,
}

/// Result of a rejected transition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TransitionError {
    /// No edge exists from `from` for this event.
    Illegal { from: TaskState, event: TaskEvent },
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::Illegal { from, event } => {
                write!(f, "illegal transition: {:?} -- {:?} -->", from, event)
            }
        }
    }
}

impl std::error::Error for TransitionError {}

/// Exhaustive table of legal edges: (from, event) -> (next, audit event).
const MOVE: &[(TaskState, TaskEvent, TaskState, AuditEvent)] = &[
    (
        TaskState::Created,
        TaskEvent::Admit,
        TaskState::Queued,
        AuditEvent::TaskAdmitted,
    ),
    (
        TaskState::Queued,
        TaskEvent::ExecutionStarted,
        TaskState::Running,
        AuditEvent::ExecutionStarted,
    ),
    (
        TaskState::Running,
        TaskEvent::CandidateReady,
        TaskState::Verifying,
        AuditEvent::CandidateRecorded,
    ),
    (
        TaskState::Verifying,
        TaskEvent::VerificationOutcome { passed: true },
        TaskState::Succeeded,
        AuditEvent::VerificationPassed,
    ),
    (
        TaskState::Verifying,
        TaskEvent::VerificationOutcome { passed: false },
        TaskState::VerificationFailed,
        AuditEvent::VerificationFailed,
    ),
    (
        TaskState::Running,
        TaskEvent::ExecutionFailed,
        TaskState::Failed,
        AuditEvent::ExecutionFailed,
    ),
    (
        TaskState::Failed,
        TaskEvent::RetryRequested,
        TaskState::Queued,
        AuditEvent::RetryScheduled,
    ),
    (
        TaskState::VerificationFailed,
        TaskEvent::RetryRequested,
        TaskState::Queued,
        AuditEvent::RetryScheduled,
    ),
    (
        TaskState::Created,
        TaskEvent::CancelRequested,
        TaskState::Cancelled,
        AuditEvent::TaskCancelled,
    ),
    (
        TaskState::Queued,
        TaskEvent::CancelRequested,
        TaskState::Cancelled,
        AuditEvent::TaskCancelled,
    ),
    (
        TaskState::Running,
        TaskEvent::CancellationConfirmed,
        TaskState::Cancelled,
        AuditEvent::TaskCancelled,
    ),
];

/// Apply `event` to `current` and return the next state plus the audit event
/// to record in the same transaction as the state change.
///
/// This is the only legitimate writer of [`TaskState`]. Repairs and retries
/// are bounded by the caller (the scheduler's retry/repair counters, per plan
/// section 7); the table here does not track counters.
pub fn transition(
    current: TaskState,
    event: TaskEvent,
) -> Result<(TaskState, AuditEvent), TransitionError> {
    MOVE.iter()
        .find(|(from, ev, _, _)| *from == current && *ev == event)
        .map(|(_, _, next, audit)| (*next, *audit))
        .ok_or(TransitionError::Illegal {
            from: current,
            event,
        })
}

/// An execution attempt of a task. Attempts are append-only: a failed attempt
/// is never erased, and a retry creates a new attempt row (plan section 7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Attempt {
    /// Stable task-scoped identifier (a string so core stays std-only; the
    /// store maps it to its own id type).
    pub id: String,
    pub task_id: String,
    pub attempt_number: u32,
    pub state: AttemptState,
}

/// Lifecycle state of one execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AttemptState {
    /// Scheduled but not yet spawned.
    Pending,
    /// Agent session is running.
    Running,
    /// Produced a candidate commit; verification pending.
    CandidateReady,
    /// Attempt terminated with a failure reason; preserved, never mutated.
    Failed,
    /// Attempt terminated after a verified candidate.
    Succeeded,
    /// Attempt cancelled.
    Cancelled,
}

impl AttemptState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            AttemptState::Failed | AttemptState::Succeeded | AttemptState::Cancelled
        )
    }
}

/// A single agent process within one attempt (plan section 5.1, "Session").
///
/// Invariant: a session belongs to exactly one attempt, and an attempt hosts
/// at most one live session. Session completion reports into the attempt and,
/// via the attempt, into the task. It can never write task state or policy
/// directly; the only session fact the task state machine consumes is the
/// supervisor's translated [`TaskEvent`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub attempt_id: String,
    /// Adapter kind, recorded verbatim from the adapter so post-hoc
    /// diagnosis knows which protocol produced the session.
    pub adapter: String,
    pub state: SessionState,
}

/// Lifecycle state of one agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionState {
    /// Process spawned; first protocol event not yet observed.
    Spawning,
    /// Session is exchanging events.
    Running,
    /// Process exited; outcome recorded. Terminal.
    Exited,
    /// Process disappeared without a clean protocol exit (crash, EOF, kill).
    /// Terminal.
    Aborted,
}

impl SessionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, SessionState::Exited | SessionState::Aborted)
    }
}

/// Monotonic lifecycle of one execution attempt. Same rule as the task
/// table: the table is the exhaustive set of legal edges, terminal
/// states accept no moves, and no state regresses.
const ATTEMPT_MOVE: &[(AttemptState, AttemptState)] = &[
    (AttemptState::Pending, AttemptState::Running),
    (AttemptState::Pending, AttemptState::Cancelled),
    (AttemptState::Running, AttemptState::CandidateReady),
    (AttemptState::Running, AttemptState::Failed),
    (AttemptState::Running, AttemptState::Cancelled),
    (AttemptState::CandidateReady, AttemptState::Succeeded),
    (AttemptState::CandidateReady, AttemptState::Failed),
    (AttemptState::CandidateReady, AttemptState::Cancelled),
];

/// Validate one attempt state change. Callers must persist the change only
/// when this accepts it; terminal states accept nothing.
pub fn attempt_transition(current: AttemptState, next: AttemptState) -> Result<(), LifecycleError> {
    ATTEMPT_MOVE
        .iter()
        .any(|(f, t)| *f == current && *t == next)
        .then_some(())
        .ok_or(LifecycleError::IllegalAttempt { current, next })
}

/// Monotonic lifecycle of one agent session. A session's `Exited`/`Aborted`
/// is a *report about that process*, not a verdict on the attempt or the
/// task: the tables here never touch [`TaskState`].
const SESSION_MOVE: &[(SessionState, SessionState)] = &[
    (SessionState::Spawning, SessionState::Running),
    (SessionState::Spawning, SessionState::Exited),
    (SessionState::Spawning, SessionState::Aborted),
    (SessionState::Running, SessionState::Exited),
    (SessionState::Running, SessionState::Aborted),
];

/// Validate one session state change; see [`attempt_transition`].
pub fn session_transition(current: SessionState, next: SessionState) -> Result<(), LifecycleError> {
    SESSION_MOVE
        .iter()
        .any(|(f, t)| *f == current && *t == next)
        .then_some(())
        .ok_or(LifecycleError::IllegalSession { current, next })
}

/// Result of a rejected attempt or session state change.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LifecycleError {
    /// No edge exists from `current` to `next` for an attempt.
    IllegalAttempt {
        current: AttemptState,
        next: AttemptState,
    },
    /// No edge exists from `current` to `next` for a session.
    IllegalSession {
        current: SessionState,
        next: SessionState,
    },
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleError::IllegalAttempt { current, next } => {
                write!(f, "illegal attempt transition: {current:?} -> {next:?}")
            }
            LifecycleError::IllegalSession { current, next } => {
                write!(f, "illegal session transition: {current:?} -> {next:?}")
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_states() -> [TaskState; 8] {
        [
            TaskState::Created,
            TaskState::Queued,
            TaskState::Running,
            TaskState::Verifying,
            TaskState::Succeeded,
            TaskState::VerificationFailed,
            TaskState::Failed,
            TaskState::Cancelled,
        ]
    }

    fn all_events() -> [TaskEvent; 9] {
        [
            TaskEvent::Admit,
            TaskEvent::ExecutionStarted,
            TaskEvent::CandidateReady,
            TaskEvent::VerificationOutcome { passed: true },
            TaskEvent::VerificationOutcome { passed: false },
            TaskEvent::ExecutionFailed,
            TaskEvent::RetryRequested,
            TaskEvent::CancelRequested,
            TaskEvent::CancellationConfirmed,
        ]
    }

    #[test]
    fn table_edges_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for &(from, event, _, _) in MOVE {
            assert!(
                seen.insert((from, event)),
                "duplicate edge {from:?} -- {event:?}"
            );
        }
    }

    #[test]
    fn every_table_edge_applies_through_transition() {
        for &(from, event, next, audit) in MOVE {
            assert_eq!(transition(from, event), Ok((next, audit)));
        }
    }

    #[test]
    fn unlisted_combinations_are_rejected() {
        for state in all_states() {
            for event in all_events() {
                let listed = MOVE.iter().any(|(f, e, _, _)| *f == state && *e == event);
                if listed {
                    continue;
                }
                match transition(state, event) {
                    Err(TransitionError::Illegal { from, event: ev }) => {
                        assert_eq!(from, state);
                        assert_eq!(ev, event);
                    }
                    Ok((next, _)) => {
                        panic!("expected rejection: {state:?} -- {event:?} --> {next:?}")
                    }
                }
            }
        }
    }

    #[test]
    fn happy_path_reaches_success() {
        let mut state = TaskState::Created;
        for event in [
            TaskEvent::Admit,
            TaskEvent::ExecutionStarted,
            TaskEvent::CandidateReady,
            TaskEvent::VerificationOutcome { passed: true },
        ] {
            state = transition(state, event).unwrap().0;
        }
        assert_eq!(state, TaskState::Succeeded);
        assert!(state.is_terminal());
    }

    #[test]
    fn irreversible_states_reject_every_event() {
        for state in all_states() {
            if !state.is_terminal() {
                continue;
            }
            for event in all_events() {
                assert!(
                    matches!(
                        transition(state, event),
                        Err(TransitionError::Illegal { .. })
                    ),
                    "irreversible {state:?} accepted {event:?}"
                );
            }
            assert!(
                transition(state, TaskEvent::CancelRequested).is_err(),
                "irreversible {state:?} accepted cancel"
            );
        }
    }

    #[test]
    fn quiescent_failures_accept_only_retry() {
        for state in [TaskState::Failed, TaskState::VerificationFailed] {
            assert!(!state.is_terminal());
            for event in all_events() {
                if event == TaskEvent::RetryRequested {
                    continue;
                }
                assert!(
                    matches!(
                        transition(state, event),
                        Err(TransitionError::Illegal { .. })
                    ),
                    "quiescent {state:?} accepted {event:?}"
                );
            }
            assert!(
                transition(state, TaskEvent::CancelRequested).is_err(),
                "quiescent {state:?} accepted cancel"
            );
        }
    }

    #[test]
    fn retry_starts_from_failure_and_loses_the_old_attempt_path() {
        assert_eq!(
            transition(TaskState::Failed, TaskEvent::RetryRequested),
            Ok((TaskState::Queued, AuditEvent::RetryScheduled))
        );
        assert_eq!(
            transition(TaskState::VerificationFailed, TaskEvent::RetryRequested),
            Ok((TaskState::Queued, AuditEvent::RetryScheduled))
        );
        // Retry is not a general escape hatch: no other state offers it.
        for state in all_states() {
            if !matches!(state, TaskState::Failed | TaskState::VerificationFailed) {
                assert!(
                    transition(state, TaskEvent::RetryRequested).is_err(),
                    "unexpected retry from {state:?}"
                );
            }
        }
    }

    #[test]
    fn cancel_requested_is_legal_only_before_agent_start() {
        for state in [TaskState::Created, TaskState::Queued] {
            assert_eq!(
                transition(state, TaskEvent::CancelRequested),
                Ok((TaskState::Cancelled, AuditEvent::TaskCancelled)),
                "cancel not legal from {state:?}"
            );
        }
        for state in [
            TaskState::Running,
            TaskState::Verifying,
            TaskState::Failed,
            TaskState::VerificationFailed,
        ] {
            assert!(
                matches!(
                    transition(state, TaskEvent::CancelRequested),
                    Err(TransitionError::Illegal { .. })
                ),
                "unconfirmed cancel request was terminal from {state:?}"
            );
        }
    }

    #[test]
    fn running_task_requires_cancellation_confirmation() {
        assert_eq!(
            transition(TaskState::Running, TaskEvent::CancellationConfirmed),
            Ok((TaskState::Cancelled, AuditEvent::TaskCancelled))
        );
        for state in [
            TaskState::Created,
            TaskState::Queued,
            TaskState::Verifying,
            TaskState::Failed,
            TaskState::VerificationFailed,
        ] {
            assert!(
                transition(state, TaskEvent::CancellationConfirmed).is_err(),
                "cancellation confirmation was legal from {state:?}"
            );
        }
    }

    #[test]
    fn failed_attempt_preserved_on_retry() {
        // The state machine itself cannot erase history: the attempt record
        // is append-only data, and RetryRequested re-queues the *task* rather
        // than mutating the attempt row.
        let failed = Attempt {
            id: "a1".into(),
            task_id: "t1".into(),
            attempt_number: 1,
            state: AttemptState::Failed,
        };
        let retried = Attempt {
            id: "a2".into(),
            task_id: "t1".into(),
            attempt_number: 2,
            state: AttemptState::Pending,
        };
        assert_eq!(failed.attempt_number, 1);
        assert_eq!(retried.attempt_number, 2);
        assert!(failed.state.is_terminal() && !retried.state.is_terminal());
    }

    #[test]
    fn session_exit_is_terminal_and_scoped_to_attempt() {
        let session = Session {
            id: "s1".into(),
            attempt_id: "a1".into(),
            adapter: "codex".into(),
            state: SessionState::Running,
        };
        assert!(!session.state.is_terminal());
        let exited = SessionState::Exited;
        assert!(exited.is_terminal());
        assert!(SessionState::Aborted.is_terminal());
        // A session references exactly one attempt; the type has no task-id
        // or policy field it could write to.
        assert_eq!(session.attempt_id, "a1");
    }

    #[test]
    fn transition_table_is_total_over_documented_events() {
        // Guards against a new event variant sneaking in without table
        // coverage.
        for event in all_events() {
            let covered = MOVE.iter().filter(|(_, e, _, _)| *e == event).count();
            assert!(covered > 0, "event {event:?} has no edges");
        }
    }

    fn all_attempt_states() -> [AttemptState; 6] {
        [
            AttemptState::Pending,
            AttemptState::Running,
            AttemptState::CandidateReady,
            AttemptState::Failed,
            AttemptState::Succeeded,
            AttemptState::Cancelled,
        ]
    }

    fn all_session_states() -> [SessionState; 4] {
        [
            SessionState::Spawning,
            SessionState::Running,
            SessionState::Exited,
            SessionState::Aborted,
        ]
    }

    #[test]
    fn attempt_edges_match_the_table_exactly() {
        // Every table edge applies; every other ordered pair is rejected.
        for &(from, next) in ATTEMPT_MOVE {
            assert!(
                attempt_transition(from, next).is_ok(),
                "{from:?} -> {next:?}"
            );
        }
        for &from in &all_attempt_states() {
            for &next in &all_attempt_states() {
                let legal = ATTEMPT_MOVE.iter().any(|(f, t)| *f == from && *t == next);
                assert_eq!(
                    attempt_transition(from, next).is_ok(),
                    legal,
                    "{from:?} -> {next:?}"
                );
            }
        }
    }

    #[test]
    fn attempt_terminals_accept_nothing_and_nothing_is_undone() {
        for terminal in [
            AttemptState::Failed,
            AttemptState::Succeeded,
            AttemptState::Cancelled,
        ] {
            for &next in &all_attempt_states() {
                assert!(
                    attempt_transition(terminal, next).is_err(),
                    "terminal {terminal:?} accepted a move to {next:?}"
                );
            }
        }
        // Regression: a running attempt never steps back to Pending, and a
        // candidate never steps back to Running.
        assert!(attempt_transition(AttemptState::Running, AttemptState::Pending).is_err());
        assert!(attempt_transition(AttemptState::CandidateReady, AttemptState::Running).is_err());
    }

    #[test]
    fn session_edges_match_the_table_exactly() {
        for &(from, next) in SESSION_MOVE {
            assert!(
                session_transition(from, next).is_ok(),
                "{from:?} -> {next:?}"
            );
        }
        for &from in &all_session_states() {
            for &next in &all_session_states() {
                let legal = SESSION_MOVE.iter().any(|(f, t)| *f == from && *t == next);
                assert_eq!(
                    session_transition(from, next).is_ok(),
                    legal,
                    "{from:?} -> {next:?}"
                );
            }
        }
        // Regression: no session state is ever undone.
        assert!(session_transition(SessionState::Running, SessionState::Spawning).is_err());
    }
}
