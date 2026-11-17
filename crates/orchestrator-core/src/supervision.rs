//! Pure supervision rules for the wake-up loop (DPES-1105 phase 3b).
//!
//! The stall rule, the wake limits, the per-scope debounce, the monitoring mode and the
//! injectable clock. Nothing here reads a clock or does I/O: every call takes the time it
//! happens at. [`SystemClock`] is not here; the daemon, which owns the one wall clock,
//! implements [`Clock`] for it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub const DEFAULT_ATTEMPT_WALL_MINUTES: u32 = 120;
/// A task is flagged as quiet only after `max(effective interval, STALL_QUIET_FLOOR)`.
pub const STALL_QUIET_FLOOR: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_WAKE_HOURLY_CAP: u32 = 6;
pub const DEFAULT_WAKE_DAILY_CAP: u32 = 40;
pub const DEBOUNCE_MS: u64 = 60_000;
pub const MAX_BATCH: usize = 20;
pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 86_400_000;

/// Whether the daemon watches, and whether a model may speak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitoringMode {
    On,
    Quiet,
    Off,
    /// Phase 5. `monitoring.set` refuses it until then.
    Away,
}

impl MonitoringMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Quiet => "quiet",
            Self::Off => "off",
            Self::Away => "away",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "on" => Some(Self::On),
            "quiet" => Some(Self::Quiet),
            "off" => Some(Self::Off),
            "away" => Some(Self::Away),
            _ => None,
        }
    }

    /// Whether a model-needing event may start a wake turn.
    pub const fn runs_wake_turns(self) -> bool {
        matches!(self, Self::On | Self::Quiet | Self::Away)
    }

    /// Whether a wake reply raises the badge and the announcement.
    pub const fn notifies(self) -> bool {
        matches!(self, Self::On)
    }
}

/// Caps on wake turns per scope. `None` means no cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakeLimits {
    pub hourly: Option<u32>,
    pub daily: Option<u32>,
    pub daily_tokens: Option<u64>,
}

impl Default for WakeLimits {
    fn default() -> Self {
        Self {
            hourly: Some(DEFAULT_WAKE_HOURLY_CAP),
            daily: Some(DEFAULT_WAKE_DAILY_CAP),
            daily_tokens: None,
        }
    }
}

/// What one scope's wake turns used in the rolling windows, read from the ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WakeUsage {
    pub hour_count: u32,
    pub day_count: u32,
    pub oldest_hour_ms: Option<u64>,
    pub oldest_day_ms: Option<u64>,
    pub day_tokens: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    Hourly,
    Daily,
    Tokens,
}

impl PauseReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Tokens => "tokens",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Go,
    Paused {
        reason: PauseReason,
        until_ms: Option<u64>,
    },
}

/// Checks hourly, then daily, then tokens; the first limit reached pauses. `until_ms` is when
/// the oldest counted turn leaves the window; `None` for tokens.
pub fn admit(limits: &WakeLimits, usage: &WakeUsage) -> Admission {
    if matches!(limits.hourly, Some(cap) if usage.hour_count >= cap) {
        return Admission::Paused {
            reason: PauseReason::Hourly,
            until_ms: usage
                .oldest_hour_ms
                .map(|oldest| oldest.saturating_add(HOUR_MS)),
        };
    }
    if matches!(limits.daily, Some(cap) if usage.day_count >= cap) {
        return Admission::Paused {
            reason: PauseReason::Daily,
            until_ms: usage
                .oldest_day_ms
                .map(|oldest| oldest.saturating_add(DAY_MS)),
        };
    }
    if matches!(limits.daily_tokens, Some(budget) if usage.day_tokens >= budget) {
        return Admission::Paused {
            reason: PauseReason::Tokens,
            until_ms: None,
        };
    }
    Admission::Go
}

