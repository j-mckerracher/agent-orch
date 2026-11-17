//! Pure machine load gate (decision N12): whether new work may start.
//!
//! The gate keeps a rolling window of host readings per meter (CPU, memory,
//! disk) and compares each meter's mean with a limit. It closes when any gated
//! mean reaches its limit and reopens only when every gated mean is at least
//! [`REOPEN_MARGIN`] points under its limit. Nothing here reads a clock, a
//! socket or a file: every call takes the time it happens at.
//!
//! Rules a caller can rely on:
//!
//! * A meter with no reading (`None` in a [`LoadSample`], or a reading that is
//!   not a finite number) is left out and reported as unknown. It never closes
//!   the gate, and when no meter can be judged the gate is open.
//! * A meter with fewer than [`MIN_SAMPLES`] readings in its window is warming
//!   up and is left out too. From [`MIN_SAMPLES`] readings on the gate uses
//!   the mean of the readings so far, so a restart does not wait a full window.
//! * A limit of [`OFF`] turns that meter's check off.
//! * Readings are rounded to a hundredth of a percent and held as integers, and
//!   a mean is compared with a limit as an exact product. No float comparison
//!   decides whether work may start.
//! * A close or an open is a [`Change`]. [`LoadGate::status`] never takes one,
//!   so any number of pollers can read the gate without hiding a change from
//!   the caller that wakes on it. Only [`LoadGate::record`] and
//!   [`LoadGate::take_change`] take a change, and each one is taken once.
//!
//! Updated: 2026-11-17T05:47:09Z.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How far back a mean looks: 30 readings at the default 2 second sample interval.
pub const WINDOW: Duration = Duration::from_secs(60);
/// A closed gate reopens only when every gated mean is at most this many
/// percentage points under its limit.
pub const REOPEN_MARGIN: u8 = 10;
/// Fewest readings of one meter that may close the gate (10 seconds at the
/// default interval).
pub const MIN_SAMPLES: usize = 5;
/// Lowest limit a user may set.
pub const MIN_LIMIT: u8 = 50;
/// A limit of 100 turns that meter's check off.
pub const OFF: u8 = 100;

/// Memory bound on one meter's window: one reading per 100 ms over [`WINDOW`].
/// The default feed puts 30 readings in it.
const MAX_READINGS: usize = 600;
/// Readings are held in hundredths of a percent.
const SCALE: u64 = 100;

/// A host meter the gate checks. GPU is never gated, because agents do not use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadMeter {
    Cpu,
    Memory,
    Disk,
}

impl LoadMeter {
    /// Gate order. It also decides which meter a closed gate names first.
    pub const ALL: [Self; 3] = [Self::Cpu, Self::Memory, Self::Disk];

    /// Stable wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
        }
    }

    /// Name for user-facing text.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "memory",
            Self::Disk => "disk",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Cpu => 0,
            Self::Memory => 1,
            Self::Disk => 2,
        }
    }
}

/// The three limits, in percent. Each is from [`MIN_LIMIT`] to [`OFF`].
/// Defaults are CPU 80, memory 85, disk 90.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadLimits {
    pub cpu: u8,
    pub memory: u8,
    pub disk: u8,
}

impl Default for LoadLimits {
    fn default() -> Self {
        Self {
            cpu: 80,
            memory: 85,
            disk: 90,
        }
    }
}

impl LoadLimits {
    /// Builds limits, refusing the first meter whose limit is outside
    /// [`MIN_LIMIT`] to [`OFF`].
    pub fn new(cpu: u8, memory: u8, disk: u8) -> Result<Self, LimitError> {
        let limits = Self { cpu, memory, disk };
        match LoadMeter::ALL
            .into_iter()
            .find(|&meter| !(MIN_LIMIT..=OFF).contains(&limits.of(meter)))
        {
            Some(meter) => Err(LimitError(meter)),
            None => Ok(limits),
        }
    }

    /// The limit for one meter.
    pub fn of(&self, meter: LoadMeter) -> u8 {
        match meter {
            LoadMeter::Cpu => self.cpu,
            LoadMeter::Memory => self.memory,
            LoadMeter::Disk => self.disk,
        }
    }

    /// The limit the gate applies. The fields are public, so a value that did
    /// not come through [`LoadLimits::new`] is clamped into range here instead
    /// of underflowing or turning a check into nonsense.
    fn bounded(&self, meter: LoadMeter) -> u8 {
        self.of(meter).clamp(MIN_LIMIT, OFF)
    }
}

/// The first meter whose limit is outside [`MIN_LIMIT`] to [`OFF`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError(pub LoadMeter);

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} limit must be from {MIN_LIMIT} to {OFF} percent",
            self.0.label()
        )
    }
}

impl std::error::Error for LimitError {}

