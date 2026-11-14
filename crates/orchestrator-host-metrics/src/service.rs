//! Sampled cache. One worker thread samples a [`Probe`]; readers copy the
//! last result and never wait for it.

use crate::model::{rfc3339_utc, shape, Meter, MeterState, Snapshot, ALL_METERS};
use crate::SCHEMA_VERSION;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime};

pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(2);
/// Shortest allowed interval. CPU utilization is a difference between two
/// reads and the operating systems need about 200 ms between them.
pub const MIN_INTERVAL: Duration = Duration::from_millis(500);
/// A cached sample older than this many intervals is reported `unavailable`
/// instead of being served as current (a wedged or dead sampler).
pub const STALE_AFTER_INTERVALS: u32 = 5;

/// A source of readings. It runs only on the sampler thread, so it may block
/// and may keep state between samples (CPU utilization needs the previous
/// read). It returns the meters it could attempt; the cache fills in any
/// meter it omits as `unavailable`.
pub trait Probe: Send + 'static {
    fn sample(&mut self) -> Vec<Meter>;
}

struct Cached {
    observed: SystemTime,
    meters: Vec<Meter>,
}

struct State {
    stop: bool,
    cached: Option<Cached>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Handle to the sampler. Dropping it asks the worker to stop but does not
/// wait for it, so a probe blocked in the operating system cannot delay
/// shutdown; the thread ends when its sample returns or the process exits.
pub struct HostMetrics {
    shared: Arc<Shared>,
    interval: Duration,
}

impl HostMetrics {
    /// Starts sampling `probe` immediately and then every `interval`
    /// (raised to [`MIN_INTERVAL`] when shorter).
    pub fn start(mut probe: impl Probe, interval: Duration) -> Self {
        let interval = interval.max(MIN_INTERVAL);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                stop: false,
                cached: None,
            }),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        thread::Builder::new()
            .name("host-metrics".to_owned())
            .spawn(move || loop {
                let meters = normalize(probe.sample());
                let observed = SystemTime::now();
                let mut state = worker.lock();
                if state.stop {
                    return;
                }
                state.cached = Some(Cached { observed, meters });
                let (state, _) = worker
                    .wake
                    .wait_timeout_while(state, interval, |state| !state.stop)
                    .unwrap_or_else(|error| error.into_inner());
                if state.stop {
                    return;
                }
            })
            .expect("host metrics sampler thread starts");
        Self { shared, interval }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// The last sample, without sampling. Holds the cache lock only to copy.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot_at(SystemTime::now())
    }

    pub(crate) fn snapshot_at(&self, now: SystemTime) -> Snapshot {
        let interval_ms = u64::try_from(self.interval.as_millis()).unwrap_or(u64::MAX);
        let cached = self
            .shared
            .lock()
            .cached
            .as_ref()
            .map(|cached| (cached.observed, cached.meters.clone()));
        let meters = match cached {
            None => ALL_METERS
                .iter()
                .map(|&id| {
                    let (kind, unit) = shape(id);
                    let mut meter =
                        Meter::unavailable(id, kind, unit, "no sample has completed yet");
                    meter.sample_interval_ms = interval_ms;
                    meter
                })
                .collect(),
            Some((observed, mut meters)) => {
                let observed_at = rfc3339_utc(observed);
                // A clock that stepped back makes the age zero, not stale.
                let age = now.duration_since(observed).unwrap_or(Duration::ZERO);
                let stale = age > self.interval * STALE_AFTER_INTERVALS;
                for meter in &mut meters {
                    meter.observed_at = Some(observed_at.clone());
                    meter.sample_interval_ms = interval_ms;
                    if stale && meter.state == MeterState::Ok {
                        *meter = Meter {
                            observed_at: meter.observed_at.take(),
                            sample_interval_ms: interval_ms,
                            ..Meter::missing(
                                meter.meter,
                                MeterState::Unavailable,
                                meter.kind,
                                meter.unit,
                                meter.scope.take(),
                                "the last sample is stale; the sampler is not running",
                            )
                        };
                    }
                }
                meters
            }
        };
        Snapshot {
            schema_version: SCHEMA_VERSION,
            sample_interval_ms: interval_ms,
            meters,
        }
    }
}

impl Drop for HostMetrics {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
    }
}

/// Enforces the wire invariants on whatever a probe returned: `ok` needs a
/// finite, non-negative value (a total, for capacity); every resource appears
/// at least once; order is fixed.
fn normalize(mut meters: Vec<Meter>) -> Vec<Meter> {
    for meter in &mut meters {
        if meter.state != MeterState::Ok {
            meter.value = None;
            meter.total = None;
            continue;
        }
        let usable = |number: Option<f64>| number.is_some_and(|n| n.is_finite() && n >= 0.0);
        let valid = usable(meter.value)
            && (meter.total.is_none() || usable(meter.total))
            && (meter.unit != crate::Unit::Percent || meter.value.is_some_and(|v| v <= 100.0));
        if !valid {
            *meter = Meter::missing(
                meter.meter,
                MeterState::Unavailable,
                meter.kind,
                meter.unit,
                meter.scope.take(),
                "the source returned an invalid reading",
            );
        }
    }
    let mut ordered = Vec::with_capacity(meters.len().max(ALL_METERS.len()));
    for id in ALL_METERS {
        let before = ordered.len();
        ordered.extend(meters.iter().filter(|meter| meter.meter == id).cloned());
        if ordered.len() == before {
            let (kind, unit) = shape(id);
            ordered.push(Meter::unavailable(
                id,
                kind,
                unit,
                "the source returned no reading",
            ));
        }
    }
    ordered
}

#[cfg(test)]
mod tests;
