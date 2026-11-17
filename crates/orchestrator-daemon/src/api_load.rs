//! The daemon's machine load gate (DPES-1105 phase 2c).
//!
//! The pure gate lives in `orchestrator-core`; this wraps it for threads. The host
//! sampler feeds it through [`LoadHandle::record`], the executor's scheduler pass asks
//! it for a [`StartGate`], and a closed-to-open change wakes the executor once so the
//! queue does not wait for the next poll.
//!
//! The sampler calls `record` on its own thread, so `record` does a short hand-off
//! only: one lock, no I/O, no store access, no waiting on another thread. Readers
//! (`status`, `start_gate`, `read`) never take the change, so a poll of `settings.get`
//! or `queue.snapshot` cannot swallow the open that the executor is waiting for.
//!
//! Nothing here holds a path. A poisoned lock is recovered, never a panic: a
//! panic in the sampler's observer must not take every reader down with it.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use orchestrator_core::load_gate::{Change, LoadGate, LoadLimits, LoadSample, LoadStatus};
use orchestrator_core::scheduler::StartGate;
use orchestrator_host_metrics::{Meter, MeterId, MeterState};

struct State {
    gate: LoadGate,
    limits: LoadLimits,
}

/// The limits and the gate as one reader sees them at one moment.
#[derive(Clone, Debug)]
pub(crate) struct LoadReading {
    pub(crate) limits: LoadLimits,
    pub(crate) status: LoadStatus,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// The pure gate plus the wake for a closed-to-open change.
pub(crate) struct LoadHandle {
    state: Mutex<State>,
    on_open: Mutex<Option<Wake>>,
}

impl Default for LoadHandle {
    /// The design limits, nothing to wake. What the tests use.
    fn default() -> Self {
        Self::new(LoadLimits::default())
    }
}

impl LoadHandle {
    pub(crate) fn new(limits: LoadLimits) -> Self {
        Self {
            state: Mutex::new(State {
                gate: LoadGate::new(limits),
                limits,
            }),
            on_open: Mutex::new(None),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Adds one host sample taken at `now`. Runs the wake once when this sample
    /// opened a closed gate, with no lock held.
    pub(crate) fn record(&self, sample: LoadSample, now: Instant) {
        let change = self.state().gate.record(sample, now);
        self.wake_if_opened(change);
    }

    /// The limits and the gate at `now`. Read-only: it never takes the change.
    pub(crate) fn read(&self, now: Instant) -> LoadReading {
        let mut state = self.state();
        let limits = state.limits;
        let (status, _) = state.gate.status(now);
        LoadReading { limits, status }
    }

    pub(crate) fn status(&self, now: Instant) -> LoadStatus {
        self.read(now).status
    }

    /// What the scheduler may do at `now`. Read-only. Phase 3 wake turns call this
    /// before they start an agent.
    pub(crate) fn start_gate(&self, now: Instant) -> StartGate {
        self.status(now)
            .hold
            .map_or(StartGate::Open, StartGate::Closed)
    }

    /// New limits, applied over the readings already in the window. A change that
    /// opens the gate wakes the executor like a sample would.
    pub(crate) fn set_limits(&self, limits: LoadLimits) {
        let change = {
            let mut state = self.state();
            state.limits = limits;
            state.gate.set_limits(limits);
            state.gate.take_change(Instant::now())
        };
        self.wake_if_opened(change);
    }

    /// Registers the one wake. A second call replaces the first.
    pub(crate) fn on_open(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self
            .on_open
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(Arc::new(wake));
    }

    fn wake_if_opened(&self, change: Change) {
        if change != Change::Opened {
            return;
        }
        let wake = self
            .on_open
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}

/// The three gated percentages of one host snapshot. A meter that is not `ok`, has no
/// value, or (memory and disk) has no total gives `None`. Memory and disk are
/// `value / total * 100`; CPU is `value`. GPU is never read.
pub(crate) fn sample_of(meters: &[Meter]) -> LoadSample {
    let percent = |id: MeterId| {
        let meter = meters.iter().find(|meter| meter.meter == id)?;
        if meter.state != MeterState::Ok {
            return None;
        }
        let value = meter.value?;
        match id {
            MeterId::Cpu => Some(value),
            _ => meter
                .total
                .filter(|total| *total > 0.0)
                .map(|total| value / total * 100.0),
        }
    };
    LoadSample {
        cpu: percent(MeterId::Cpu),
        memory: percent(MeterId::Memory),
        disk: percent(MeterId::Disk),
    }
}

#[cfg(test)]
#[path = "api_load_tests.rs"]
pub(crate) mod tests;
