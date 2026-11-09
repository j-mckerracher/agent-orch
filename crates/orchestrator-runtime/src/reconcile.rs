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
/// Unlike [`observe_process`], this ignores process-group membership. A
/// missing start time or inconclusive OS query remains Unknown; a changed
/// start time for the same PID proves the recorded process has exited.
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
            ProcessClassification::IdentityMismatch => ProcessClassification::Exited,
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
