//! OS process identity and pure reconciliation outcomes.
//!
//! Runtime captures OS fields, and persistence stores this value separately
//! from the core session lifecycle. CLI/scheduler wiring is deferred to M2-06;
//! delivery inspection is deferred to M3-06.
//!
//! Updated: 2026-11-08T08:00:58+00:00.

/// Platform tag identifies which process fields can be compared reliably.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ProcessPlatform {
    #[serde(rename = "macos")]
    MacOS,
    Unix,
    Windows,
    Other,
}

impl ProcessPlatform {
    /// Stable database tag for this platform family.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::MacOS => "macos",
            Self::Unix => "unix",
            Self::Windows => "windows",
            Self::Other => "other",
        }
    }

    /// Parses a stored platform tag.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "macos" => Some(Self::MacOS),
            "unix" => Some(Self::Unix),
            "windows" => Some(Self::Windows),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// Launch-time identity used to distinguish a process from a reused PID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub process_group: Option<u32>,
    /// OS process start time in microseconds since Unix epoch, when available.
    pub start_time_micros: Option<u64>,
    pub platform: ProcessPlatform,
}

/// OS observation supplied to the pure process planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessObservation {
    Alive(ProcessIdentity),
    Exited,
    Unknown,
}

/// Process fact, distinct from task or attempt success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessClassification {
    Alive,
    Exited,
    IdentityMismatch,
    Unknown,
}

/// Action proposed for a process record. No action represents task success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    LeaveRunning,
    MarkInterrupted,
    MarkNeedsReview,
}

/// Pure process-reconciliation decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessPlan {
    pub classification: ProcessClassification,
    pub action: ProcessAction,
}

/// Classifies one identity observation and maps it to a non-success action.
pub fn plan_process_reconciliation(
    expected: ProcessIdentity,
    observation: ProcessObservation,
) -> ProcessPlan {
    let classification = match observation {
        ProcessObservation::Alive(observed) => classify_identity(expected, observed),
        ProcessObservation::Exited => ProcessClassification::Exited,
        ProcessObservation::Unknown => ProcessClassification::Unknown,
    };
    let action = match classification {
        ProcessClassification::Alive => ProcessAction::LeaveRunning,
        ProcessClassification::Exited => ProcessAction::MarkInterrupted,
        ProcessClassification::IdentityMismatch | ProcessClassification::Unknown => {
            ProcessAction::MarkNeedsReview
        }
    };
    ProcessPlan {
        classification,
        action,
    }
}

/// Compares identity snapshots without consulting the operating system.
pub fn classify_identity(
    expected: ProcessIdentity,
    observed: ProcessIdentity,
) -> ProcessClassification {
    if expected.pid != observed.pid || expected.platform != observed.platform {
        return ProcessClassification::IdentityMismatch;
    }
    if differs(expected.process_group, observed.process_group)
        || differs(expected.start_time_micros, observed.start_time_micros)
    {
        return ProcessClassification::IdentityMismatch;
    }
    if unavailable(expected.process_group, observed.process_group)
        || unavailable(expected.start_time_micros, observed.start_time_micros)
        || expected.process_group.is_none() && expected.platform != ProcessPlatform::Windows
        || expected.platform == ProcessPlatform::MacOS && expected.start_time_micros.is_none()
    {
        return ProcessClassification::Unknown;
    }
    ProcessClassification::Alive
}

fn differs<T: PartialEq>(expected: Option<T>, observed: Option<T>) -> bool {
    matches!((expected, observed), (Some(expected), Some(observed)) if expected != observed)
}

fn unavailable<T>(expected: Option<T>, observed: Option<T>) -> bool {
    expected.is_some() && observed.is_none()
}
