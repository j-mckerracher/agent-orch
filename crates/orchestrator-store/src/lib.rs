//! Local durable records for the task lifecycle (board M1-01; plan sections
//! 5, 7, 9). One task = one JSON file under a data directory, rewritten
//! atomically (temp file + rename) on every accepted mutation. M2-01
//! replaces this file layout with SQLite; the [`Store`] trait is the
//! replacement seam, and the invariants below hold for any backend.
//!
//! Updated: 2026-11-07T13:21:11-07:00 (M1-01 implementation).
//!
//! # Invariants enforced here (core owns the state tables, this crate owns
//! their durability)
//!
//! - **Contract before state.** [`Store::create`] validates via
//!   [`orchestrator_core::contract::validate`]; an invalid contract never
//!   produces a record, and an admitted contract is stored verbatim and
//!   never rewritten afterwards.
//! - **State and audit event are one record.** [`Store::transition`]
//!   validates via the single central table
//!   [`orchestrator_core::task::transition`], appends the returned
//!   [`orchestrator_core::task::AuditEvent`] to the record's ordered
//!   journal, and persists both in the single atomic rewrite. The pair can
//!   never be half-written: rename is all-or-nothing, so a crash leaves the
//!   previous record intact (the new transition simply never happened).
//! - **Attempts and sessions are separate records of separate lifetimes.**
//!   They are stored as their own collections with their own ids and
//!   terminal-state rules; none of their methods touches
//!   [`orchestrator_core::task::TaskState`]. A session exiting (even cleanly)
//!   can never move a task to `Succeeded` — only the engine's
//!   `VerificationOutcome { passed: true }` transition does that.
//! - **History is append-only.** Failed attempts and terminal sessions are
//!   immutable; retries append a new attempt and a new journal entry.
//!
//! # Assumptions / limits
//!
//! - Single writer process. Cross-process locking and the WAL/journal
//!   schema are M2-01 (SQLite) scope. In-process callers on one
//!   `FileStore` handle are serialized by a mutex (no shared temp path,
//!   no lost transition); two processes writing the same directory are
//!   last-writer-wins with no optimistic concurrency in M1.
//! - `std::fs::rename` replaces the destination on both supported platforms
//!   (Windows `MoveFileEx`, POSIX atomic rename). This is what makes the
//!   rewrite atomic.
use std::path::Path;

use orchestrator_core::contract::{validate as validate_contract, ContractError, TaskContract};
use orchestrator_core::task::{
    transition, Attempt, AttemptState, AuditEvent, Session, SessionState, TaskEvent, TaskState,
    TransitionError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The full durable record of one task. Attempts and sessions are stored as
/// separate collections (separate ids, separate lifetimes), not folded into
/// the task row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    /// The admitted contract, stored verbatim at creation and immutable.
    pub contract: TaskContract,
    /// Current task state. Only [`Store::transition`] may change it.
    pub state: TaskState,
    /// Execution attempts of this task, in creation order. Append-only:
    /// entries are never removed; terminal entries are never modified.
    pub attempts: Vec<Attempt>,
    /// Agent sessions of this task, in creation order. Same rules as
    /// `attempts`.
    pub sessions: Vec<Session>,
    /// Ordered audit journal: one entry per accepted task transition, in
    /// transition order. Grows with the record; never truncated.
    pub events: Vec<AuditEvent>,
    /// Bumped on every accepted mutation; lets a future backend detect
    /// lost or stale updates.
    pub version: u64,
}