/// One host reading: each meter as a percentage from 0 to 100. `None` means
/// the meter was not `ok` (unavailable or unsupported). A value that is not a
/// finite number counts as `None`; a finite value outside 0 to 100 is clamped
/// into the range, as the host sampler does.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoadSample {
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
    pub disk: Option<f64>,
}

impl LoadSample {
    fn of(&self, meter: LoadMeter) -> Option<f64> {
        match meter {
            LoadMeter::Cpu => self.cpu,
            LoadMeter::Memory => self.memory,
            LoadMeter::Disk => self.disk,
        }
    }
}

/// Why the gate cannot judge a meter. Either way the meter is left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    /// Fewer than [`MIN_SAMPLES`] readings so far, or the gate has not been fed yet.
    WarmingUp,
    /// The gate has been fed but the meter has no reading in its window.
    Unavailable,
}

/// What the gate knows about one meter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    /// The limit is [`OFF`]; the meter is not checked.
    Off,
    Unknown(Unknown),
    /// Mean of the window, in percent.
    Mean(f64),
}

impl Reading {
    /// The mean in whole percents, rounded down as [`LoadHold::mean_percent`]
    /// is. A mean is a whole number of hundredths divided by at most 600, so
    /// storing it as a double cannot carry it over a whole percent.
    pub fn mean_percent(&self) -> Option<u8> {
        match self {
            Self::Mean(mean) => Some(mean.floor() as u8),
            Self::Off | Self::Unknown(_) => None,
        }
    }
}

/// Why the gate is closed: the meter that holds it, in whole percents (the
/// mean rounded down, so a meter at or over its limit never shows a mean under
/// it). A meter held only by the reopen band can show a mean equal to
/// [`LoadHold::reopen_percent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadHold {
    pub meter: LoadMeter,
    pub mean_percent: u8,
    pub limit_percent: u8,
}

impl LoadHold {
    /// The mean this meter must fall to (or under) before the gate may open.
    pub fn reopen_percent(&self) -> u8 {
        self.limit_percent.saturating_sub(REOPEN_MARGIN)
    }
}

/// The gate at one moment. `hold` is `Some` exactly when the gate is closed.
/// `meters` lists every meter in gate order.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadStatus {
    pub hold: Option<LoadHold>,
    pub meters: [(LoadMeter, Reading); 3],
}

/// The gate's last move since the previous take. Each close and each open is
/// reported once, to the first [`LoadGate::record`] or [`LoadGate::take_change`]
/// that follows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    Closed,
    Opened,
}

/// A window's readings summed, for exact comparison. `count` is at least one.
#[derive(Clone, Copy)]
struct Mean {
    /// Sum of the readings, in hundredths of a percent.
    sum: u64,
    count: u64,
}

impl Mean {
    fn of(readings: &VecDeque<(Instant, u16)>) -> Option<Self> {
        let count = readings.len() as u64;
        (count > 0).then(|| Self {
            sum: readings
                .iter()
                .map(|&(_, reading)| u64::from(reading))
                .sum(),
            count,
        })
    }

    /// The mean is at least `percent`: `sum / count >= percent * 100`, compared
    /// as a product so nothing rounds. At most 600 readings of at most 10 000
    /// each, so no product comes near `u64::MAX`.
    fn reaches(self, percent: u8) -> bool {
        self.sum >= u64::from(percent) * SCALE * self.count
    }

    /// The mean is at most `percent`.
    fn at_most(self, percent: u8) -> bool {
        self.sum <= u64::from(percent) * SCALE * self.count
    }

    /// The mean in whole percents, rounded down.
    fn percent(self) -> u8 {
        u8::try_from(self.sum / self.count / SCALE).unwrap_or(OFF)
    }

    /// The mean in percent, for display only.
    fn exact(self) -> f64 {
        self.sum as f64 / (SCALE * self.count) as f64
    }
}

/// A reading in hundredths of a percent, or `None` when it is not a finite number.
fn hundredths(percent: f64) -> Option<u16> {
    // Infinity is no reading, not 100: a broken probe leaves the meter unknown (fails open).
    percent
        .is_finite()
        .then(|| (percent.clamp(0.0, 100.0) * 100.0).round() as u16)
}

/// The first meter, in gate order, whose mean passes `test`.
fn first_meter(
    means: &[Option<Mean>; 3],
    test: impl Fn(Mean, LoadMeter) -> bool,
) -> Option<LoadMeter> {
    LoadMeter::ALL
        .into_iter()
        .find(|&meter| means[meter.index()].is_some_and(|mean| test(mean, meter)))
}

