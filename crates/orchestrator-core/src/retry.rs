//! Pure bounded retry decisions.
//!
//! Attempt counts include the failed attempt already persisted.
//!
//! Updated: 2026-11-08T09:09:22Z.

use std::num::NonZeroU32;
use std::time::Duration;

/// Failure category supplied by the executor. Only `Transient` is retryable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Failure may succeed if the task runs again.
    Transient,
    /// Agent output violated its protocol.
    ProtocolViolation,
    /// The task contract was rejected.
    ContractRejected,
    /// The process exited with a non-retryable status.
    NonRetryableExit,
}

/// Adjusts computed backoff. Implementations define deterministic or random jitter.
pub trait RetryJitter {
    /// Returns adjusted delay for one-based retry number. Policy caps result at max backoff.
    fn apply(&self, backoff: Duration, retry_number: u32) -> Duration;
}

/// Jitter implementation that leaves backoff unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoJitter;

impl RetryJitter for NoJitter {
    fn apply(&self, backoff: Duration, _retry_number: u32) -> Duration {
        backoff
    }
}

/// Exponential retry schedule with positive bounded retries and a maximum delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy<J = NoJitter> {
    max_retries: NonZeroU32,
    initial_backoff: Duration,
    max_backoff: Duration,
    jitter: J,
}

impl RetryPolicy<NoJitter> {
    /// Creates a policy with a positive, bounded retry count and valid backoff limits.
    /// `max_retries` counts retries after the first attempt. `None` is unbounded
    /// and rejected. Zero initial backoff and a cap below the initial delay are rejected.
    pub fn new(
        max_retries: Option<u32>,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Result<Self, RetryPolicyError> {
        let max_retries = max_retries.ok_or(RetryPolicyError::UnboundedMaxRetries)?;
        let max_retries = NonZeroU32::new(max_retries).ok_or(RetryPolicyError::ZeroMaxRetries)?;
        if initial_backoff.is_zero() {
            return Err(RetryPolicyError::ZeroInitialBackoff);
        }
        if max_backoff < initial_backoff {
            return Err(RetryPolicyError::MaxBackoffBelowInitialBackoff);
        }
        Ok(Self {
            max_retries,
            initial_backoff,
            max_backoff,
            jitter: NoJitter,
        })
    }
}

impl<J> RetryPolicy<J> {
    /// Replaces configured jitter with caller-supplied jitter without allocating.
    pub fn with_jitter<K: RetryJitter>(self, jitter: K) -> RetryPolicy<K> {
        RetryPolicy {
            max_retries: self.max_retries,
            initial_backoff: self.initial_backoff,
            max_backoff: self.max_backoff,
            jitter,
        }
    }
}

impl<J: RetryJitter> RetryPolicy<J> {
    /// Chooses a new attempt from recorded attempts, including the current failure.
    /// Returns attempt `attempts_recorded + 1` only for transient failures within the retry bound.
    pub fn next_attempt(&self, failure: FailureClass, attempts_recorded: u32) -> RetryDecision {
        if failure != FailureClass::Transient {
            return RetryDecision::Stop {
                reason: RetryStopReason::PermanentFailure(failure),
            };
        }
        if attempts_recorded == 0 {
            return RetryDecision::Stop {
                reason: RetryStopReason::NoAttemptsRecorded,
            };
        }
        if attempts_recorded > self.max_retries.get() {
            return RetryDecision::Stop {
                reason: RetryStopReason::RetryLimitReached {
                    max_retries: self.max_retries.get(),
                },
            };
        }
        let Some(attempt_number) = attempts_recorded.checked_add(1) else {
            return RetryDecision::Stop {
                reason: RetryStopReason::AttemptNumberOverflow,
            };
        };
        let retry_number = attempts_recorded;
        let backoff = self.backoff_for(retry_number);
        RetryDecision::Retry {
            after: self
                .jitter
                .apply(backoff, retry_number)
                .min(self.max_backoff),
            attempt_number,
        }
    }

    fn backoff_for(&self, retry_number: u32) -> Duration {
        let mut backoff = self.initial_backoff;
        for _ in 1..retry_number {
            if backoff >= self.max_backoff {
                break;
            }
            backoff = backoff
                .checked_mul(2)
                .unwrap_or(self.max_backoff)
                .min(self.max_backoff);
        }
        backoff
    }
}

/// Defaults to three retries, one-second initial backoff, a 30-second cap, and no jitter.
impl Default for RetryPolicy<NoJitter> {
    fn default() -> Self {
        Self {
            max_retries: NonZeroU32::new(3).expect("three is nonzero"),
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            jitter: NoJitter,
        }
    }
}

/// Result of applying retry policy to the latest failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Create a new attempt after this delay.
    Retry {
        after: Duration,
        attempt_number: u32,
    },
    /// Do not create another attempt.
    Stop { reason: RetryStopReason },
}

/// Why a retry was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryStopReason {
    /// The failure class is permanent.
    PermanentFailure(FailureClass),
    /// No durable attempt exists for the reported failure.
    NoAttemptsRecorded,
    /// The configured retry count has been consumed.
    RetryLimitReached { max_retries: u32 },
    /// The next attempt number cannot fit in `u32`.
    AttemptNumberOverflow,
}

/// Invalid retry policy configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPolicyError {
    /// Retry count was unbounded.
    UnboundedMaxRetries,
    /// Retry count was zero.
    ZeroMaxRetries,
    /// Initial backoff was zero.
    ZeroInitialBackoff,
    /// Maximum backoff was less than initial backoff.
    MaxBackoffBelowInitialBackoff,
}

impl std::fmt::Display for RetryPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnboundedMaxRetries => f.write_str("maximum retries must be bounded"),
            Self::ZeroMaxRetries => f.write_str("maximum retries must be nonzero"),
            Self::ZeroInitialBackoff => f.write_str("initial backoff must be nonzero"),
            Self::MaxBackoffBelowInitialBackoff => {
                f.write_str("maximum backoff must be at least initial backoff")
            }
        }
    }
}

impl std::error::Error for RetryPolicyError {}
