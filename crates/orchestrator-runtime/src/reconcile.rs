//! Pure process-reconciliation planning for unfinished attempt and session records.
//!
//! Callers capture [`ProcessIdentity`] immediately after launch and pass the
//! snapshot with each unfinished attempt or session. Core records currently
//! lack process-identity fields, so persistence and launch-site wiring remain
//! caller responsibilities. This module observes processes but never changes
//! task state or terminates processes. An exit only yields `MarkInterrupted`;
//! task success requires the separate verification flow.
//!
//! Updated: 2026-11-08T06:51:00+00:00.

use std::io;

use crate::supervisor::is_process_running;
#[cfg(unix)]
use crate::supervisor::process_group_alive;

/// Process identity captured after launch. macOS records process group and
/// kernel start time so PID reuse cannot impersonate the original process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub process_group: Option<u32>,
    pub start_time: Option<ProcessStartTime>,
}

/// Kernel-reported process creation time with microsecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessStartTime {
    pub seconds: u64,
    pub microseconds: u64,
}

impl ProcessIdentity {
    /// Captures process metadata while `pid` is expected to be alive.
    pub fn capture(pid: u32) -> io::Result<Self> {
        if pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process ID must be nonzero",
            ));
        }
        let os_pid = i32::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process ID exceeds range"))?;

        #[cfg(target_os = "macos")]
        {
            let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
            let size = std::mem::size_of::<libc::proc_bsdinfo>();
            // libproc writes this C-layout struct; require full size before reading it.
            let received = unsafe {
                libc::proc_pidinfo(
                    os_pid,
                    libc::PROC_PIDTBSDINFO,
                    0,
                    info.as_mut_ptr().cast(),
                    i32::try_from(size).expect("process info struct fits in i32"),
                )
            };
            if received != i32::try_from(size).expect("process info struct fits in i32") {
                return Err(io::Error::last_os_error());
            }
            let info = unsafe { info.assume_init() };
            if info.pbi_pid != pid {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "process disappeared during identity capture",
                ));
            }
            Ok(Self {
                pid,
                process_group: Some(info.pbi_pgid),
                start_time: Some(ProcessStartTime {
                    seconds: info.pbi_start_tvsec,
                    microseconds: info.pbi_start_tvusec,
                }),
            })
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // `os_pid` is validated and `getpgid` only observes process metadata.
            let group = unsafe { libc::getpgid(os_pid) };
            if group < 0 {
                return Err(io::Error::last_os_error());
            }
            return Ok(Self {
                pid,
                process_group: Some(u32::try_from(group).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid process group")
                })?),
                start_time: None,
            });
        }

        #[cfg(not(unix))]
        {
            let _ = os_pid;
            Ok(Self {
                pid,
                process_group: None,
                start_time: None,
            })
        }
    }
}

/// One unfinished attempt or session whose process must be observed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessSubject {
    Attempt { task_id: String, attempt_id: String },
    Session { task_id: String, session_id: String },
}

/// Caller-supplied identity for one unfinished attempt or session. Include
/// each nonterminal process record exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedProcess {
    pub subject: ProcessSubject,
    pub identity: ProcessIdentity,
}

/// OS observation, distinct from task or attempt outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessClassification {
    Alive,
    Exited,
    IdentityMismatch,
    Unknown,
}

/// Reconciliation action. No variant represents task success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    LeaveRunning,
    MarkInterrupted,
    MarkNeedsReview,
}

/// One deterministic process decision in the reconciliation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessDecision {
    pub subject: ProcessSubject,
    pub identity: ProcessIdentity,
    pub classification: ProcessClassification,
    pub action: ProcessAction,
}

/// Resolution state used by the scheduling gate for process, workspace, and
/// delivery records. Non-resolved records prevent scheduling from resuming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationResolution {
    Resolved,
    Unresolved,
    Ambiguous,
}

impl ProcessClassification {
    pub fn action(self) -> ProcessAction {
        match self {
            Self::Alive => ProcessAction::LeaveRunning,
            Self::Exited => ProcessAction::MarkInterrupted,
            Self::IdentityMismatch | Self::Unknown => ProcessAction::MarkNeedsReview,
        }
    }

    pub fn resolution(self) -> ReconciliationResolution {
        match self {
            Self::Alive | Self::Exited => ReconciliationResolution::Resolved,
            Self::IdentityMismatch | Self::Unknown => ReconciliationResolution::Unresolved,
        }
    }
}

/// Compares two observations without consulting the operating system.
pub fn classify_identity(
    expected: ProcessIdentity,
    observed: ProcessIdentity,
) -> ProcessClassification {
    if expected.pid != observed.pid
        || field_mismatch(expected.process_group, observed.process_group)
        || field_mismatch(expected.start_time, observed.start_time)
    {
        return ProcessClassification::IdentityMismatch;
    }
    if field_unavailable(expected.process_group, observed.process_group)
        || field_unavailable(expected.start_time, observed.start_time)
    {
        return ProcessClassification::Unknown;
    }
    ProcessClassification::Alive
}

fn field_mismatch<T: PartialEq>(expected: Option<T>, observed: Option<T>) -> bool {
    matches!((expected, observed), (Some(expected), Some(observed)) if expected != observed)
}

fn field_unavailable<T>(expected: Option<T>, observed: Option<T>) -> bool {
    expected.is_some() && observed.is_none()
}

pub fn observe_process(expected: ProcessIdentity) -> ProcessClassification {
    if !is_process_running(expected.pid) {
        return if recorded_process_group_alive(expected.process_group) {
            ProcessClassification::Unknown
        } else {
            ProcessClassification::Exited
        };
    }
    match ProcessIdentity::capture(expected.pid) {
        Ok(observed) => classify_identity(expected, observed),
        Err(_) => ProcessClassification::Unknown,
    }
}

#[cfg(unix)]
fn recorded_process_group_alive(group: Option<u32>) -> bool {
    group.is_some_and(process_group_alive)
}

#[cfg(not(unix))]
fn recorded_process_group_alive(_group: Option<u32>) -> bool {
    false
}

/// Classifies every supplied unfinished process and sorts decisions by owner
/// and recorded identity so identical observations produce identical plans.
pub fn reconcile_processes(records: &[RecordedProcess]) -> Vec<ProcessDecision> {
    let mut decisions: Vec<_> = records
        .iter()
        .map(|record| {
            let classification = observe_process(record.identity);
            ProcessDecision {
                subject: record.subject.clone(),
                identity: record.identity,
                classification,
                action: classification.action(),
            }
        })
        .collect();
    decisions.sort_by(|left, right| {
        left.subject
            .cmp(&right.subject)
            .then_with(|| left.identity.cmp(&right.identity))
    });
    decisions
}

/// Returns true only when every supplied unfinished record is resolved.
pub fn scheduling_may_resume(
    processes: &[ProcessDecision],
    other_records: &[ReconciliationResolution],
) -> bool {
    processes
        .iter()
        .all(|decision| decision.classification.resolution() == ReconciliationResolution::Resolved)
        && other_records
            .iter()
            .all(|resolution| *resolution == ReconciliationResolution::Resolved)
}
