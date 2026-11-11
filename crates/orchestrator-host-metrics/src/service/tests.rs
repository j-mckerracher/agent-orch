use super::*;
use crate::model::{MeterId, MeterKind, Unit};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

const WAIT: Duration = Duration::from_secs(10);

fn ok(meter: MeterId, value: f64, total: Option<f64>) -> Meter {
    let (kind, unit) = shape(meter);
    Meter::ok(meter, kind, unit, "test", value, total)
}

fn full_sample() -> Vec<Meter> {
    vec![
        ok(MeterId::Cpu, 12.5, None),
        ok(MeterId::Memory, 4.0, Some(8.0)),
        ok(MeterId::Disk, 1.0, Some(2.0)),
        Meter::missing(
            MeterId::Gpu,
            MeterState::Unsupported,
            MeterKind::Utilization,
            Unit::Percent,
            None,
            "no source",
        ),
    ]
}

struct Fixed {
    meters: Vec<Meter>,
    keep_alive: Arc<()>,
}

impl Probe for Fixed {
    fn sample(&mut self) -> Vec<Meter> {
        let _ = &self.keep_alive;
        self.meters.clone()
    }
}

/// Blocks inside `sample` until released, like a wedged operating-system call.
struct Gated {
    release: Receiver<()>,
}

impl Probe for Gated {
    fn sample(&mut self) -> Vec<Meter> {
        let _ = self.release.recv();
        full_sample()
    }
}

fn wait_for(metrics: &HostMetrics, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = metrics.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "timed out: {snapshot:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn fixed(meters: Vec<Meter>) -> Fixed {
    Fixed {
        meters,
        keep_alive: Arc::new(()),
    }
}

#[test]
fn a_blocked_sampler_never_blocks_readers_and_nothing_is_reported_before_a_sample() {
    let (release, gate): (Sender<()>, Receiver<()>) = mpsc::channel();
    let metrics = HostMetrics::start(Gated { release: gate }, DEFAULT_INTERVAL);
    for _ in 0..3 {
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
        assert_eq!(snapshot.meters.len(), 4);
        for meter in &snapshot.meters {
            assert_eq!(meter.state, MeterState::Unavailable);
            assert_eq!(meter.value, None);
            assert_eq!(meter.observed_at, None);
            assert!(meter.reason.as_deref().unwrap().contains("no sample"));
            assert_eq!(meter.sample_interval_ms, 2_000);
        }
    }
    release.send(()).unwrap();
    let snapshot = wait_for(&metrics, |snapshot| {
        snapshot.meters[0].state == MeterState::Ok
    });
    assert_eq!(snapshot.meters[0].value, Some(12.5));
}

#[test]
fn a_completed_sample_is_served_with_time_unit_and_kind() {
    let metrics = HostMetrics::start(fixed(full_sample()), DEFAULT_INTERVAL);
    let snapshot = wait_for(&metrics, |snapshot| {
        snapshot.meters[0].observed_at.is_some()
    });
    let memory = &snapshot.meters[1];
    assert_eq!(memory.meter, MeterId::Memory);
    assert_eq!(memory.state, MeterState::Ok);
    assert_eq!((memory.value, memory.total), (Some(4.0), Some(8.0)));
    assert_eq!(
        (memory.kind, memory.unit),
        (MeterKind::Capacity, Unit::Bytes)
    );
    let observed = memory.observed_at.as_deref().unwrap();
    assert_eq!(observed.len(), "2026-11-10T18:23:20.007Z".len());
    assert!(observed.ends_with('Z'));
    assert_eq!(snapshot.meters[3].state, MeterState::Unsupported);
    assert_eq!(snapshot.meters[3].reason.as_deref(), Some("no source"));
    assert_eq!(snapshot.sample_interval_ms, 2_000);
}

#[test]
fn a_sample_that_is_too_old_is_unavailable_and_keeps_its_scope() {
    let metrics = HostMetrics::start(fixed(full_sample()), MIN_INTERVAL);
    wait_for(&metrics, |snapshot| {
        snapshot.meters[0].observed_at.is_some()
    });
    let later = SystemTime::now() + MIN_INTERVAL * (STALE_AFTER_INTERVALS + 20);
    let snapshot = metrics.snapshot_at(later);
    let cpu = &snapshot.meters[0];
    assert_eq!(cpu.state, MeterState::Unavailable);
    assert_eq!((cpu.value, cpu.total), (None, None));
    assert_eq!(cpu.scope.as_deref(), Some("test"));
    assert!(cpu.observed_at.is_some());
    assert!(cpu.reason.as_deref().unwrap().contains("stale"));
    // A meter that was already missing keeps its own state.
    assert_eq!(snapshot.meters[3].state, MeterState::Unsupported);
}

