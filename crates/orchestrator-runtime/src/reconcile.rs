//! Pure process-reconciliation planning for unfinished attempt and session records.
//!
//! The supervisor captures identity at launch; SQLite persists it separately
//! from core session state. macOS captures process group and start time. Other
//! Unix targets capture process group. Windows and unsupported targets return
//! no launch identity here. Reconciliation observes but never mutates or kills.
//! CLI/scheduler wiring is deferred to M2-06; delivery inspection is M3-06.
//!
//! Updated: 2026-11-08T08:24:47+00:00.

use std::io;

use orchestrator_core::identity::{
    plan_process_reconciliation, ProcessObservation, ProcessPlatform,
};
pub use orchestrator_core::identity::{ProcessAction, ProcessClassification, ProcessIdentity};

use crate::supervisor::is_process_running;
#[cfg(unix)]
use crate::supervisor::process_group_alive;

/// Captures launch-time OS metadata. Unsupported or unavailable metadata
/// returns an error so the supervisor can retain `None` without failing launch.
pub fn capture_process_identity(pid: u32) -> io::Result<ProcessIdentity> {
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process ID must be nonzero",
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let os_pid = i32::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process ID exceeds range"))?;
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let size = i32::try_from(size)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "process info too large"))?;
        // libproc writes this C-layout struct; read it only after a full-size result.
        let received = unsafe {
            libc::proc_pidinfo(
                os_pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if received != size {
            return Err(io::Error::last_os_error());
        }
        let info = unsafe { info.assume_init() };
        if info.pbi_pid != pid {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "process changed during identity capture",
            ));
        }
        let start_time_micros = info
            .pbi_start_tvsec
            .checked_mul(1_000_000)
            .and_then(|seconds| seconds.checked_add(info.pbi_start_tvusec))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "start time overflow"))?;
        Ok(ProcessIdentity {
            pid,
            process_group: Some(info.pbi_pgid),
            start_time_micros: Some(start_time_micros),
            platform: ProcessPlatform::MacOS,
        })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let os_pid = i32::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process ID exceeds range"))?;
        // `os_pid` is validated and `getpgid` only observes process metadata.
        let group = unsafe { libc::getpgid(os_pid) };
        if group < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ProcessIdentity {
            pid,
            process_group: Some(u32::try_from(group).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid process group")
            })?),
            start_time_micros: None,
            platform: ProcessPlatform::Unix,
        })
    }

    #[cfg(windows)]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows process identity capture is deferred",
        ))
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process identity capture is unsupported on this platform",
        ))
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

pub fn observe_process(expected: ProcessIdentity) -> ProcessObservation {
    match capture_process_identity(expected.pid) {
        Ok(observed) => ProcessObservation::Alive(observed),
        Err(_) if is_process_running(expected.pid) => ProcessObservation::Unknown,
        Err(_) if recorded_process_group_alive(expected.process_group) => {
            ProcessObservation::Unknown
        }
        Err(_) => ProcessObservation::Exited,
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
            let plan =
                plan_process_reconciliation(record.identity, observe_process(record.identity));
            ProcessDecision {
                subject: record.subject.clone(),
                identity: record.identity,
                classification: plan.classification,
                action: plan.action,
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

fn process_resolution(classification: ProcessClassification) -> ReconciliationResolution {
    match classification {
        ProcessClassification::Alive | ProcessClassification::Exited => {
            ReconciliationResolution::Resolved
        }
        ProcessClassification::IdentityMismatch | ProcessClassification::Unknown => {
            ReconciliationResolution::Unresolved
        }
    }
}

/// Returns true only when every supplied unfinished record is resolved.
pub fn scheduling_may_resume(
    processes: &[ProcessDecision],
    other_records: &[ReconciliationResolution],
) -> bool {
    processes.iter().all(|decision| {
        process_resolution(decision.classification) == ReconciliationResolution::Resolved
    }) && other_records
        .iter()
        .all(|resolution| *resolution == ReconciliationResolution::Resolved)
}