/// Failures of a store operation. Each variant names the invariant that
/// failed so callers (and tests) can distinguish policy violations from
/// I/O. Not `Clone`/`PartialEq`: the I/O and serde errors it wraps are
/// neither.
#[derive(Debug, Error)]
pub enum StoreError {
    /// `create` for an id that already has a record.
    #[error("task {0} already exists")]
    AlreadyExists(String),
    /// The id has no record.
    #[error("task {0} not found")]
    NotFound(String),
    /// The id is not a plain file name, so no record path can be formed.
    #[error("invalid task id {0:?}")]
    InvalidTaskId(String),
    /// The attempt names a different task than the record it was added to.
    #[error("attempt {attempt_id} belongs to task {attempt_task}, not {record_task}")]
    CrossTaskAttempt {
        attempt_id: String,
        attempt_task: String,
        record_task: String,
    },
    /// Attempt numbers are sequential from 1; this one breaks the sequence.
    #[error("attempt number {got} out of sequence (expected {expected}) for task {task_id}")]
    BadAttemptNumber {
        expected: u32,
        got: u32,
        task_id: String,
    },
    /// `create` rejected the contract.
    #[error("contract rejected: {0}")]
    InvalidContract(#[from] ContractError),
    /// The event is not a legal edge from the current state.
    #[error("illegal transition: {0}")]
    IllegalTransition(#[from] TransitionError),
    /// An attempt already records this id.
    #[error("attempt {attempt_id} already recorded for task {task_id}")]
    DuplicateAttempt { attempt_id: String, task_id: String },
    /// The session references an attempt this task has not recorded. A
    /// session belongs to exactly one attempt.
    #[error("session {session_id} references unknown attempt {attempt_id}")]
    UnknownAttempt { session_id: String, attempt_id: String },
    /// An attempt already records this session id.
    #[error("session {session_id} already recorded for task {task_id}")]
    DuplicateSession { session_id: String, task_id: String },
    /// The attempt has no record.
    #[error("attempt {attempt_id} not found for task {task_id}")]
    NoAttempt { attempt_id: String, task_id: String },
    /// The session has no record.
    #[error("session {session_id} not found for task {task_id}")]
    NoSession { session_id: String, task_id: String },
    /// A terminal attempt cannot be updated; the history is preserved.
    #[error("attempt {attempt_id} is terminal ({state:?}) and immutable")]
    TerminalAttempt { attempt_id: String, state: AttemptState },
    /// A terminal session cannot be updated.
    #[error("session {session_id} is terminal ({state:?}) and immutable")]
    TerminalSession { session_id: String, state: SessionState },
    /// A stored file does not parse as a [`TaskRecord`].
    #[error("corrupt record for task {task_id}: {detail}")]
    Corrupt { task_id: String, detail: String },
    /// (De)serialization failure while writing a record.
    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),
    /// Filesystem failure.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

/// Durable store for task records. The single seam M2-01 will re-implement
/// on SQLite (WAL, migrations, ordered journal table) without changing
/// callers.
///
/// Implementations must guarantee, for each method that mutates, that the
/// mutation (state change *and* its audit event, where applicable) is
/// durable as one unit: a process crash may leave either the previous
/// record or the new one, never a mix.
pub trait Store {
    /// Validate and persist a new contract as a `Created` task.
    fn create(&self, contract: &TaskContract) -> Result<(), StoreError>;

    /// Load the full record for a task.
    fn get(&self, task_id: &str) -> Result<TaskRecord, StoreError>;

    /// Load every record in the store.
    fn list(&self) -> Result<Vec<TaskRecord>, StoreError>;

    /// Apply a central transition: validate with the single state table,
    /// move the state, append the matching audit event, and persist both
    /// atomically. Returns the audit event that was recorded.
    fn transition(&self, task_id: &str, event: TaskEvent) -> Result<AuditEvent, StoreError>;

    /// Append a new execution attempt. The attempt must name this task,
    /// its id must be fresh, and its number must continue the sequence
    /// from 1. This is how a retry gets its new attempt without erasing
    /// the failed one.
    fn add_attempt(&self, task_id: &str, attempt: &Attempt) -> Result<(), StoreError>;

    /// Advance one attempt's own state. Terminal attempts are immutable.
    /// This never touches task state.
    fn update_attempt_state(
        &self,
        task_id: &str,
        attempt_id: &str,
        state: AttemptState,
    ) -> Result<(), StoreError>;

    /// Append a new agent session. The attempt must already be recorded on
    /// the task. This never touches task state.
    fn add_session(&self, task_id: &str, session: &Session) -> Result<(), StoreError>;

    /// Advance one session's own state. Terminal sessions are immutable.
    /// This never touches task state: a session exiting can not succeed,
    /// fail, or cancel the task.
    fn update_session_state(
        &self,
        task_id: &str,
        session_id: &str,
        state: SessionState,
    ) -> Result<(), StoreError>;
}

/// File-backed [`Store`]: one `<data_dir>/tasks/<task_id>.json` per task,
/// written by temp-file + rename. The temp file is removed if anything
/// fails before the rename, so a failure leaves the previous record intact.
pub struct FileStore {
    tasks: std::path::PathBuf,
    /// Serializes mutations on this handle so concurrent in-process
    /// callers cannot share the pid-only temp path or lose a transition.
    /// It does not span processes (see the crate-level limits).
    lock: std::sync::Mutex<()>,
}

impl FileStore {
    /// Open (creating the directory tree if needed) a store rooted at
    /// `data_dir`. The store itself holds no file handles.
    pub fn new(data_dir: &Path) -> Result<Self, StoreError> {
        let tasks = data_dir.join("tasks");
        std::fs::create_dir_all(&tasks)?;
        Ok(Self {
            tasks,
            lock: std::sync::Mutex::new(()),
        })
    }

    fn path_for(&self, task_id: &str) -> Result<std::path::PathBuf, StoreError> {
        // A task id is also a file name. Accept only a plain-file-name
        // alphabet (ASCII alphanumeric, '-', '_') so a hostile id cannot
        // escape the tasks directory: no '/' or '\\' (path separators),
        // no '.' (so ".." cannot traverse), no ':' (Windows drive syntax
        // and alternate-data-stream names both need it). Plan section 16.
        let safe = !task_id.is_empty()
            && task_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !safe {
            return Err(StoreError::InvalidTaskId(task_id.to_string()));
        }
        Ok(self.tasks.join(format!("{task_id}.json")))
    }

    fn read(&self, task_id: &str) -> Result<TaskRecord, StoreError> {
        let path = self.path_for(task_id)?;
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::NotFound(task_id.to_string()));
            }
            Err(e) => return Err(StoreError::Io(e)),
        };
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
            task_id: task_id.to_string(),
            detail: e.to_string(),
        })
    }

    fn write(&self, record: &TaskRecord) -> Result<(), StoreError> {
        let path = self.path_for(&record.contract.id)?;
        let tmp = self
            .tasks
            .join(format!("{}.{}.tmp", record.contract.id, std::process::id()));
        let bytes = serde_json::to_vec_pretty(record)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &path).inspect_err(|_| {
            // Best-effort: a failed rename may leave a temp file behind;
            // the previous record is untouched, which is the important part.
            let _ = std::fs::remove_file(&tmp);
        })?;
        Ok(())
    }

    /// Read-modify-write, serialized by the handle's mutex so in-process
    /// callers never share the pid-only temp path or lose a transition.
    /// The atomic rename is what makes each individual write durable;
    /// cross-process locking is M2 scope.
    fn mutate<F>(&self, task_id: &str, f: F) -> Result<TaskRecord, StoreError>
    where
        F: FnOnce(&mut TaskRecord) -> Result<(), StoreError>,
    {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut record = self.read(task_id)?;
        f(&mut record)?;
        record.version += 1;
        self.write(&record)?;
        Ok(record)
    }
}