#[test]
fn a_clock_that_steps_back_does_not_make_a_sample_stale() {
    let metrics = HostMetrics::start(fixed(full_sample()), MIN_INTERVAL);
    wait_for(&metrics, |snapshot| {
        snapshot.meters[0].observed_at.is_some()
    });
    let earlier = SystemTime::now() - Duration::from_secs(3_600);
    assert_eq!(metrics.snapshot_at(earlier).meters[0].state, MeterState::Ok);
}

#[test]
fn the_interval_has_a_floor_and_the_sampler_repeats() {
    let metrics = HostMetrics::start(fixed(full_sample()), Duration::from_millis(1));
    assert_eq!(metrics.interval(), MIN_INTERVAL);
    let first = wait_for(&metrics, |snapshot| {
        snapshot.meters[0].observed_at.is_some()
    });
    let first_at = first.meters[0].observed_at.clone();
    wait_for(&metrics, |snapshot| {
        snapshot.meters[0].observed_at != first_at
    });
}

#[test]
fn dropping_the_handle_stops_the_sampler() {
    let keep_alive = Arc::new(());
    let metrics = HostMetrics::start(
        Fixed {
            meters: full_sample(),
            keep_alive: keep_alive.clone(),
        },
        MIN_INTERVAL,
    );
    wait_for(&metrics, |snapshot| {
        snapshot.meters[0].observed_at.is_some()
    });
    drop(metrics);
    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&keep_alive) > 1 {
        assert!(Instant::now() < deadline, "sampler thread did not stop");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn invalid_probe_output_is_replaced_by_unavailable_never_a_number() {
    let (kind, unit) = shape(MeterId::Memory);
    let mut no_value = Meter::ok(MeterId::Memory, kind, unit, "m", 1.0, Some(2.0));
    no_value.value = None;
    let meters = normalize(vec![
        ok(MeterId::Cpu, f64::NAN, None),
        no_value,
        ok(MeterId::Disk, -1.0, Some(5.0)),
        ok(MeterId::Gpu, 101.0, None),
    ]);
    assert_eq!(meters.len(), 4);
    for meter in &meters {
        assert_eq!(meter.state, MeterState::Unavailable, "{:?}", meter.meter);
        assert_eq!((meter.value, meter.total), (None, None));
        assert!(meter.reason.as_deref().unwrap().contains("invalid"));
    }
}

#[test]
fn missing_resources_are_added_and_order_is_fixed() {
    let meters = normalize(vec![
        ok(MeterId::Gpu, 5.0, None),
        ok(MeterId::Gpu, 6.0, None),
        ok(MeterId::Cpu, 1.0, None),
    ]);
    let ids: Vec<_> = meters.iter().map(|meter| meter.meter).collect();
    assert_eq!(
        ids,
        [
            MeterId::Cpu,
            MeterId::Memory,
            MeterId::Disk,
            MeterId::Gpu,
            MeterId::Gpu
        ]
    );
    assert_eq!(meters[1].state, MeterState::Unavailable);
    assert_eq!(meters[2].state, MeterState::Unavailable);
    assert_eq!(meters[3].state, MeterState::Ok);
}

#[test]
fn a_missing_meter_drops_any_value_a_probe_left_on_it() {
    let mut meter = Meter::unavailable(MeterId::Cpu, MeterKind::Utilization, Unit::Percent, "x");
    meter.value = Some(0.0);
    meter.total = Some(1.0);
    let meters = normalize(vec![meter]);
    assert_eq!((meters[0].value, meters[0].total), (None, None));
}

#[test]
fn the_wire_form_always_includes_every_field() {
    let (_release, gate) = mpsc::channel();
    let metrics = HostMetrics::start(Gated { release: gate }, DEFAULT_INTERVAL);
    let value = serde_json::to_value(metrics.snapshot()).unwrap();
    let cpu = &value["meters"][0];
    assert_eq!(cpu["meter"], "cpu");
    assert_eq!(cpu["state"], "unavailable");
    assert_eq!(cpu["kind"], "utilization");
    assert_eq!(cpu["unit"], "percent");
    for field in ["scope", "value", "total", "observed_at"] {
        assert!(cpu[field].is_null(), "{field}");
    }
    assert!(cpu["reason"].is_string());
    assert_eq!(cpu["sample_interval_ms"], 2_000);
    assert_eq!(value["schema_version"], 1);
}