/// One running attempt, as the stall rule sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    pub task_id: String,
    pub attempt_id: String,
    pub session_id: String,
    pub started_ms: u64,
    pub last_event_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stall {
    OverWallTime { running_ms: u64 },
    NoEvents { quiet_ms: u64 },
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Wall time wins when both hold. The caller passes
/// `quiet_for = max(effective interval, STALL_QUIET_FLOOR)`. Silence is measured from the
/// later of the last event and the start, so an attempt never counts as quiet before it began.
pub fn stalled(
    activity: &Activity,
    now_ms: u64,
    quiet_for: Duration,
    wall: Duration,
) -> Option<Stall> {
    let running_ms = now_ms.saturating_sub(activity.started_ms);
    if running_ms >= millis(wall) {
        return Some(Stall::OverWallTime { running_ms });
    }
    let quiet_ms = now_ms.saturating_sub(activity.last_event_ms.max(activity.started_ms));
    (quiet_ms >= millis(quiet_for)).then_some(Stall::NoEvents { quiet_ms })
}

#[derive(Debug)]
struct Window {
    due_ms: u64,
    ids: Vec<String>,
}

/// Per-scope debounce: a window opens at the first offered id and is due `DEBOUNCE_MS` later
/// (fixed, not sliding, so a steady stream cannot starve a wake).
#[derive(Debug, Default)]
pub struct Debounce {
    windows: BTreeMap<String, Window>,
}

impl Debounce {
    /// Adds a record id to the scope's window, opening it when none is open. A duplicate id is
    /// ignored.
    pub fn offer(&mut self, scope: &str, record_id: &str, now_ms: u64) {
        let window = self
            .windows
            .entry(scope.to_owned())
            .or_insert_with(|| Window {
                due_ms: now_ms.saturating_add(DEBOUNCE_MS),
                ids: Vec::new(),
            });
        if !window.ids.iter().any(|id| id == record_id) {
            window.ids.push(record_id.to_owned());
        }
    }

    /// Scopes whose window is due, sorted.
    pub fn due(&self, now_ms: u64) -> Vec<String> {
        self.windows
            .iter()
            .filter(|(_, window)| window.due_ms <= now_ms && !window.ids.is_empty())
            .map(|(scope, _)| scope.clone())
            .collect()
    }

    /// Up to `MAX_BATCH` ids of the scope, oldest first. The rest stay in a window that is
    /// still due.
    pub fn take(&mut self, scope: &str) -> Vec<String> {
        let Some(window) = self.windows.get_mut(scope) else {
            return Vec::new();
        };
        let count = window.ids.len().min(MAX_BATCH);
        let batch: Vec<String> = window.ids.drain(..count).collect();
        if window.ids.is_empty() {
            self.windows.remove(scope);
        }
        batch
    }

    /// Drops the scope's window (mode off).
    pub fn forget(&mut self, scope: &str) {
        self.windows.remove(scope);
    }
}

/// Source of Unix milliseconds. The daemon passes its wall clock; tests pass [`ManualClock`].
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// A clock a test moves by hand.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicU64);

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self(AtomicU64::new(now_ms))
    }

    pub fn set(&self, now_ms: u64) {
        self.0.store(now_ms, Ordering::SeqCst);
    }

    pub fn advance(&self, by_ms: u64) {
        let _ = self
            .0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |now| {
                Some(now.saturating_add(by_ms))
            });
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Stable id of a wake record: the same scope and dedupe key always give the same id, so a
/// replay of an event is a no-op.
pub fn wake_record_id(scope: &str, dedupe_key: &str) -> String {
    let digest = Sha256::digest(format!("{scope}\n{dedupe_key}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("wake-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(started_ms: u64, last_event_ms: u64) -> Activity {
        Activity {
            task_id: "task-1".into(),
            attempt_id: "attempt-1".into(),
            session_id: "session-1".into(),
            started_ms,
            last_event_ms,
        }
    }

    fn minutes(value: u64) -> Duration {
        Duration::from_secs(value * 60)
    }

    #[test]
    fn stall_by_wall_time_wins_over_no_events() {
        // Running 130 minutes, silent for 130 minutes: both rules hold.
        let found = stalled(&activity(0, 0), 130 * 60_000, minutes(10), minutes(120));
        assert_eq!(
            found,
            Some(Stall::OverWallTime {
                running_ms: 130 * 60_000
            })
        );
    }

    #[test]
    fn stall_needs_a_full_quiet_period() {
        let quiet_for = minutes(10);
        let wall = minutes(120);
        let last = 1_000_000;
        assert_eq!(
            stalled(&activity(0, last), last + 599_000, quiet_for, wall),
            None
        );
        assert_eq!(
            stalled(&activity(0, last), last + 600_000, quiet_for, wall),
            Some(Stall::NoEvents { quiet_ms: 600_000 })
        );
        // Silence counts from the start when the last event is older than the start.
        assert_eq!(
            stalled(
                &activity(5_000_000, 0),
                5_000_000 + 599_000,
                quiet_for,
                wall
            ),
            None
        );
    }

    #[test]
    fn the_quiet_floor_is_five_minutes() {
        assert_eq!(STALL_QUIET_FLOOR, Duration::from_secs(300));
        assert_eq!(
            Duration::from_secs(60).max(STALL_QUIET_FLOOR),
            STALL_QUIET_FLOOR
        );
        assert_eq!(minutes(10).max(STALL_QUIET_FLOOR), minutes(10));
    }

    #[test]
    fn admit_pauses_at_the_hourly_then_daily_then_token_limit() {
        let limits = WakeLimits {
            hourly: Some(2),
            daily: Some(4),
            daily_tokens: Some(100),
        };
        let mut usage = WakeUsage {
            hour_count: 1,
            day_count: 1,
            oldest_hour_ms: Some(1_000),
            oldest_day_ms: Some(500),
            day_tokens: 0,
        };
        assert_eq!(admit(&limits, &usage), Admission::Go);

        usage.day_tokens = 100;
        assert_eq!(
            admit(&limits, &usage),
            Admission::Paused {
                reason: PauseReason::Tokens,
                until_ms: None
            }
        );

        usage.day_count = 4;
        assert_eq!(
            admit(&limits, &usage),
            Admission::Paused {
                reason: PauseReason::Daily,
                until_ms: Some(500 + DAY_MS)
            }
        );

        usage.hour_count = 2;
        assert_eq!(
            admit(&limits, &usage),
            Admission::Paused {
                reason: PauseReason::Hourly,
                until_ms: Some(1_000 + HOUR_MS)
            }
        );
        assert_eq!(PauseReason::Hourly.as_str(), "hourly");
        assert_eq!(PauseReason::Daily.as_str(), "daily");
        assert_eq!(PauseReason::Tokens.as_str(), "tokens");
    }

    #[test]
    fn admit_with_no_limits_always_goes() {
        let limits = WakeLimits {
            hourly: None,
            daily: None,
            daily_tokens: None,
        };
        let usage = WakeUsage {
            hour_count: u32::MAX,
            day_count: u32::MAX,
            oldest_hour_ms: Some(0),
            oldest_day_ms: Some(0),
            day_tokens: u64::MAX,
        };
        assert_eq!(admit(&limits, &usage), Admission::Go);
    }

    #[test]
    fn default_limits_are_six_per_hour_and_forty_per_day() {
        let limits = WakeLimits::default();
        assert_eq!(limits.hourly, Some(6));
        assert_eq!(limits.daily, Some(40));
        assert_eq!(limits.daily_tokens, None);
    }

    #[test]
    fn debounce_batches_events_for_sixty_seconds() {
        let mut debounce = Debounce::default();
        debounce.offer("scope", "a", 0);
        debounce.offer("scope", "b", 20_000);
        debounce.offer("scope", "c", 59_000);
        debounce.offer("scope", "c", 59_500);
        assert!(debounce.due(59_999).is_empty());
        assert_eq!(debounce.due(60_000), vec!["scope".to_owned()]);
        assert_eq!(debounce.take("scope"), vec!["a", "b", "c"]);
        assert!(debounce.due(60_000).is_empty());
    }

    #[test]
    fn debounce_window_is_fixed_not_sliding() {
        let mut debounce = Debounce::default();
        for step in 0..=5 {
            debounce.offer("scope", &format!("id-{step}"), step * 10_000);
        }
        // A steady stream every ten seconds did not push the due time past 60 s.
        assert_eq!(debounce.due(60_000), vec!["scope".to_owned()]);
    }

    #[test]
    fn debounce_batches_are_capped_and_the_rest_stay_due() {
        let mut debounce = Debounce::default();
        for index in 0..25 {
            debounce.offer("scope", &format!("id-{index:02}"), 0);
        }
        let first = debounce.take("scope");
        assert_eq!(first.len(), MAX_BATCH);
        assert_eq!(first[0], "id-00");
        assert_eq!(debounce.due(60_000), vec!["scope".to_owned()]);
        assert_eq!(debounce.take("scope").len(), 5);
        assert!(debounce.due(u64::MAX).is_empty());
    }

    #[test]
    fn debounce_keeps_scopes_apart() {
        let mut debounce = Debounce::default();
        debounce.offer("b-scope", "x", 0);
        debounce.offer("a-scope", "y", 30_000);
        assert_eq!(debounce.due(60_000), vec!["b-scope".to_owned()]);
        assert_eq!(
            debounce.due(90_000),
            vec!["a-scope".to_owned(), "b-scope".to_owned()]
        );
        assert_eq!(debounce.take("a-scope"), vec!["y"]);
        debounce.forget("b-scope");
        assert!(debounce.due(u64::MAX).is_empty());
        assert!(debounce.take("b-scope").is_empty());
    }

    #[test]
    fn mode_parse_and_flags() {
        for (name, mode, runs, notifies) in [
            ("on", MonitoringMode::On, true, true),
            ("quiet", MonitoringMode::Quiet, true, false),
            ("off", MonitoringMode::Off, false, false),
            ("away", MonitoringMode::Away, true, false),
        ] {
            assert_eq!(MonitoringMode::parse(name), Some(mode));
            assert_eq!(mode.as_str(), name);
            assert_eq!(mode.runs_wake_turns(), runs, "{name}");
            assert_eq!(mode.notifies(), notifies, "{name}");
        }
        assert_eq!(MonitoringMode::parse("bogus"), None);
    }

    #[test]
    fn the_manual_clock_moves_only_when_told() {
        let clock = ManualClock::new(10);
        assert_eq!(clock.now_ms(), 10);
        clock.advance(5);
        assert_eq!(clock.now_ms(), 15);
        clock.set(1);
        assert_eq!(clock.now_ms(), 1);
    }

    #[test]
    fn a_wake_record_id_is_stable_and_depends_on_both_parts() {
        let id = wake_record_id("conversation:global", "failed:t1:7");
        assert_eq!(id, wake_record_id("conversation:global", "failed:t1:7"));
        assert_ne!(id, wake_record_id("conversation:global", "failed:t1:8"));
        assert_ne!(
            id,
            wake_record_id("conversation:workstream:a", "failed:t1:7")
        );
        assert!(id.starts_with("wake-") && id.len() == 5 + 64);
    }
}