/// The rolling window, the limits and whether the gate is closed. Feed it with
/// [`LoadGate::record`]; read it with [`LoadGate::status`].
#[derive(Debug)]
pub struct LoadGate {
    limits: LoadLimits,
    /// Readings per meter, oldest first, in hundredths of a percent.
    windows: [VecDeque<(Instant, u16)>; 3],
    closed: bool,
    /// The newest close or open that no `record` or `take_change` has taken. A
    /// newer one replaces it: a waiter needs the gate's last move, and a close
    /// then an open leaves it open.
    untaken: Change,
    /// Whether any sample has reached the gate.
    any_sample: bool,
    /// The newest time the gate has seen.
    latest: Option<Instant>,
}

impl LoadGate {
    /// An open gate with empty windows.
    pub fn new(limits: LoadLimits) -> Self {
        Self {
            limits,
            windows: Default::default(),
            closed: false,
            untaken: Change::None,
            any_sample: false,
            latest: None,
        }
    }

    /// Takes effect at the next `record`, `take_change` or `status`, over the
    /// readings already in the windows.
    pub fn set_limits(&mut self, limits: LoadLimits) {
        self.limits = limits;
    }

    /// Adds one sample taken at `now`, drops readings 60 seconds old, evaluates,
    /// and takes the change (see [`LoadGate::take_change`]).
    pub fn record(&mut self, sample: LoadSample, now: Instant) -> Change {
        let now = self.advance(now);
        self.any_sample = true;
        for meter in LoadMeter::ALL {
            let Some(reading) = sample.of(meter).and_then(hundredths) else {
                continue;
            };
            let window = &mut self.windows[meter.index()];
            if window.len() >= MAX_READINGS {
                window.pop_front();
            }
            window.push_back((now, reading));
        }
        self.take_change(now)
    }

    /// Evaluates at `now` without a new sample and takes the close or open that
    /// no call has taken yet, if any. The caller that wakes on a change uses this
    /// when no sample arrives: a sampler that stops empties every window within
    /// 60 seconds, and a closed gate then fails open.
    pub fn take_change(&mut self, now: Instant) -> Change {
        let now = self.advance(now);
        self.evaluate(now);
        std::mem::replace(&mut self.untaken, Change::None)
    }

    /// Evaluates at `now` without a new sample. Aging moves the gate on, but this
    /// takes no change: the second value is the one still waiting for `record` or
    /// `take_change`, and reading it any number of times leaves it there.
    pub fn status(&mut self, now: Instant) -> (LoadStatus, Change) {
        let now = self.advance(now);
        let status = self.evaluate(now);
        (status, self.untaken)
    }

    /// Time never runs backwards inside the gate. Callers on different threads
    /// can each take `Instant::now()` before the lock and reach it in the other
    /// order, so an earlier `now` is read as the newest time seen. Every window
    /// stays sorted and pruning from the front stays correct. Readings with the
    /// same time are kept, one per call.
    fn advance(&mut self, now: Instant) -> Instant {
        let now = self.latest.map_or(now, |latest| latest.max(now));
        self.latest = Some(now);
        now
    }

