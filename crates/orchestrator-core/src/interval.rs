//! Pure check interval for running work (DPES-1105 phase 3b, decisions N10 and N11).
//!
//! More running work means a shorter interval between checks:
//! `interval(W) = max(minimum, base / 2^(W - 1))` for `W >= 1`, where `W` is the number of
//! work units in one scope. `W = 0` arms no check. Nothing here reads a clock or does I/O:
//! the caller passes the time of the previous check.

use std::time::Duration;

/// The base is a user setting between one minute and four hours.
pub const BASE_INTERVAL_RANGE: (Duration, Duration) =
    (Duration::from_secs(60), Duration::from_secs(240 * 60));
/// The least a minimum may be. A zero or tiny floor would make the check spin.
pub const MIN_INTERVAL_FLOOR: Duration = Duration::from_secs(10);
pub const DEFAULT_BASE_INTERVAL: Duration = Duration::from_secs(10 * 60);
pub const DEFAULT_MIN_INTERVAL: Duration = Duration::from_secs(60);

/// The largest shift of a u128 that is still defined. A larger work count saturates here.
const MAX_SHIFT: u32 = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalPolicyError {
    /// Base outside [`BASE_INTERVAL_RANGE`].
    BaseOutOfRange,
    /// Minimum under [`MIN_INTERVAL_FLOOR`].
    MinimumBelowFloor,
    /// Minimum above the base.
    MinimumAboveBase,
}

/// Adjusts a computed interval. Implementations define deterministic or random jitter. The
/// policy clamps the result into `[minimum, base]`, so jitter can never break the floor.
pub trait IntervalJitter {
    fn apply(&self, interval: Duration, work_units: u32) -> Duration;
}

/// Jitter implementation that leaves the interval unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoIntervalJitter;

impl IntervalJitter for NoIntervalJitter {
    fn apply(&self, interval: Duration, _work_units: u32) -> Duration {
        interval
    }
}

/// `interval(W) = max(minimum, base / 2^(W - 1))` for `W >= 1`. `W = 0` arms no timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalPolicy<J = NoIntervalJitter> {
    base: Duration,
    minimum: Duration,
    jitter: J,
}

impl IntervalPolicy<NoIntervalJitter> {
    /// Validates: base in [`BASE_INTERVAL_RANGE`], minimum at least [`MIN_INTERVAL_FLOOR`],
    /// minimum at most the base.
    pub fn new(base: Duration, minimum: Duration) -> Result<Self, IntervalPolicyError> {
        if base < BASE_INTERVAL_RANGE.0 || base > BASE_INTERVAL_RANGE.1 {
            return Err(IntervalPolicyError::BaseOutOfRange);
        }
        if minimum < MIN_INTERVAL_FLOOR {
            return Err(IntervalPolicyError::MinimumBelowFloor);
        }
        if minimum > base {
            return Err(IntervalPolicyError::MinimumAboveBase);
        }
        Ok(Self {
            base,
            minimum,
            jitter: NoIntervalJitter,
        })
    }
}

impl Default for IntervalPolicy<NoIntervalJitter> {
    fn default() -> Self {
        Self {
            base: DEFAULT_BASE_INTERVAL,
            minimum: DEFAULT_MIN_INTERVAL,
            jitter: NoIntervalJitter,
        }
    }
}

impl<J> IntervalPolicy<J> {
    /// Replaces the configured jitter with caller-supplied jitter.
    pub fn with_jitter<K: IntervalJitter>(self, jitter: K) -> IntervalPolicy<K> {
        IntervalPolicy {
            base: self.base,
            minimum: self.minimum,
            jitter,
        }
    }

    pub fn base(&self) -> Duration {
        self.base
    }

    pub fn minimum(&self) -> Duration {
        self.minimum
    }
}