impl Store for FileStore {
    fn create(&self, contract: &TaskContract) -> Result<(), StoreError> {
        validate_contract(contract)?;
        let path = self.path_for(&contract.id)?;
        if path.exists() {
            return Err(StoreError::AlreadyExists(contract.id.clone()));
        }
        let record = TaskRecord {
            contract: contract.clone(),
            state: TaskState::Created,
            attempts: Vec::new(),
            sessions: Vec::new(),
            // The journal starts empty: admission (`Admit`) is the first
            // audited transition, recorded by `transition`, not here.
            events: Vec::new(),
            version: 1,
        };
        self.write(&record)
    }

    fn get(&self, task_id: &str) -> Result<TaskRecord, StoreError> {
        self.read(task_id)
    }

    fn list(&self) -> Result<Vec<TaskRecord>, StoreError> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.tasks)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(stem) = name.to_str().and_then(|s| s.strip_suffix(".json")) else {
                continue; // skip .tmp and foreign files
            };
            out.push(self.read(stem)?);
        }
        out.sort_by(|a, b| a.contract.id.cmp(&b.contract.id));
        Ok(out)
    }

    fn transition(&self, task_id: &str, event: TaskEvent) -> Result<AuditEvent, StoreError> {
        // The next state and audit event are computed from the record
        // being mutated, inside the same locked read-modify-write, so the
        // state and its journal entry always come from the same state.
        let mut audit = None;
        self.mutate(task_id, |r| {
            let (next, a) = transition(r.state, event)?;
            r.state = next;
            r.events.push(a);
            audit = Some(a);
            Ok(())
        })?;
        Ok(audit.expect("mutate returned Ok without running the closure"))
    }

    fn add_attempt(&self, task_id: &str, attempt: &Attempt) -> Result<(), StoreError> {
        self.mutate(task_id, |r| {
            if r.attempts.iter().any(|a| a.id == attempt.id) {
                return Err(StoreError::DuplicateAttempt {
                    attempt_id: attempt.id.clone(),
                    task_id: task_id.to_string(),
                });
            }
            // Bind the attempt to this task's contract and keep its
            // numbering sequential from 1, so a stray or reordered row
            // cannot be slipped into the history.
            if attempt.task_id != r.contract.id {
                return Err(StoreError::CrossTaskAttempt {
                    attempt_id: attempt.id.clone(),
                    attempt_task: attempt.task_id.clone(),
                    record_task: r.contract.id.clone(),
                });
            }
            let expected = r.attempts.len() as u32 + 1;
            if attempt.attempt_number != expected {
                return Err(StoreError::BadAttemptNumber {
                    expected,
                    got: attempt.attempt_number,
                    task_id: task_id.to_string(),
                });
            }
            r.attempts.push(attempt.clone());
            Ok(())
        })?;
        Ok(())
    }

    fn update_attempt_state(
        &self,
        task_id: &str,
        attempt_id: &str,
        state: AttemptState,
    ) -> Result<(), StoreError> {
        self.mutate(task_id, |r| {
            let a = r
                .attempts
                .iter_mut()
                .find(|a| a.id == attempt_id)
                .ok_or_else(|| StoreError::NoAttempt {
                    attempt_id: attempt_id.to_string(),
                    task_id: task_id.to_string(),
                })?;
            if a.state.is_terminal() {
                return Err(StoreError::TerminalAttempt {
                    attempt_id: attempt_id.to_string(),
                    state: a.state,
                });
            }
            if a.task_id != r.contract.id {
                return Err(StoreError::CrossTaskAttempt {
                    attempt_id: attempt_id.to_string(),
                    attempt_task: a.task_id.clone(),
                    record_task: r.contract.id.clone(),
                });
            }
            a.state = state;
            Ok(())
        })?;
        Ok(())
    }

    fn add_session(&self, task_id: &str, session: &Session) -> Result<(), StoreError> {
        self.mutate(task_id, |r| {
            if r.sessions.iter().any(|s| s.id == session.id) {
                return Err(StoreError::DuplicateSession {
                    session_id: session.id.clone(),
                    task_id: task_id.to_string(),
                });
            }
            // A session belongs to exactly one attempt; it must be one this
            // task already recorded.
            if !r.attempts.iter().any(|a| a.id == session.attempt_id) {
                return Err(StoreError::UnknownAttempt {
                    session_id: session.id.clone(),
                    attempt_id: session.attempt_id.clone(),
                });
            }
            r.sessions.push(session.clone());
            Ok(())
        })?;
        Ok(())
    }

    fn update_session_state(
        &self,
        task_id: &str,
        session_id: &str,
        state: SessionState,
    ) -> Result<(), StoreError> {
        self.mutate(task_id, |r| {
            let s = r
                .sessions
                .iter_mut()
                .find(|s| s.id == session_id)
                .ok_or_else(|| StoreError::NoSession {
                    session_id: session_id.to_string(),
                    task_id: task_id.to_string(),
                })?;
            if s.state.is_terminal() {
                return Err(StoreError::TerminalSession {
                    session_id: session_id.to_string(),
                    state: s.state,
                });
            }
            s.state = state;
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskKind, VerificationPolicy,
    };

    /// A fresh unique data directory per test. `temp_dir + pid + counter`
    /// keeps tests independent without a tempfile dependency.
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn new_dir() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "orch-store-test-{}-{n}",
            std::process::id()
        ));
        FileStore::new(&dir).unwrap();
        dir
    }

    fn contract(id: &str) -> TaskContract {
        TaskContract {
            id: id.into(),
            project_id: "p1".into(),
            parent_id: None,
            objective: "Implement the thing".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: vec![],
            verification: VerificationPolicy::Commands {
                commands: vec![vec!["cargo".into(), "test".into()]],
            },
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
        }
    }

    fn attempt(id: &str, task_id: &str, n: u32) -> Attempt {
        Attempt {
            id: id.into(),
            task_id: task_id.into(),
            attempt_number: n,
            state: AttemptState::Pending,
        }
    }

    fn session(id: &str, attempt_id: &str) -> Session {
        Session {
            id: id.into(),
            attempt_id: attempt_id.into(),
            adapter: "codex".into(),
            state: SessionState::Spawning,
        }
    }

    /// Drive a record (currently in `from`) through the happy path up to
    /// `up_to`, which must be a later state on that path.
    fn drive(store: &FileStore, task: &str, from: TaskState, up_to: TaskState) {
        let events = [
            (TaskEvent::Admit, TaskState::Queued),
            (TaskEvent::ExecutionStarted, TaskState::Running),
            (TaskEvent::CandidateReady, TaskState::Verifying),
            (
                TaskEvent::VerificationOutcome { passed: true },
                TaskState::Succeeded,
            ),
        ];
        // `from` is the state we're in; each event's target names the state
        // it moves us into, so the first event to apply is the one after the
        // one whose target is `from` (all events if `from` is Created).
        let started = events
            .iter()
            .position(|(_, next)| *next == from)
            .map(|i| i + 1)
            .unwrap_or(0);
        for (event, next) in &events[started..] {
            store.transition(task, *event).unwrap();
            if *next == up_to {
                return;
            }
        }
        panic!("drive: never reached {up_to:?}");
    }

    #[test]
    fn create_persists_validated_contract_and_rejects_invalid() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        let rec = store.get("t1").unwrap();
        assert_eq!(rec.contract, contract("t1"));
        assert_eq!(rec.state, TaskState::Created);
        assert!(rec.attempts.is_empty());
        assert!(rec.sessions.is_empty());

        // Duplicate create is rejected, original untouched.
        assert!(matches!(
            store.create(&contract("t1")),
            Err(StoreError::AlreadyExists(_))
        ));

        // An invalid contract never produces a record.
        let mut blank = contract("bad2");
        blank.objective = "   ".into();
        assert!(matches!(
            store.create(&blank),
            Err(StoreError::InvalidContract(ContractError::EmptyObjective))
        ));
        assert!(matches!(store.get("bad2"), Err(StoreError::NotFound(_))));

        // list sees t1 only.
        let ids: Vec<_> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|r| r.contract.id)
            .collect();
        assert_eq!(ids, vec!["t1"]);
    }

    #[test]
    fn close_and_reopen_preserves_everything() {
        let dir = new_dir();
        {
            let store = FileStore::new(&dir).unwrap();
            store.create(&contract("t1")).unwrap();
            store.transition("t1", TaskEvent::Admit).unwrap();
            store
                .add_attempt("t1", &attempt("a1", "t1", 1))
                .unwrap();
            store
                .add_session("t1", &session("s1", "a1"))
                .unwrap();
            store
                .update_session_state("t1", "s1", SessionState::Exited)
                .unwrap();
            store.transition("t1", TaskEvent::ExecutionStarted).unwrap();
        }
        // A brand-new handle over the same directory is the reopen.
        let store = FileStore::new(&dir).unwrap();
        let rec = store.get("t1").unwrap();
        assert_eq!(rec.contract, contract("t1"));
        assert_eq!(rec.state, TaskState::Running);
        assert_eq!(rec.attempts.len(), 1);
        assert_eq!(rec.attempts[0].id, "a1");
        assert_eq!(rec.sessions[0].state, SessionState::Exited);
        assert_eq!(
            rec.events,
            vec![AuditEvent::TaskAdmitted, AuditEvent::ExecutionStarted]
        );
        assert!(rec.version >= 5);
    }

    #[test]
    fn invalid_transition_changes_nothing() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        let before = store.get("t1").unwrap();

        // Created cannot start execution without Admit, and no other event
        // is legal from it. Every one of these must fail with a typed error.
        for event in [
            TaskEvent::ExecutionStarted,
            TaskEvent::CandidateReady,
            TaskEvent::VerificationOutcome { passed: true },
            TaskEvent::ExecutionFailed,
            TaskEvent::RetryRequested,
        ] {
            let err = store.transition("t1", event).unwrap_err();
            assert!(
                matches!(err, StoreError::IllegalTransition(TransitionError::Illegal { from: TaskState::Created, .. })),
                "expected Illegal from Created, got {err:?}"
            );
        }
        // Succeeded is irreversible: no event may leave it.
        store.transition("t1", TaskEvent::Admit).unwrap();
        drive(&store, "t1", TaskState::Queued, TaskState::Succeeded);
        let after_success = store.get("t1").unwrap();
        for event in [
            TaskEvent::Admit,
            TaskEvent::RetryRequested,
            TaskEvent::CancelRequested,
            TaskEvent::ExecutionFailed,
        ] {
            assert!(
                store.transition("t1", event).is_err(),
                "terminal Succeeded accepted {event:?}"
            );
        }
        // The record is byte-for-byte the pre-failure snapshot (plus the
        // successful transitions): no state drift, no spurious journal rows.
        let after = store.get("t1").unwrap();
        assert_eq!(after, after_success);
        assert_ne!(before.state, TaskState::Succeeded);
    }

    #[test]
    fn state_change_and_audit_event_persist_together() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        // Walk every edge of the state table that leads to Succeeded and
        // verify the journal matches the state one-for-one, in order.
        store.transition("t1", TaskEvent::Admit).unwrap();
        store
            .transition("t1", TaskEvent::ExecutionStarted)
            .unwrap();
        store.transition("t1", TaskEvent::CandidateReady).unwrap();
        store
            .transition("t1", TaskEvent::VerificationOutcome { passed: true })
            .unwrap();
        let rec = store.get("t1").unwrap();
        assert_eq!(rec.state, TaskState::Succeeded);
        assert_eq!(
            rec.events,
            vec![
                AuditEvent::TaskAdmitted,
                AuditEvent::ExecutionStarted,
                AuditEvent::CandidateRecorded,
                AuditEvent::VerificationPassed,
            ]
        );
    }

    #[test]
    fn failed_attempt_preserved_and_retry_appends_new_one() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        store.transition("t1", TaskEvent::Admit).unwrap();
        store
            .add_attempt("t1", &attempt("a1", "t1", 1))
            .unwrap();
        store
            .update_attempt_state("t1", "a1", AttemptState::Running)
            .unwrap();
        store.transition("t1", TaskEvent::ExecutionStarted).unwrap();
        store
            .update_attempt_state("t1", "a1", AttemptState::Failed)
            .unwrap();
        // Task fails; retry is legal and moves it back to Queued.
        store.transition("t1", TaskEvent::ExecutionFailed).unwrap();
        assert_eq!(store.get("t1").unwrap().state, TaskState::Failed);
        store.transition("t1", TaskEvent::RetryRequested).unwrap();
        assert_eq!(store.get("t1").unwrap().state, TaskState::Queued);
        // The retry creates attempt 2. Attempt 1 stays Failed, forever.
        store
            .add_attempt("t1", &attempt("a2", "t1", 2))
            .unwrap();
        let rec = store.get("t1").unwrap();
        assert_eq!(rec.attempts.len(), 2);
        assert_eq!(rec.attempts[0].id, "a1");
        assert_eq!(rec.attempts[0].state, AttemptState::Failed);
        assert_eq!(rec.attempts[1].id, "a2");
        assert_eq!(rec.attempts[1].state, AttemptState::Pending);
        // A terminal attempt cannot be rewritten back to Running.
        assert!(matches!(
            store.update_attempt_state("t1", "a1", AttemptState::Running),
            Err(StoreError::TerminalAttempt { .. })
        ));
        // The journal proves the failed attempt's history: execution start,
        // execution failure, retry scheduled — all still present.
        assert_eq!(
            rec.events,
            vec![
                AuditEvent::TaskAdmitted,
                AuditEvent::ExecutionStarted,
                AuditEvent::ExecutionFailed,
                AuditEvent::RetryScheduled,
            ]
        );
    }

    #[test]
    fn session_exit_does_not_affect_task_state() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        store.transition("t1", TaskEvent::Admit).unwrap();
        store
            .add_attempt("t1", &attempt("a1", "t1", 1))
            .unwrap();
        store
            .add_session("t1", &session("s1", "a1"))
            .unwrap();
        store.transition("t1", TaskEvent::ExecutionStarted).unwrap();
        // The agent process exits (cleanly, with whatever it thinks is
        // success). None of these may move the task: session facts report
        // into the attempt, and only the supervisor's translated task event
        // moves the task.
        store
            .update_session_state("t1", "s1", SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state("t1", "a1", AttemptState::CandidateReady)
            .unwrap();
        let rec = store.get("t1").unwrap();
        assert_eq!(rec.state, TaskState::Running);
        assert_eq!(rec.sessions[0].state, SessionState::Exited);
        // Terminal sessions are immutable too: no "Exited then Aborted".
        assert!(matches!(
            store.update_session_state("t1", "s1", SessionState::Aborted),
            Err(StoreError::TerminalSession { .. })
        ));
        // Only the engine's verification outcome can make the task succeed.
        store.transition("t1", TaskEvent::CandidateReady).unwrap();
        assert_eq!(store.get("t1").unwrap().state, TaskState::Verifying);
        store
            .transition("t1", TaskEvent::VerificationOutcome { passed: true })
            .unwrap();
        assert_eq!(store.get("t1").unwrap().state, TaskState::Succeeded);
    }

    #[test]
    fn session_must_reference_recorded_attempt() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        // No attempt recorded yet: the session cannot exist.
        assert!(matches!(
            store.add_session("t1", &session("s1", "ghost")),
            Err(StoreError::UnknownAttempt { .. })
        ));
    }

    #[test]
    fn list_ignores_foreign_files() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        std::fs::write(dir.join("tasks").join("stray.txt"), b"junk").unwrap();
        std::fs::write(dir.join("tasks").join("t1.42.tmp"), b"partial").unwrap();
        let ids: Vec<_> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|r| r.contract.id)
            .collect();
        assert_eq!(ids, vec!["t1"]);
    }

    #[test]
    fn invalid_task_ids_are_rejected_not_panics() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        // Separators, traversal, Windows drive/alternate-stream syntax,
        // dots, NUL, and empty: none of these may reach the filesystem.
        for bad in [
            "", ".", "..", "a/b", "a\\b", "C:evil", "C:/evil", "x.json",
            "a:b", "a\0b", "a b", "a.b",
        ] {
            match store.get(bad) {
                Err(StoreError::InvalidTaskId(_)) => {}
                other => panic!("expected InvalidTaskId for {bad:?}, got {other:?}"),
            }
        }
        // A contract whose id is not a plain file name is rejected by
        // `create` the same way.
        let hostile = contract("..");
        assert!(matches!(
            store.create(&hostile),
            Err(StoreError::InvalidTaskId(_))
        ));
        // Nothing escaped: no sibling of the data dir was created, and
        // the tasks dir gained no record.
        assert!(!dir.parent().unwrap().join("tasks..json").exists());
        let ids = store.list().unwrap().into_iter().map(|r| r.contract.id).collect::<Vec<_>>();
        assert!(ids.is_empty());
    }

    #[test]
    fn attempt_must_match_task_and_sequence() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        store.transition("t1", TaskEvent::Admit).unwrap();
        // A numbered attempt belonging to a different task is rejected.
        assert!(matches!(
            store.add_attempt("t1", &attempt("a1", "other-task", 1)),
            Err(StoreError::CrossTaskAttempt { .. })
        ));
        // The first attempt must be number 1.
        assert!(matches!(
            store.add_attempt("t1", &attempt("a1", "t1", 2)),
            Err(StoreError::BadAttemptNumber { expected: 1, got: 2, .. })
        ));
        store.add_attempt("t1", &attempt("a1", "t1", 1)).unwrap();
        // The next one must be 2, and the id must be fresh.
        assert!(matches!(
            store.add_attempt("t1", &attempt("a2", "t1", 1)),
            Err(StoreError::BadAttemptNumber { expected: 2, got: 1, .. })
        ));
        assert!(matches!(
            store.add_attempt("t1", &attempt("a1", "t1", 2)),
            Err(StoreError::DuplicateAttempt { .. })
        ));
        store.add_attempt("t1", &attempt("a2", "t1", 2)).unwrap();
        // A recorded attempt is updatable on its own task.
        store
            .update_attempt_state("t1", "a2", AttemptState::Running)
            .unwrap();
        assert_eq!(store.get("t1").unwrap().attempts[1].state, AttemptState::Running);
    }

    #[test]
    fn session_cannot_carry_a_foreign_attempt() {
        let dir = new_dir();
        let store = FileStore::new(&dir).unwrap();
        store.create(&contract("t1")).unwrap();
        store.transition("t1", TaskEvent::Admit).unwrap();
        store.add_attempt("t1", &attempt("a1", "t1", 1)).unwrap();
        // A session referencing an attempt that is not recorded on this
        // task (i.e. an attempt of some other task) is rejected.
        assert!(matches!(
            store.add_session("t1", &session("s1", "a-from-t2")),
            Err(StoreError::UnknownAttempt { .. })
        ));
        // And a session on the recorded attempt works.
        store.add_session("t1", &session("s1", "a1")).unwrap();
        assert_eq!(store.get("t1").unwrap().sessions[0].attempt_id, "a1");
    }

    #[test]
    fn concurrent_mutations_do_not_lose_a_transition() {
        use std::sync::Arc;
        let dir = new_dir();
        let store = Arc::new(FileStore::new(&dir).unwrap());
        store.create(&contract("t1")).unwrap();
        store.transition("t1", TaskEvent::Admit).unwrap();
        // Add 4 attempts from 4 threads against one shared handle. The
        // mutex serializes them, so whichever order they acquire the lock
        // in, the record must be a contiguous prefix 1..=k of the
        // sequence — no gaps, no duplicates, no lost writes.
        let handles: Vec<_> = (1..=4u32)
            .map(|n| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    store
                        .add_attempt("t1", &attempt(&format!("a{n}"), "t1", n))
                        .is_ok()
                })
            })
            .collect();
        let ok: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        // Exactly the thread that got the lock first for each number
        // succeeds; a racing caller may lose with BadAttemptNumber. The
        // record, however, must be a valid prefix 1..=k of the sequence.
        let rec = store.get("t1").unwrap();
        assert!(!rec.attempts.is_empty());
        for (i, a) in rec.attempts.iter().enumerate() {
            assert_eq!(a.attempt_number, i as u32 + 1, "sequence must stay contiguous");
        }
        assert!(ok.iter().any(|&b| b), "at least one attempt must land");
    }
}