    /// Moves the gate to `now` and records a close or open in `untaken`.
    fn evaluate(&mut self, now: Instant) -> LoadStatus {
        let limits = self.limits;
        let any_sample = self.any_sample;
        let mut means: [Option<Mean>; 3] = [None; 3];
        let mut meters = LoadMeter::ALL.map(|meter| (meter, Reading::Off));
        for meter in LoadMeter::ALL {
            let window = &mut self.windows[meter.index()];
            while window
                .front()
                .is_some_and(|&(at, _)| now.saturating_duration_since(at) >= WINDOW)
            {
                window.pop_front();
            }
            let mean = Mean::of(window).filter(|_| window.len() >= MIN_SAMPLES);
            meters[meter.index()].1 = if limits.bounded(meter) >= OFF {
                Reading::Off
            } else if let Some(mean) = mean {
                means[meter.index()] = Some(mean);
                Reading::Mean(mean.exact())
            } else if window.is_empty() && any_sample {
                Reading::Unknown(Unknown::Unavailable)
            } else {
                Reading::Unknown(Unknown::WarmingUp)
            };
        }

        // Only a `Mean` meter takes part. With none, both rules find nothing
        // and the gate is open: it fails open on a meter it cannot read.
        let reopen = |meter: LoadMeter| limits.bounded(meter).saturating_sub(REOPEN_MARGIN);
        let at_limit = |mean: Mean, meter: LoadMeter| mean.reaches(limits.bounded(meter));
        let above_reopen = |mean: Mean, meter: LoadMeter| !mean.at_most(reopen(meter));
        let was_closed = self.closed;
        self.closed = if was_closed {
            first_meter(&means, above_reopen).is_some()
        } else {
            first_meter(&means, at_limit).is_some()
        };

        // Name the first meter at or over its limit, else the first one still
        // in the reopen band. A closed gate always has one.
        let hold = if self.closed {
            first_meter(&means, at_limit)
                .or_else(|| first_meter(&means, above_reopen))
                .and_then(|meter| {
                    means[meter.index()].map(|mean| LoadHold {
                        meter,
                        mean_percent: mean.percent(),
                        limit_percent: limits.bounded(meter),
                    })
                })
        } else {
            None
        };
        match (was_closed, self.closed) {
            (false, true) => self.untaken = Change::Closed,
            (true, false) => self.untaken = Change::Opened,
            _ => {}
        }
        LoadStatus { hold, meters }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Time of reading `n` from a 2 second sampler that started at `base`.
    fn at(base: Instant, n: u32) -> Instant {
        base + Duration::from_secs(2 * u64::from(n))
    }

    fn cpu(percent: f64) -> LoadSample {
        LoadSample {
            cpu: Some(percent),
            ..LoadSample::default()
        }
    }

    fn all(cpu: f64, memory: f64, disk: f64) -> LoadSample {
        LoadSample {
            cpu: Some(cpu),
            memory: Some(memory),
            disk: Some(disk),
        }
    }

    fn hold(meter: LoadMeter, mean_percent: u8, limit_percent: u8) -> LoadHold {
        LoadHold {
            meter,
            mean_percent,
            limit_percent,
        }
    }

    /// Records `sample` as each reading in `range` and returns every change
    /// the gate reported, in order.
    fn feed(
        gate: &mut LoadGate,
        base: Instant,
        range: std::ops::Range<u32>,
        sample: LoadSample,
    ) -> Vec<Change> {
        range
            .map(|n| gate.record(sample, at(base, n)))
            .filter(|change| *change != Change::None)
            .collect()
    }

    fn reading(status: &LoadStatus, meter: LoadMeter) -> Reading {
        status.meters[meter.index()].1
    }

    fn mean(status: &LoadStatus, meter: LoadMeter) -> f64 {
        match reading(status, meter) {
            Reading::Mean(mean) => mean,
            other => panic!("expected a mean for {meter:?}, got {other:?}"),
        }
    }

    fn close_to(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} is not {expected}"
        );
    }

    /// Feeds a default gate 30 readings of `first`, which must close it, then
    /// 30 of `then`. Returns the gate, the time of the last reading and the
    /// changes the second phase reported.
    fn close_then(first: LoadSample, then: LoadSample) -> (LoadGate, Instant, Vec<Change>) {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..30, first), [Change::Closed]);
        let changes = feed(&mut gate, t0, 30..60, then);
        (gate, at(t0, 59), changes)
    }

    fn hold_after(first: LoadSample, then: LoadSample) -> LoadHold {
        let (mut gate, now, _) = close_then(first, then);
        gate.status(now).0.hold.expect("the gate is closed")
    }

    #[test]
    fn a_full_window_with_a_mean_equal_to_the_limit_closes_the_gate() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..30, cpu(80.0)), [Change::Closed]);
        let (status, change) = gate.status(at(t0, 29));
        assert_eq!(change, Change::None);
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 80, 80)));

        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, cpu(79.9)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(status.hold, None);
        close_to(mean(&status, LoadMeter::Cpu), 79.9);
    }

    #[test]
    fn one_spike_inside_the_window_does_not_close_the_gate() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..29, cpu(20.0)).is_empty());
        assert!(feed(&mut gate, t0, 29..30, cpu(100.0)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(status.hold, None);
        let cpu_mean = mean(&status, LoadMeter::Cpu);
        assert!(cpu_mean > 22.6 && cpu_mean < 22.7, "mean was {cpu_mean}");
    }

    #[test]
    fn the_window_holds_the_last_sixty_seconds_so_an_old_spike_ages_out() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        gate.record(cpu(100.0), at(t0, 0));
        feed(&mut gate, t0, 1..30, cpu(20.0));
        // 59.999 seconds after the spike it still counts.
        let (status, _) = gate.status(t0 + Duration::from_millis(59_999));
        close_to(mean(&status, LoadMeter::Cpu), 680.0 / 30.0);
        // At exactly 60 seconds it is gone.
        let (status, _) = gate.status(t0 + Duration::from_secs(60));
        close_to(mean(&status, LoadMeter::Cpu), 20.0);
    }

    #[test]
    fn a_closed_gate_reopens_when_the_spike_that_closed_it_ages_out() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        // Ten readings of 100, then 60. Means stay over the reopen level of 70
        // until the spike starts to leave the window at reading 30.
        assert_eq!(feed(&mut gate, t0, 0..10, cpu(100.0)), [Change::Closed]);
        assert!(feed(&mut gate, t0, 10..32, cpu(60.0)).is_empty());
        let (status, _) = gate.status(at(t0, 31));
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 70, 80)));
        // Reading 32 is the one that sees only seven spikes left (mean 69.3).
        assert_eq!(gate.record(cpu(60.0), at(t0, 32)), Change::Opened);
    }

    #[test]
    fn the_gate_reopens_only_when_every_gated_mean_is_ten_points_under_its_limit() {
        // CPU closes at 80. A mean of 71 is nine under: closed. 70 is ten under: open.
        let (mut gate, now, changes) = close_then(all(80.0, 10.0, 10.0), all(71.0, 10.0, 10.0));
        assert!(changes.is_empty());
        let (status, _) = gate.status(now);
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 71, 80)));
        let (_, _, changes) = close_then(all(80.0, 10.0, 10.0), all(70.0, 10.0, 10.0));
        assert_eq!(changes, [Change::Opened]);

        // CPU falls to 60, but a memory mean of 76 (limit 85) keeps the gate closed. 75 opens it.
        let (mut gate, now, changes) = close_then(all(80.0, 76.0, 10.0), all(60.0, 76.0, 10.0));
        assert!(changes.is_empty());
        let (status, _) = gate.status(now);
        let held = status.hold.expect("memory holds the gate");
        assert_eq!(held.meter, LoadMeter::Memory);
        assert_eq!((held.mean_percent, held.reopen_percent()), (76, 75));
        let (_, _, changes) = close_then(all(80.0, 75.0, 10.0), all(60.0, 75.0, 10.0));
        assert_eq!(changes, [Change::Opened]);
    }

    #[test]
    fn the_gate_does_not_flap_between_the_limit_and_the_reopen_level() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        // Means 79, 81, 79 and 75 over four full windows.
        let mut changes = feed(&mut gate, t0, 0..30, cpu(79.0));
        changes.extend(feed(&mut gate, t0, 30..60, cpu(81.0)));
        changes.extend(feed(&mut gate, t0, 60..90, cpu(79.0)));
        changes.extend(feed(&mut gate, t0, 90..120, cpu(75.0)));
        assert_eq!(changes, [Change::Closed]);
        let (status, _) = gate.status(at(t0, 119));
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 75, 80)));
    }

    #[test]
    fn an_unavailable_meter_is_left_out_and_reported_unknown() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        let sample = LoadSample {
            cpu: None,
            memory: Some(90.0),
            disk: Some(10.0),
        };
        assert_eq!(feed(&mut gate, t0, 0..30, sample), [Change::Closed]);
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(status.hold, Some(hold(LoadMeter::Memory, 90, 85)));
        assert_eq!(
            reading(&status, LoadMeter::Cpu),
            Reading::Unknown(Unknown::Unavailable)
        );
        close_to(mean(&status, LoadMeter::Memory), 90.0);
        close_to(mean(&status, LoadMeter::Disk), 10.0);
    }

    #[test]
    fn with_every_meter_unavailable_the_gate_is_open_and_every_meter_is_unknown() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, LoadSample::default()).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(status.hold, None);
        for meter in LoadMeter::ALL {
            assert_eq!(
                reading(&status, meter),
                Reading::Unknown(Unknown::Unavailable)
            );
        }

        // Closed first, then every sensor stops reporting: the gate opens once.
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..30, cpu(90.0)), [Change::Closed]);
        assert_eq!(
            feed(&mut gate, t0, 30..60, LoadSample::default()),
            [Change::Opened]
        );
        let (status, _) = gate.status(at(t0, 59));
        assert_eq!(status.hold, None);
        for meter in LoadMeter::ALL {
            assert_eq!(
                reading(&status, meter),
                Reading::Unknown(Unknown::Unavailable)
            );
        }
    }

    #[test]
    fn a_first_reading_at_the_limit_does_not_close_the_gate() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(gate.record(all(100.0, 100.0, 100.0), t0), Change::None);
        let (status, _) = gate.status(t0);
        assert_eq!(status.hold, None);
    }

    #[test]
    fn a_meter_with_fewer_than_five_readings_is_warming_up_and_cannot_close_the_gate() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        // A gate that has not been fed is warming up on every meter.
        let (status, change) = gate.status(t0);
        assert_eq!(change, Change::None);
        for meter in LoadMeter::ALL {
            assert_eq!(
                reading(&status, meter),
                Reading::Unknown(Unknown::WarmingUp)
            );
        }
        assert!(feed(&mut gate, t0, 0..4, all(100.0, 100.0, 100.0)).is_empty());
        let (status, _) = gate.status(at(t0, 3));
        assert_eq!(status.hold, None);
        for meter in LoadMeter::ALL {
            assert_eq!(
                reading(&status, meter),
                Reading::Unknown(Unknown::WarmingUp)
            );
        }
    }

    #[test]
    fn the_gate_closes_on_the_fifth_reading_when_the_mean_so_far_is_over_the_limit() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..4, cpu(100.0)).is_empty());
        // 100, 100, 100, 100 and 0 average exactly 80.
        assert_eq!(gate.record(cpu(0.0), at(t0, 4)), Change::Closed);
        let (status, _) = gate.status(at(t0, 4));
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 80, 80)));
        // The sixth reading takes the mean so far to 66.7, under the reopen level.
        assert_eq!(gate.record(cpu(0.0), at(t0, 5)), Change::Opened);
    }

    #[test]
    fn a_limit_of_100_turns_that_meter_off_and_the_others_still_gate() {
        let t0 = Instant::now();
        let limits = LoadLimits::new(OFF, 85, 90).expect("limits are in range");
        let mut gate = LoadGate::new(limits);
        assert!(feed(&mut gate, t0, 0..30, all(100.0, 10.0, 10.0)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(reading(&status, LoadMeter::Cpu), Reading::Off);
        assert_eq!(status.hold, None);

        assert_eq!(
            feed(&mut gate, t0, 30..60, all(100.0, 90.0, 10.0)),
            [Change::Closed]
        );
        let (status, _) = gate.status(at(t0, 59));
        assert_eq!(status.hold.map(|hold| hold.meter), Some(LoadMeter::Memory));
        assert_eq!(reading(&status, LoadMeter::Cpu), Reading::Off);
    }

    #[test]
    fn a_changed_limit_applies_from_the_next_evaluation_without_a_new_sample() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, cpu(75.0)).is_empty());

        gate.set_limits(LoadLimits::new(70, 85, 90).expect("limits are in range"));
        let (status, change) = gate.status(at(t0, 29));
        assert_eq!(change, Change::Closed);
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 75, 70)));

        // A limit of 90 reopens at 80, and the mean is 75.
        gate.set_limits(LoadLimits::new(90, 85, 90).expect("limits are in range"));
        let (status, change) = gate.status(at(t0, 29));
        assert_eq!(change, Change::Opened);
        assert_eq!(status.hold, None);
    }

    #[test]
    fn a_silent_sampler_empties_the_window_and_a_closed_gate_fails_open() {
        let t0 = Instant::now();
        let later = |seconds: u64| t0 + Duration::from_secs(seconds);
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..30, cpu(90.0)), [Change::Closed]);

        // Readings are 0 to 58 seconds old. At 106 seconds six remain: still judged.
        let (status, change) = gate.status(later(106));
        assert_eq!(change, Change::None);
        assert!(status.hold.is_some());
        // At 110 seconds four remain: warming up, nothing to judge, so it opens.
        let (status, change) = gate.status(later(110));
        assert_eq!(change, Change::Opened);
        assert_eq!(status.hold, None);
        assert_eq!(
            reading(&status, LoadMeter::Cpu),
            Reading::Unknown(Unknown::WarmingUp)
        );
        // Polling took nothing: the open waits for the caller that wakes on it, once.
        assert_eq!(gate.take_change(later(110)), Change::Opened);
        assert_eq!(gate.take_change(later(110)), Change::None);
        // At 120 seconds none remain.
        let (status, change) = gate.status(later(120));
        assert_eq!(change, Change::None);
        assert_eq!(
            reading(&status, LoadMeter::Cpu),
            Reading::Unknown(Unknown::Unavailable)
        );
    }

    #[test]
    fn the_hold_names_the_meter_at_or_over_its_limit_before_one_in_the_reopen_band() {
        // Every meter over its limit: the first in order, CPU.
        let all_over = all(85.0, 90.0, 95.0);
        assert_eq!(hold_after(all_over, all_over), hold(LoadMeter::Cpu, 85, 80));

        // CPU is in the reopen band (75 is over 70) but memory is over its limit: memory.
        let memory_over = all(75.0, 86.0, 10.0);
        assert_eq!(
            hold_after(memory_over, memory_over),
            hold(LoadMeter::Memory, 86, 85)
        );

        // Closed by CPU, then CPU falls to 60 while memory (80 over 75) and disk (85 over 80)
        // stay in the band: the first of those in order, memory.
        assert_eq!(
            hold_after(all(85.0, 80.0, 85.0), all(60.0, 80.0, 85.0)),
            hold(LoadMeter::Memory, 80, 85)
        );
    }

    #[test]
    fn limits_outside_50_to_100_are_refused_with_the_meter_named() {
        assert_eq!(LoadLimits::new(49, 85, 90), Err(LimitError(LoadMeter::Cpu)));
        assert_eq!(
            LoadLimits::new(80, 101, 90),
            Err(LimitError(LoadMeter::Memory))
        );
        assert_eq!(LoadLimits::new(80, 85, 0), Err(LimitError(LoadMeter::Disk)));
        // The first meter in gate order is the one named.
        assert_eq!(
            LoadLimits::new(255, 49, 90),
            Err(LimitError(LoadMeter::Cpu))
        );
        assert_eq!(
            LoadLimits::new(50, 50, 50),
            Ok(LoadLimits {
                cpu: 50,
                memory: 50,
                disk: 50
            })
        );
        assert_eq!(
            LoadLimits::new(100, 100, 100),
            Ok(LoadLimits {
                cpu: 100,
                memory: 100,
                disk: 100
            })
        );
        assert_eq!(
            LimitError(LoadMeter::Memory).to_string(),
            "memory limit must be from 50 to 100 percent"
        );
    }

    #[test]
    fn default_limits_are_80_85_90() {
        let limits = LoadLimits::default();
        assert_eq!(LoadMeter::ALL.map(|meter| limits.of(meter)), [80, 85, 90]);
    }

    #[test]
    fn a_change_is_reported_once_per_close_and_once_per_open() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..30, cpu(90.0)), [Change::Closed]);
        // Reading the status does not report the change a second time.
        assert_eq!(gate.status(at(t0, 29)).1, Change::None);
        assert_eq!(feed(&mut gate, t0, 30..60, cpu(10.0)), [Change::Opened]);
        assert_eq!(gate.status(at(t0, 59)).1, Change::None);
        assert_eq!(feed(&mut gate, t0, 60..90, cpu(90.0)), [Change::Closed]);
        assert_eq!(feed(&mut gate, t0, 90..120, cpu(10.0)), [Change::Opened]);
    }

    #[test]
    fn polling_status_does_not_consume_the_opened_edge() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..10, cpu(100.0)), [Change::Closed]);
        assert!(feed(&mut gate, t0, 10..32, cpu(60.0)).is_empty());

        // At the time of reading 32 the window holds 29 readings with a mean of 69.7, under
        // the reopen level of 70, so a poll that comes before that sample sees the gate
        // open. A poll takes nothing, however often it asks.
        for _ in 0..3 {
            let (status, change) = gate.status(at(t0, 32));
            assert_eq!(status.hold, None);
            assert_eq!(change, Change::Opened);
        }
        // The sample that follows, and only it, reports the open.
        assert_eq!(gate.record(cpu(60.0), at(t0, 32)), Change::Opened);
        assert_eq!(gate.take_change(at(t0, 32)), Change::None);
        assert_eq!(gate.status(at(t0, 32)).1, Change::None);

        // A newer move replaces one nobody took: a close then an open is the open.
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, cpu(75.0)).is_empty());
        gate.set_limits(LoadLimits::new(70, 85, 90).expect("limits are in range"));
        assert_eq!(gate.status(at(t0, 29)).1, Change::Closed);
        gate.set_limits(LoadLimits::new(90, 85, 90).expect("limits are in range"));
        assert_eq!(gate.take_change(at(t0, 29)), Change::Opened);
        assert_eq!(gate.take_change(at(t0, 29)), Change::None);
    }

    #[test]
    fn a_mean_that_is_exactly_the_limit_closes_though_its_readings_are_not_exact_in_binary() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        // Three of 79.8 and three of 80.2 average exactly 80.
        for (n, value) in (0..).zip([79.8, 80.2, 79.8, 80.2, 79.8]) {
            assert_eq!(gate.record(cpu(value), at(t0, n)), Change::None);
        }
        assert_eq!(gate.record(cpu(80.2), at(t0, 5)), Change::Closed);
    }

    #[test]
    fn means_are_compared_exactly_in_hundredths_of_a_percent() {
        // A mean one hundredth over the reopen level keeps the gate closed.
        let (_, _, changes) = close_then(all(80.0, 10.0, 10.0), all(70.01, 10.0, 10.0));
        assert!(changes.is_empty());
        // 70.004 rounds to 70.00, which is ten under.
        let (_, _, changes) = close_then(all(80.0, 10.0, 10.0), all(70.004, 10.0, 10.0));
        assert_eq!(changes, [Change::Opened]);
        // 79.99 is a hundredth under the limit.
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, cpu(79.99)).is_empty());
    }

    #[test]
    fn a_reading_that_is_not_a_finite_number_is_left_out_of_the_window() {
        let t0 = Instant::now();
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut gate = LoadGate::new(LoadLimits::default());
            assert!(feed(&mut gate, t0, 0..30, cpu(value)).is_empty());
            let (status, _) = gate.status(at(t0, 29));
            assert_eq!(status.hold, None);
            assert_eq!(
                reading(&status, LoadMeter::Cpu),
                Reading::Unknown(Unknown::Unavailable)
            );
        }
    }

    #[test]
    fn a_reading_outside_0_to_100_is_clamped_into_the_range() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert_eq!(feed(&mut gate, t0, 0..30, cpu(250.0)), [Change::Closed]);
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 100, 80)));

        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, cpu(-40.0)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        close_to(mean(&status, LoadMeter::Cpu), 0.0);
    }

    #[test]
    fn limits_built_without_the_constructor_are_clamped_into_range_by_the_gate() {
        let t0 = Instant::now();
        // 5 reads as 50 and 200 reads as 100 (off).
        let mut gate = LoadGate::new(LoadLimits {
            cpu: 5,
            memory: 200,
            disk: 90,
        });
        assert_eq!(
            feed(&mut gate, t0, 0..30, all(60.0, 100.0, 10.0)),
            [Change::Closed]
        );
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(status.hold, Some(hold(LoadMeter::Cpu, 60, 50)));
        assert_eq!(reading(&status, LoadMeter::Memory), Reading::Off);
    }

    #[test]
    fn a_reading_stamped_before_the_newest_one_is_stored_at_the_newest_time() {
        let t0 = Instant::now();
        let late = t0 + Duration::from_secs(100);
        let mut gate = LoadGate::new(LoadLimits::default());
        for n in 0..5 {
            gate.record(cpu(100.0), late + Duration::from_secs(n));
        }
        // A reading taken at t0 arrives after them. It still counts; it does not
        // go stale at once and it does not disturb the order of the window.
        assert_eq!(gate.record(cpu(0.0), t0), Change::None);
        let (status, _) = gate.status(late + Duration::from_secs(5));
        close_to(mean(&status, LoadMeter::Cpu), 500.0 / 6.0);
        // Every reading, the late one included, is over 60 seconds old at late + 65.
        let (status, change) = gate.status(late + Duration::from_secs(65));
        assert_eq!(change, Change::Opened);
        assert_eq!(
            reading(&status, LoadMeter::Cpu),
            Reading::Unknown(Unknown::Unavailable)
        );
    }

    #[test]
    fn readings_with_the_same_timestamp_each_count_and_age_out_together() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        for _ in 0..4 {
            assert_eq!(gate.record(cpu(90.0), t0), Change::None);
        }
        assert_eq!(gate.record(cpu(90.0), t0), Change::Closed);
        let (status, _) = gate.status(t0 + Duration::from_secs(59));
        assert!(status.hold.is_some());
        let (status, change) = gate.status(t0 + Duration::from_secs(60));
        assert_eq!(change, Change::Opened);
        assert_eq!(
            reading(&status, LoadMeter::Cpu),
            Reading::Unknown(Unknown::Unavailable)
        );
    }

    #[test]
    fn the_window_keeps_at_most_the_newest_600_readings() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        for _ in 0..MAX_READINGS {
            gate.record(cpu(0.0), t0);
        }
        for _ in 0..100 {
            gate.record(cpu(100.0), t0);
        }
        let (status, _) = gate.status(t0);
        // 500 zeros and 100 hundreds. Without the bound the mean would be 14.3.
        close_to(mean(&status, LoadMeter::Cpu), 100.0 * 100.0 / 600.0);
    }

    #[test]
    fn a_reading_reports_its_mean_in_whole_percents_rounded_down() {
        let t0 = Instant::now();
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..30, all(79.99, 10.0, 10.0)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(reading(&status, LoadMeter::Cpu).mean_percent(), Some(79));
        assert_eq!(reading(&status, LoadMeter::Memory).mean_percent(), Some(10));

        // A meter that is off has no mean.
        let limits = LoadLimits::new(OFF, 85, 90).expect("limits are in range");
        let mut gate = LoadGate::new(limits);
        assert!(feed(&mut gate, t0, 0..29, all(20.0, 80.0, 0.0)).is_empty());
        assert!(feed(&mut gate, t0, 29..30, all(100.0, 80.0, 0.0)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(reading(&status, LoadMeter::Cpu).mean_percent(), None);
        assert_eq!(reading(&status, LoadMeter::Memory).mean_percent(), Some(80));

        // 22.67 reads as 22, and a meter with no readings has no mean.
        let mut gate = LoadGate::new(LoadLimits::default());
        assert!(feed(&mut gate, t0, 0..29, cpu(20.0)).is_empty());
        assert!(feed(&mut gate, t0, 29..30, cpu(100.0)).is_empty());
        let (status, _) = gate.status(at(t0, 29));
        assert_eq!(reading(&status, LoadMeter::Cpu).mean_percent(), Some(22));
        assert_eq!(reading(&status, LoadMeter::Disk).mean_percent(), None);
    }

    #[test]
    fn a_hold_reopens_ten_points_under_its_limit() {
        assert_eq!(hold(LoadMeter::Cpu, 91, 80).reopen_percent(), 70);
        assert_eq!(hold(LoadMeter::Disk, 91, 100).reopen_percent(), 90);
        assert_eq!(hold(LoadMeter::Disk, 91, 5).reopen_percent(), 0);
    }

    #[test]
    fn meters_have_stable_wire_names_and_labels() {
        assert_eq!(
            LoadMeter::ALL.map(LoadMeter::as_str),
            ["cpu", "memory", "disk"]
        );
        assert_eq!(
            LoadMeter::ALL.map(LoadMeter::label),
            ["CPU", "memory", "disk"]
        );
    }
}
