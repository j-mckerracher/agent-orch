//! Pure process-reconciliation planning for unfinished attempt and session records.
//!
//! The supervisor captures identity at launch; SQLite persists it separately
//! from core session state. macOS and Linux capture process group and start
//! time; Windows cancellation owners also retain exact creation time.
//! Reconciliation observes but never mutates or kills.
//!
//! Updated: 2026-11-08T08:24:47+00:00.

use std::io;

use orchestrator_core::identity::{
    classify_exact_process_identity, plan_process_reconciliation, ProcessObservation,
    ProcessPlatform,
};
pub use orchestrator_core::identity::{
    ExactProcessIdentity, ProcessAction, ProcessClassification, ProcessIdentity,
};
use orchestrator_core::task::{AttemptState, TaskState};
use orchestrator_git::delivery::{UnsafeToDeleteReason, WorktreeDeletionSafety};
use orchestrator_git::delivery_reconcile::{DeliveryClassification, DeliveryStepDecision};
use orchestrator_git::reconcile::{WorkspaceClassification, WorkspaceDecision};

#[cfg(unix)]
use crate::supervisor::process_group_alive;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Captures launch-time OS metadata. Unsupported or unavailable metadata
/// returns an error so the supervisor can retain `None` without failing launch.
pub fn capture_process_identity(pid: u32) -> io::Result<ProcessIdentity> {
    capture_exact_process_identity(pid).map(|identity| identity.identity)
}

/// Captures an identity suitable for durable cancellation-owner recovery.
///
/// Windows retains its exact process-creation time; other platforms use the
/// process identity available from [`capture_process_identity`].
pub fn capture_exact_process_identity(pid: u32) -> io::Result<ExactProcessIdentity> {
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
        Ok(ExactProcessIdentity {
            identity: ProcessIdentity {
                pid,
                process_group: Some(info.pbi_pgid),
                start_time_micros: Some(start_time_micros),
                platform: ProcessPlatform::MacOS,
            },
            windows_creation_time_100ns_since_unix_epoch: None,
        })
    }

    #[cfg(target_os = "linux")]
    {
        let (group, start_time_micros) = linux_process_identity(pid)?;
        Ok(ExactProcessIdentity {
            identity: ProcessIdentity {
                pid,
                process_group: Some(group),
                start_time_micros: Some(start_time_micros),
                platform: ProcessPlatform::Unix,
            },
            windows_creation_time_100ns_since_unix_epoch: None,
        })
    }

    #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
    {
        let os_pid = i32::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process ID exceeds range"))?;
        // `os_pid` is validated and `getpgid` only observes process metadata.
        let group = unsafe { libc::getpgid(os_pid) };
        if group < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ExactProcessIdentity {
            identity: ProcessIdentity {
                pid,
                process_group: Some(u32::try_from(group).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid process group")
                })?),
                start_time_micros: None,
                platform: ProcessPlatform::Unix,
            },
            windows_creation_time_100ns_since_unix_epoch: None,
        })
    }

    #[cfg(windows)]
    {
        capture_windows_process_identity(pid)
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
        Err(error)
            if process_is_absent(&error)
                && recorded_process_group_alive(expected.process_group) =>
        {
            ProcessObservation::Unknown
        }
        Err(error) if process_is_absent(&error) => ProcessObservation::Exited,
        Err(_) => ProcessObservation::Unknown,
    }
}

/// Observes the exact recorded process for cancellation-owner recovery.
///
/// Unlike [`observe_process`], this ignores process-group membership for live
/// processes. A missing start time or inconclusive OS query remains Unknown.
/// A changed start time proves PID reuse on Windows (immutable creation time).
/// On Unix it may be wall-clock drift, so it counts as exited only when the
/// recorded process group is provably gone; otherwise it stays Unknown.
pub fn observe_owner_process(expected: ExactProcessIdentity) -> ProcessClassification {
    if expected.identity.pid == 0 {
        return ProcessClassification::Unknown;
    }

    match capture_exact_process_identity(expected.identity.pid) {
        Ok(observed)
            if observed.identity.pid != expected.identity.pid
                || observed.identity.platform != expected.identity.platform =>
        {
            ProcessClassification::Unknown
        }
        Ok(observed) => match classify_exact_process_identity(expected, observed) {
            ProcessClassification::IdentityMismatch => {
                match (expected.identity.platform, expected.identity.process_group) {
                    (ProcessPlatform::Windows, _) => ProcessClassification::Exited,
                    (_, Some(group)) if !recorded_process_group_alive(Some(group)) => {
                        ProcessClassification::Exited
                    }
                    _ => ProcessClassification::Unknown,
                }
            }
            classification => classification,
        },
        Err(error) if process_is_absent(&error) => ProcessClassification::Exited,
        Err(_) => ProcessClassification::Unknown,
    }
}

