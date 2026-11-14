//! OS process identity and pure reconciliation outcomes.
//!
//! Runtime captures OS fields, and persistence stores this value separately
//! from the core session lifecycle. The CLI uses these identities for
//! fail-closed reconciliation and stale-owner recovery.
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

/// Exact identity used for cancellation-owner recovery.
///
/// Windows stores the exact creation timestamp at 100-ns precision since the
/// Unix epoch. Existing process identities remain unchanged; legacy Windows
/// records without this value cannot prove liveness.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ExactProcessIdentity {
    #[serde(flatten)]
    pub identity: ProcessIdentity,
    #[serde(default)]
    pub windows_creation_time_100ns_since_unix_epoch: Option<u64>,
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

/// Compares cancellation-owner identities without consulting the operating
/// system. Legacy Windows records without exact creation time remain unknown.
pub fn classify_exact_process_identity(
    expected: ExactProcessIdentity,
    observed: ExactProcessIdentity,
) -> ProcessClassification {
    if expected.identity.pid != observed.identity.pid
        || expected.identity.platform != observed.identity.platform
    {
        return ProcessClassification::IdentityMismatch;
    }

    if expected.identity.platform == ProcessPlatform::Windows {
        let (Some(expected_creation_time), Some(observed_creation_time)) = (
            expected.windows_creation_time_100ns_since_unix_epoch,
            observed.windows_creation_time_100ns_since_unix_epoch,
        ) else {
            return ProcessClassification::Unknown;
        };
        let (Some(expected_start_time), Some(observed_start_time)) = (
            expected.identity.start_time_micros,
            observed.identity.start_time_micros,
        ) else {
            return ProcessClassification::Unknown;
        };
        if expected_creation_time / 10 != expected_start_time
            || observed_creation_time / 10 != observed_start_time
        {
            return ProcessClassification::Unknown;
        }
        return if expected_creation_time == observed_creation_time {
            ProcessClassification::Alive
        } else {
            ProcessClassification::IdentityMismatch
        };
    }

    if expected
        .windows_creation_time_100ns_since_unix_epoch
        .is_some()
        || observed
            .windows_creation_time_100ns_since_unix_epoch
            .is_some()
        || expected.identity.start_time_micros.is_none()
        || observed.identity.start_time_micros.is_none()
    {
        return ProcessClassification::Unknown;
    }
    // Exact owner identity is PID plus start time. A live process may move
    // between groups, so a group change must not look like an owner exit.
    let mut observed = observed.identity;
    if observed.process_group.is_some() {
        observed.process_group = expected.identity.process_group;
    }
    classify_identity(expected.identity, observed)
}

fn differs<T: PartialEq>(expected: Option<T>, observed: Option<T>) -> bool {
    matches!((expected, observed), (Some(expected), Some(observed)) if expected != observed)
}

fn unavailable<T>(expected: Option<T>, observed: Option<T>) -> bool {
    expected.is_some() && observed.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_windows_identity_without_exact_creation_time_is_unresolved() {
        let legacy: ExactProcessIdentity = serde_json::from_str(
            r#"{"pid":42,"process_group":null,"start_time_micros":1700000000123456,"platform":"windows"}"#,
        )
        .unwrap();
        assert_eq!(legacy.windows_creation_time_100ns_since_unix_epoch, None);
        assert_eq!(
            classify_exact_process_identity(legacy, legacy),
            ProcessClassification::Unknown
        );
    }

    #[test]
    fn windows_creation_time_difference_is_not_pid_liveness() {
        let expected = ExactProcessIdentity {
            identity: ProcessIdentity {
                pid: 42,
                process_group: None,
                start_time_micros: Some(1700000000123456),
                platform: ProcessPlatform::Windows,
            },
            windows_creation_time_100ns_since_unix_epoch: Some(17000000001234567),
        };
        let reused_pid = ExactProcessIdentity {
            windows_creation_time_100ns_since_unix_epoch: Some(17000000001234568),
            ..expected
        };

        assert_eq!(
            classify_exact_process_identity(expected, expected),
            ProcessClassification::Alive
        );
        assert_eq!(
            classify_exact_process_identity(expected, reused_pid),
            ProcessClassification::IdentityMismatch
        );
    }

    #[test]
    fn exact_owner_identity_ignores_process_group_changes() {
        let expected = ExactProcessIdentity {
            identity: ProcessIdentity {
                pid: 42,
                process_group: Some(42),
                start_time_micros: Some(100),
                platform: ProcessPlatform::Unix,
            },
            windows_creation_time_100ns_since_unix_epoch: None,
        };
        let moved = ExactProcessIdentity {
            identity: ProcessIdentity {
                process_group: Some(7),
                ..expected.identity
            },
            ..expected
        };
        let reused = ExactProcessIdentity {
            identity: ProcessIdentity {
                start_time_micros: Some(101),
                ..moved.identity
            },
            ..expected
        };
        let no_group = ExactProcessIdentity {
            identity: ProcessIdentity {
                process_group: None,
                ..expected.identity
            },
            ..expected
        };

        assert_eq!(
            classify_exact_process_identity(expected, moved),
            ProcessClassification::Alive
        );
        assert_eq!(
            classify_exact_process_identity(expected, reused),
            ProcessClassification::IdentityMismatch
        );
        assert_eq!(
            classify_exact_process_identity(expected, no_group),
            ProcessClassification::Unknown
        );
        assert_eq!(
            classify_exact_process_identity(no_group, moved),
            ProcessClassification::Unknown
        );
    }

    #[test]
    fn process_group_without_start_time_does_not_prove_exact_identity() {
        let identity = ExactProcessIdentity {
            identity: ProcessIdentity {
                pid: 42,
                process_group: Some(42),
                start_time_micros: None,
                platform: ProcessPlatform::Unix,
            },
            windows_creation_time_100ns_since_unix_epoch: None,
        };
        assert_eq!(
            classify_exact_process_identity(identity, identity),
            ProcessClassification::Unknown
        );
    }
}