impl<J: IntervalJitter> IntervalPolicy<J> {
    /// `None` for zero work units. Otherwise the base shifted right by `work_units - 1`,
    /// computed in u128 nanoseconds with the shift saturated at 127, so `u32::MAX` units
    /// returns the minimum with no overflow and no panic. The result is clamped to
    /// `[minimum, base]` after jitter.
    pub fn interval(&self, work_units: u32) -> Option<Duration> {
        let shift = work_units.checked_sub(1)?.min(MAX_SHIFT);
        let nanos = self.base.as_nanos() >> shift;
        let halved = Duration::new(
            u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX),
            // The remainder is under one billion, so it always fits.
            u32::try_from(nanos % 1_000_000_000).unwrap_or(0),
        );
        let scaled = halved.max(self.minimum);
        Some(
            self.jitter
                .apply(scaled, work_units)
                .max(self.minimum)
                .min(self.base),
        )
    }

    /// Next check time: the previous check plus `interval(work_units)`, saturating. `None` for
    /// zero units. The caller recomputes it whenever the work count changes, from the SAME
    /// previous check time, so a rise in work brings the next check earlier and a drop pushes
    /// it later.
    pub fn due_ms(&self, previous_check_ms: u64, work_units: u32) -> Option<u64> {
        let interval = self.interval(work_units)?;
        let interval_ms = u64::try_from(interval.as_millis()).unwrap_or(u64::MAX);
        Some(previous_check_ms.saturating_add(interval_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    fn secs(value: u64) -> Duration {
        Duration::from_secs(value)
    }

    /// Returns a fixed interval whatever the computed one is.
    struct Fixed(Duration);

    impl IntervalJitter for Fixed {
        fn apply(&self, _interval: Duration, _work_units: u32) -> Duration {
            self.0
        }
    }

    #[test]
    fn work_halves_the_interval_down_to_the_minimum() {
        let policy = IntervalPolicy::new(10 * MINUTE, MINUTE).unwrap();
        let intervals: Vec<_> = (1..=5)
            .map(|units| policy.interval(units).unwrap())
            .collect();
        assert_eq!(
            intervals,
            vec![secs(600), secs(300), secs(150), secs(75), secs(60)]
        );
    }

    #[test]
    fn more_work_stays_at_the_floor() {
        let policy = IntervalPolicy::default();
        for units in [6, 7, 64, 65, 1000] {
            assert_eq!(policy.interval(units), Some(secs(60)), "W = {units}");
        }
    }

    #[test]
    fn zero_work_arms_no_check() {
        let policy = IntervalPolicy::default();
        assert_eq!(policy.interval(0), None);
        assert_eq!(policy.due_ms(1_000, 0), None);
    }

    #[test]
    fn minimum_equal_to_base_turns_scaling_off() {
        let policy = IntervalPolicy::new(10 * MINUTE, 10 * MINUTE).unwrap();
        for units in 1..=1000 {
            assert_eq!(policy.interval(units), Some(secs(600)), "W = {units}");
        }
    }

    #[test]
    fn a_huge_work_count_saturates_at_the_floor() {
        let policy = IntervalPolicy::default();
        assert_eq!(policy.interval(u32::MAX), Some(MINUTE));
        assert_eq!(policy.interval(129), Some(MINUTE));
    }

    #[test]
    fn a_zero_or_sub_floor_minimum_is_refused() {
        for minimum in [secs(0), secs(9)] {
            assert_eq!(
                IntervalPolicy::new(10 * MINUTE, minimum),
                Err(IntervalPolicyError::MinimumBelowFloor)
            );
        }
        assert!(IntervalPolicy::new(10 * MINUTE, secs(10)).is_ok());
    }

    #[test]
    fn a_minimum_above_the_base_is_refused() {
        assert_eq!(
            IntervalPolicy::new(10 * MINUTE, 11 * MINUTE),
            Err(IntervalPolicyError::MinimumAboveBase)
        );
    }

    #[test]
    fn a_base_outside_one_to_240_minutes_is_refused() {
        for base in [secs(59), 241 * MINUTE] {
            assert_eq!(
                IntervalPolicy::new(base, secs(10)),
                Err(IntervalPolicyError::BaseOutOfRange)
            );
        }
        assert!(IntervalPolicy::new(secs(60), secs(10)).is_ok());
        assert!(IntervalPolicy::new(240 * MINUTE, secs(10)).is_ok());
    }

    #[test]
    fn the_default_policy_is_ten_minutes_down_to_one() {
        let policy = IntervalPolicy::default();
        assert_eq!(policy.base(), secs(600));
        assert_eq!(policy.minimum(), secs(60));
    }

    #[test]
    fn jitter_cannot_break_the_floor_or_the_base() {
        let low = IntervalPolicy::default().with_jitter(Fixed(secs(1)));
        let high = IntervalPolicy::default().with_jitter(Fixed(24 * 60 * MINUTE));
        for units in [1, 3, 100] {
            assert_eq!(low.interval(units), Some(MINUTE));
            assert_eq!(high.interval(units), Some(10 * MINUTE));
        }
    }

    #[test]
    fn due_time_is_previous_check_plus_the_current_interval() {
        let policy = IntervalPolicy::default();
        assert_eq!(policy.due_ms(1_000_000, 1), Some(1_600_000));
        assert_eq!(policy.due_ms(1_000_000, 4), Some(1_075_000));
        // The same previous check with more work is earlier, with less work later.
        assert!(policy.due_ms(1_000_000, 4) < policy.due_ms(1_000_000, 2));
        assert!(policy.due_ms(1_000_000, 2) < policy.due_ms(1_000_000, 1));
    }

    #[test]
    fn due_time_saturates_at_the_largest_time() {
        let policy = IntervalPolicy::default();
        assert_eq!(policy.due_ms(u64::MAX, 1), Some(u64::MAX));
    }
}