fn process_is_absent(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::NotFound {
        return true;
    }

    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ESRCH) || error.raw_os_error() == Some(libc::ENOENT)
    }
    #[cfg(not(unix))]
    {
        false
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

#[cfg(target_os = "linux")]
fn linux_process_identity(pid: u32) -> io::Result<(u32, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let command_end = stat
        .rfind(')')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process stat record"))?;
    let observed_pid = stat[..command_end]
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process ID"))?;
    if observed_pid != pid {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "process changed during identity capture",
        ));
    }

    let fields = stat[command_end + 1..]
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields
        .first()
        .is_some_and(|state| matches!(*state, "Z" | "X"))
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "process has exited",
        ));
    }
    let process_group = fields
        .get(2)
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|group| *group != 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process group"))?;
    let start_ticks = fields
        .get(19)
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process start time"))?;
    let boot_time_seconds = fs::read_to_string("/proc/stat")?
        .lines()
        .find_map(|line| {
            line.strip_prefix("btime ")
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing system boot time"))?;
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_second <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid process clock rate",
        ));
    }
    let start_offset_micros = start_ticks
        .checked_mul(1_000_000)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "start time overflow"))?
        / ticks_per_second as u64;
    let start_time_micros = boot_time_seconds
        .checked_mul(1_000_000)
        .and_then(|boot| boot.checked_add(start_offset_micros))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "start time overflow"))?;
    Ok((process_group, start_time_micros))
}

#[cfg(windows)]
fn capture_windows_process_identity(pid: u32) -> io::Result<ExactProcessIdentity> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "process ID does not exist",
            ));
        }
        return Err(error);
    }

    let result = (|| {
        let mut creation_time = FILETIME::default();
        let mut exit_time = FILETIME::default();
        let mut kernel_time = FILETIME::default();
        let mut user_time = FILETIME::default();
        if unsafe {
            GetProcessTimes(
                process,
                &mut creation_time,
                &mut exit_time,
                &mut kernel_time,
                &mut user_time,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if filetime_value(exit_time) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "process has exited",
            ));
        }
        let filetime_unix_epoch_offset = 116_444_736_000_000_000_u64;
        let unix_time_100ns = filetime_value(creation_time)
            .checked_sub(filetime_unix_epoch_offset)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "process creation time predates the Unix epoch",
                )
            })?;
        let start_time_micros = unix_time_100ns / 10;
        Ok(ExactProcessIdentity {
            identity: ProcessIdentity {
                pid,
                process_group: None,
                start_time_micros: Some(start_time_micros),
                platform: ProcessPlatform::Windows,
            },
            windows_creation_time_100ns_since_unix_epoch: Some(unix_time_100ns),
        })
    })();
    unsafe { CloseHandle(process) };
    result
}

#[cfg(windows)]
fn filetime_value(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
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

/// Observed workspace state for one interrupted task, derived by
/// [`workspace_evidence`]; `Unknown` means inspection was missing or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceEvidence {
    Matching,
    Missing,
    BranchMoved,
    Foreign,
    Dirty,
    Unknown,
}

/// Observed partial-delivery state, summarised by [`delivery_evidence`] from the
/// Git crate's delivery reconciliation steps: `Confirmed` = every step landed, `NotApplied` =
/// none landed, `Partial` = a mix, `Ambiguous` = any step unprovable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryEvidence {
    None,
    NotApplied,
    Partial,
    Confirmed,
    Ambiguous,
}

/// Recorded task facts plus observed process, workspace and delivery state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskEvidence {
    pub task: TaskState,
    pub attempt: AttemptState,
    pub process: ProcessClassification,
    pub workspace: WorkspaceEvidence,
    pub delivery: DeliveryEvidence,
}

