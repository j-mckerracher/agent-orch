use super::*;
use std::thread;
use std::time::Duration;

fn volume(mount: &Path, total: u64, available: u64) -> Volume {
    Volume {
        mount: mount.to_path_buf(),
        total,
        available,
    }
}

fn assert_missing(meter: &Meter, id: MeterId, state: MeterState) {
    assert_eq!(meter.meter, id);
    assert_eq!(meter.state, state);
    assert_eq!(
        meter.value, None,
        "a missing reading must not carry a value"
    );
    assert_eq!(meter.total, None);
    assert!(meter
        .reason
        .as_deref()
        .is_some_and(|reason| !reason.is_empty()));
}

#[test]
fn cpu_without_a_baseline_is_unavailable_not_zero() {
    assert_missing(&cpu_meter(None), MeterId::Cpu, MeterState::Unavailable);
}

#[test]
fn cpu_rejects_non_finite_and_clamps_range() {
    assert_missing(
        &cpu_meter(Some(f32::NAN)),
        MeterId::Cpu,
        MeterState::Unavailable,
    );
    assert_eq!(cpu_meter(Some(250.0)).value, Some(100.0));
    let idle = cpu_meter(Some(0.0));
    assert_eq!((idle.state, idle.value), (MeterState::Ok, Some(0.0)));
    assert_eq!(
        (idle.kind, idle.unit),
        (MeterKind::Utilization, Unit::Percent)
    );
}

#[test]
fn memory_reports_used_out_of_total() {
    let meter = memory_meter(16_000, 6_000);
    assert_eq!(meter.state, MeterState::Ok);
    assert_eq!((meter.value, meter.total), (Some(6_000.0), Some(16_000.0)));
    assert_eq!((meter.kind, meter.unit), (MeterKind::Capacity, Unit::Bytes));
}

#[test]
fn memory_rejects_zero_total_and_inconsistent_usage() {
    assert_missing(
        &memory_meter(0, 0),
        MeterId::Memory,
        MeterState::Unavailable,
    );
    assert_missing(
        &memory_meter(100, 101),
        MeterId::Memory,
        MeterState::Unavailable,
    );
}

#[test]
fn disk_selects_the_longest_mount_that_contains_the_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let canonical = std::fs::canonicalize(dir.path()).unwrap();
    let root = comparable(canonical.ancestors().last().unwrap().to_path_buf());
    let nested = comparable(canonical.clone());
    let volumes = [volume(&root, 1_000, 400), volume(&nested, 500, 100)];
    let meter = disk_meter(&volumes, dir.path());
    assert_eq!(meter.state, MeterState::Ok);
    assert_eq!((meter.value, meter.total), (Some(400.0), Some(500.0)));
    assert_eq!(meter.scope.as_deref(), Some(&*nested.display().to_string()));
}

#[test]
fn disk_without_a_containing_volume_is_unavailable() {
    let data = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let canonical_other = comparable(std::fs::canonicalize(other.path()).unwrap());
    let meter = disk_meter(&[volume(&canonical_other, 10, 5)], data.path());
    assert_missing(&meter, MeterId::Disk, MeterState::Unavailable);
    assert_missing(
        &disk_meter(&[], data.path()),
        MeterId::Disk,
        MeterState::Unavailable,
    );
}

#[test]
fn disk_with_a_missing_data_directory_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("gone");
    let meter = disk_meter(&[volume(dir.path(), 10, 5)], &missing);
    assert_missing(&meter, MeterId::Disk, MeterState::Unavailable);
}

#[test]
fn disk_rejects_an_inconsistent_volume() {
    let dir = tempfile::tempdir().unwrap();
    let mount = comparable(std::fs::canonicalize(dir.path()).unwrap());
    for (total, available) in [(0, 0), (10, 11)] {
        let meter = disk_meter(&[volume(&mount, total, available)], dir.path());
        assert_missing(&meter, MeterId::Disk, MeterState::Unavailable);
    }
}

#[test]
fn gpu_without_devices_is_one_unavailable_meter() {
    let meters = gpu_meters(&[]);
    assert_eq!(meters.len(), 1);
    assert_missing(&meters[0], MeterId::Gpu, MeterState::Unavailable);
}

#[test]
fn gpu_without_utilization_is_unavailable_not_zero() {
    let meters = gpu_meters(&[
        GpuReading {
            model: Some("Test GPU".into()),
            utilization: None,
        },
        GpuReading {
            model: None,
            utilization: Some(f32::NAN),
        },
        GpuReading {
            model: None,
            utilization: Some(0.0),
        },
        GpuReading {
            model: None,
            utilization: Some(37.5),
        },
    ]);
    assert_missing(&meters[0], MeterId::Gpu, MeterState::Unavailable);
    assert_eq!(meters[0].scope.as_deref(), Some("Test GPU"));
    assert_missing(&meters[1], MeterId::Gpu, MeterState::Unavailable);
    assert_eq!(meters[1].scope.as_deref(), Some("GPU 1"));
    assert_eq!(
        (meters[2].state, meters[2].value),
        (MeterState::Ok, Some(0.0))
    );
    assert_eq!(meters[3].value, Some(37.5));
}

#[test]
fn verbatim_prefixes_are_removed_for_comparison() {
    assert_eq!(strip_verbatim(r"\\?\C:\Users\a"), r"C:\Users\a");
    assert_eq!(strip_verbatim(r"\\?\UNC\srv\share\a"), r"\\srv\share\a");
    assert_eq!(strip_verbatim(r"C:\Users\a"), r"C:\Users\a");
    assert_eq!(strip_verbatim("/home/a"), "/home/a");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn gpu_is_unsupported_without_a_vetted_source() {
    let meter = gpu_unsupported();
    assert_missing(&meter, MeterId::Gpu, MeterState::Unsupported);
}

/// Reads the real host. Asserts structure and plausible ranges, not exact
/// values, which differ per machine and per instant.
#[test]
fn the_live_probe_reports_every_resource_without_inventing_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut probe = SysinfoProbe::new(dir.path());

    let first = probe.sample();
    let cpu = first
        .iter()
        .find(|meter| meter.meter == MeterId::Cpu)
        .unwrap();
    assert_missing(cpu, MeterId::Cpu, MeterState::Unavailable);

    thread::sleep(Duration::from_millis(400));
    let second = probe.sample();
    for id in [MeterId::Cpu, MeterId::Memory, MeterId::Disk] {
        let meter = second.iter().find(|meter| meter.meter == id).unwrap();
        assert_eq!(meter.state, MeterState::Ok, "{id:?}: {:?}", meter.reason);
        let value = meter.value.unwrap();
        assert!(
            (0.0..=100.0).contains(&value) || id != MeterId::Cpu,
            "{id:?}"
        );
        if let Some(total) = meter.total {
            assert!(total > 0.0 && value <= total, "{id:?}");
        }
    }
    let gpus: Vec<_> = second
        .iter()
        .filter(|meter| meter.meter == MeterId::Gpu)
        .collect();
    assert!(!gpus.is_empty());
    for gpu in gpus {
        if gpu.state != MeterState::Ok {
            assert_missing(gpu, MeterId::Gpu, gpu.state);
        }
    }
}
