use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use orchestrator_core::scheduler::StartGate;
use orchestrator_host_metrics::{MeterKind, Unit};

use super::*;

/// The time of reading `n`, two seconds apart, as the sampler spaces them.
pub(crate) fn at(t0: Instant, n: u32) -> Instant {
    t0 + Duration::from_secs(2 * u64::from(n))
}

pub(crate) fn cpu(percent: f64) -> LoadSample {
    LoadSample {
        cpu: Some(percent),
        ..LoadSample::default()
    }
}

/// Closes the gate with five CPU readings of 95 percent (limit 80). The readings are
/// stamped ahead of now, so a later read at the real time still sees them as new.
pub(crate) fn close_gate(load: &LoadHandle) -> Instant {
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    at(t0, 4)
}

fn counted(load: &LoadHandle) -> Arc<AtomicUsize> {
    let wakes = Arc::new(AtomicUsize::new(0));
    let seen = wakes.clone();
    load.on_open(move || {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    wakes
}

fn meter(id: MeterId, state: MeterState, value: Option<f64>, total: Option<f64>) -> Meter {
    Meter {
        meter: id,
        state,
        kind: if id == MeterId::Cpu {
            MeterKind::Utilization
        } else {
            MeterKind::Capacity
        },
        unit: if id == MeterId::Cpu {
            Unit::Percent
        } else {
            Unit::Bytes
        },
        scope: Some("test".to_owned()),
        value,
        total,
        observed_at: None,
        sample_interval_ms: 0,
        reason: None,
    }
}

#[test]
fn sample_of_maps_cpu_memory_and_disk_to_percentages_and_leaves_out_meters_that_are_not_ok() {
    let meters = [
        meter(MeterId::Cpu, MeterState::Ok, Some(42.5), None),
        meter(MeterId::Memory, MeterState::Ok, Some(2.0), Some(8.0)),
        meter(MeterId::Disk, MeterState::Ok, Some(30.0), Some(120.0)),
        meter(MeterId::Gpu, MeterState::Ok, Some(99.0), None),
    ];
    assert_eq!(
        sample_of(&meters),
        LoadSample {
            cpu: Some(42.5),
            memory: Some(25.0),
            disk: Some(25.0),
        }
    );

    let missing = [
        meter(MeterId::Cpu, MeterState::Unavailable, None, None),
        meter(MeterId::Memory, MeterState::Unsupported, None, None),
        meter(MeterId::Disk, MeterState::Ok, Some(30.0), Some(0.0)),
    ];
    assert_eq!(sample_of(&missing), LoadSample::default());
    assert_eq!(sample_of(&[]), LoadSample::default());
}

#[test]
fn a_new_load_handle_is_open_with_the_design_limits() {
    let load = LoadHandle::default();
    let reading = load.read(Instant::now());
    assert_eq!(reading.limits, LoadLimits::default());
    assert_eq!(reading.status.hold, None);
    assert_eq!(load.start_gate(Instant::now()), StartGate::Open);
}

#[test]
fn the_wake_turn_hook_reports_a_closed_gate() {
    let load = LoadHandle::default();
    let last = close_gate(&load);
    match load.start_gate(last) {
        StartGate::Closed(hold) => {
            assert_eq!(hold.meter, orchestrator_core::load_gate::LoadMeter::Cpu);
            assert_eq!(hold.mean_percent, 95);
            assert_eq!(hold.limit_percent, 80);
        }
        StartGate::Open => panic!("the gate should be closed"),
    }
}

#[test]
fn opening_the_gate_runs_the_wake_once() {
    let load = LoadHandle::default();
    let wakes = counted(&load);
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 0, "closing does not wake");
    for n in 5..12 {
        load.record(cpu(0.0), at(t0, n));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(load.start_gate(at(t0, 11)), StartGate::Open);
}

#[test]
fn readers_never_take_the_open_so_the_next_sample_still_reports_it() {
    let load = LoadHandle::default();
    let wakes = counted(&load);
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    // A minute later every reading has aged out: the gate is open, and only readers saw it.
    let later = t0 + Duration::from_secs(70);
    for _ in 0..3 {
        assert_eq!(load.start_gate(later), StartGate::Open);
        load.status(later);
        load.read(later);
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 0, "a reader never wakes");
    load.record(cpu(10.0), later);
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        1,
        "the sampler's next record does"
    );
}

#[test]
fn raising_a_limit_to_off_opens_the_gate_and_wakes_once() {
    let load = LoadHandle::default();
    let wakes = counted(&load);
    close_gate(&load);
    load.set_limits(LoadLimits::new(100, 85, 90).unwrap());
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(load.read(Instant::now()).limits.cpu, 100);
    assert_eq!(load.start_gate(Instant::now()), StartGate::Open);
}

#[test]
fn a_poisoned_gate_mutex_does_not_panic_a_poller() {
    let load = Arc::new(LoadHandle::default());
    let poisoner = load.clone();
    let panicked = thread::spawn(move || {
        let _held = poisoner.state();
        panic!("poison the load gate on purpose");
    })
    .join();
    assert!(panicked.is_err());
    assert!(load.state.is_poisoned());

    let now = Instant::now();
    assert_eq!(load.start_gate(now), StartGate::Open);
    load.status(now);
    load.record(cpu(10.0), now);
    load.set_limits(LoadLimits::default());
    load.on_open(|| {});
}

#[test]
fn a_wake_with_no_listener_is_harmless() {
    let load = LoadHandle::default();
    close_gate(&load);
    load.set_limits(LoadLimits::new(100, 100, 100).unwrap());
    assert_eq!(load.start_gate(Instant::now()), StartGate::Open);
}