/// Recovery class of an interrupted task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRecovery {
    /// Safe to resume or re-verify from the preserved workspace.
    Recoverable,
    /// Not resumable now (live owner, missing workspace, terminal failure);
    /// nothing is deleted or released.
    Blocked,
    /// Completion is recorded or proven by Git, never inferred from exit code.
    Completed,
    /// Ownership, workspace or delivery state is unproven; a human decides.
    RequiresInspection,
}

impl TaskRecovery {
    /// Feed into [`scheduling_may_resume`]'s `other_records`.
    pub fn resolution(self) -> ReconciliationResolution {
        match self {
            Self::RequiresInspection => ReconciliationResolution::Unresolved,
            _ => ReconciliationResolution::Resolved,
        }
    }
}

/// Conservatively classifies one interrupted task. Unproven ownership,
/// uncommitted or unlanded work, and partial or ambiguous delivery all fail
/// closed to `RequiresInspection`.
pub fn classify_interrupted_task(evidence: TaskEvidence) -> TaskRecovery {
    use TaskRecovery::*;
    match evidence.process {
        ProcessClassification::Alive => return Blocked,
        ProcessClassification::Exited => {}
        _ => return RequiresInspection,
    }
    match evidence.delivery {
        DeliveryEvidence::Partial | DeliveryEvidence::Ambiguous => return RequiresInspection,
        DeliveryEvidence::Confirmed => return Completed,
        DeliveryEvidence::None | DeliveryEvidence::NotApplied => {}
    }
    match evidence.task {
        TaskState::Succeeded => return Completed,
        TaskState::Failed | TaskState::VerificationFailed | TaskState::Cancelled => return Blocked,
        _ => {}
    }
    match evidence.workspace {
        WorkspaceEvidence::Matching => Recoverable,
        WorkspaceEvidence::Missing => Blocked,
        WorkspaceEvidence::BranchMoved if evidence.attempt == AttemptState::CandidateReady => {
            Recoverable
        }
        _ => RequiresInspection,
    }
}

/// Maps Git's workspace decision and worktree safety check to classifier input.
///
/// `safety` is `check_worktree_safe_to_delete` for a present worktree; a
/// present worktree with no safety result (not inspected, or inspection failed)
/// is `Unknown`. Dirty state is never treated as safe to discard.
pub fn workspace_evidence(
    decision: Option<&WorkspaceDecision>,
    safety: Option<&WorktreeDeletionSafety>,
) -> WorkspaceEvidence {
    let Some(decision) = decision else {
        return WorkspaceEvidence::Unknown;
    };
    let base = match decision.classification {
        WorkspaceClassification::Missing => return WorkspaceEvidence::Missing,
        WorkspaceClassification::Foreign => return WorkspaceEvidence::Foreign,
        WorkspaceClassification::PresentAndMatching => WorkspaceEvidence::Matching,
        WorkspaceClassification::BranchMoved => WorkspaceEvidence::BranchMoved,
    };
    match safety {
        None => WorkspaceEvidence::Unknown,
        Some(WorktreeDeletionSafety::SafeToDelete) => base,
        Some(WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::Dirty(_))) => {
            WorkspaceEvidence::Dirty
        }
        Some(WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::PrimaryCheckout)) => {
            WorkspaceEvidence::Foreign
        }
        Some(WorktreeDeletionSafety::Unsafe(_)) => WorkspaceEvidence::BranchMoved,
    }
}

/// Summarises one task's delivery steps: empty is `None`, any ambiguous step is
/// `Ambiguous`, a mix of landed and unlanded steps is `Partial`.
pub fn delivery_evidence(steps: &[DeliveryStepDecision]) -> DeliveryEvidence {
    let confirmed = steps
        .iter()
        .filter(|step| step.classification == DeliveryClassification::Confirmed)
        .count();
    let ambiguous = steps
        .iter()
        .any(|step| step.classification == DeliveryClassification::Ambiguous);
    match (steps.len(), confirmed, ambiguous) {
        (0, _, _) => DeliveryEvidence::None,
        (_, _, true) => DeliveryEvidence::Ambiguous,
        (total, landed, _) if landed == total => DeliveryEvidence::Confirmed,
        (_, 0, _) => DeliveryEvidence::NotApplied,
        _ => DeliveryEvidence::Partial,
    }
}
