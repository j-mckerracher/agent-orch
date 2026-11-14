use std::time::Duration;

use orchestrator_core::retry::{
    FailureClass, RetryDecision, RetryJitter, RetryPolicy, RetryPolicyError, RetryStopReason,
};

fn policy(max_retries: Option<u32>) -> RetryPolicy {
    RetryPolicy::new(max_retries, Duration::from_secs(1), Duration::from_secs(4)).unwrap()
}

#[test]
fn rejects_zero_and_unbounded_retry_counts() {
    assert_eq!(
        RetryPolicy::new(Some(0), Duration::from_secs(1), Duration::from_secs(4)),
        Err(RetryPolicyError::ZeroMaxRetries)
    );
    assert_eq!(
        RetryPolicy::new(None, Duration::from_secs(1), Duration::from_secs(4)),
        Err(RetryPolicyError::UnboundedMaxRetries)
    );
}

#[test]
fn rejects_invalid_backoff_bounds() {
    assert_eq!(
        RetryPolicy::new(Some(1), Duration::ZERO, Duration::from_secs(1)),
        Err(RetryPolicyError::ZeroInitialBackoff)
    );
    assert_eq!(
        RetryPolicy::new(Some(1), Duration::from_secs(2), Duration::from_secs(1)),
        Err(RetryPolicyError::MaxBackoffBelowInitialBackoff)
    );
}

#[test]
fn schedule_doubles_caps_and_is_deterministic() {
    let policy = policy(Some(5));
    let expected = [1, 2, 4, 4, 4];
    for (index, seconds) in expected.into_iter().enumerate() {
        let attempts_recorded = u32::try_from(index).unwrap() + 1;
        assert_eq!(
            policy.next_attempt(FailureClass::Transient, attempts_recorded),
            RetryDecision::Retry {
                after: Duration::from_secs(seconds),
                attempt_number: attempts_recorded + 1,
            }
        );
    }
}

#[test]
fn default_policy_uses_three_retries() {
    let policy = RetryPolicy::default();
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 1),
        RetryDecision::Retry {
            after: Duration::from_secs(1),
            attempt_number: 2,
        }
    );
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 3),
        RetryDecision::Retry {
            after: Duration::from_secs(4),
            attempt_number: 4,
        }
    );
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 4),
        RetryDecision::Stop {
            reason: RetryStopReason::RetryLimitReached { max_retries: 3 },
        }
    );
}

#[test]
fn permanent_failure_classes_never_retry() {
    let policy = policy(Some(3));
    for failure in [
        FailureClass::ProtocolViolation,
        FailureClass::ContractRejected,
        FailureClass::NonRetryableExit,
    ] {
        assert_eq!(
            policy.next_attempt(failure, 1),
            RetryDecision::Stop {
                reason: RetryStopReason::PermanentFailure(failure),
            }
        );
    }
}

#[test]
fn transient_failure_retries_until_limit_then_stops() {
    let policy = policy(Some(2));
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 1),
        RetryDecision::Retry {
            after: Duration::from_secs(1),
            attempt_number: 2,
        }
    );
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 2),
        RetryDecision::Retry {
            after: Duration::from_secs(2),
            attempt_number: 3,
        }
    );
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 3),
        RetryDecision::Stop {
            reason: RetryStopReason::RetryLimitReached { max_retries: 2 },
        }
    );
}

struct AddQuarterSecond;

impl RetryJitter for AddQuarterSecond {
    fn apply(&self, backoff: Duration, _retry_number: u32) -> Duration {
        backoff + Duration::from_millis(250)
    }
}

#[test]
fn injected_jitter_changes_delay_and_remains_capped() {
    let policy = policy(Some(4)).with_jitter(AddQuarterSecond);
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 1),
        RetryDecision::Retry {
            after: Duration::from_millis(1_250),
            attempt_number: 2,
        }
    );
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, 4),
        RetryDecision::Retry {
            after: Duration::from_secs(4),
            attempt_number: 5,
        }
    );
}

#[test]
fn missing_history_and_attempt_number_overflow_stop() {
    assert_eq!(
        policy(Some(3)).next_attempt(FailureClass::Transient, 0),
        RetryDecision::Stop {
            reason: RetryStopReason::NoAttemptsRecorded,
        }
    );
    let policy = RetryPolicy::new(
        Some(u32::MAX),
        Duration::from_secs(1),
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(
        policy.next_attempt(FailureClass::Transient, u32::MAX),
        RetryDecision::Stop {
            reason: RetryStopReason::AttemptNumberOverflow,
        }
    );
}
