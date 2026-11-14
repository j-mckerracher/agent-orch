//! Reads the operating system through the `sysinfo` crate.
//!
//! Every reading is built by a pure function over plain numbers
//! ([`cpu_meter`], [`memory_meter`], [`disk_meter`], [`gpu_meters`]) so the
//! unavailable and error paths are tested without touching the host.

use crate::model::{Meter, MeterId, MeterKind, MeterState, Unit};
use crate::service::Probe;
use std::path::{Path, PathBuf};
use sysinfo::{Disks, System};

/// A mounted volume as the operating system reports it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Volume {
    pub(crate) mount: PathBuf,
    pub(crate) total: u64,
    pub(crate) available: u64,
}

/// One GPU as the operating system reports it.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuReading {
    pub(crate) model: Option<String>,
    pub(crate) utilization: Option<f32>,
}

/// Samples the host that runs the daemon. `data_dir` selects the volume the
/// disk meter describes.
pub struct SysinfoProbe {
    data_dir: PathBuf,
    system: Result<System, String>,
    disks: Result<Disks, String>,
    cpu_baseline_taken: bool,
    #[cfg(target_os = "macos")]
    gpus: Result<sysinfo::Gpus, String>,
}

impl SysinfoProbe {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            system: System::new().map_err(|error| error.to_string()),
            disks: Disks::new().map_err(|error| error.to_string()),
            cpu_baseline_taken: false,
            #[cfg(target_os = "macos")]
            gpus: sysinfo::Gpus::new().map_err(|error| error.to_string()),
        }
    }
}

impl Probe for SysinfoProbe {
    fn sample(&mut self) -> Vec<Meter> {
        let mut meters = Vec::with_capacity(4);
        match &mut self.system {
            Ok(system) => {
                system.refresh_cpu_usage();
                // Utilization is the change since the previous refresh, so the
                // first refresh only sets the baseline.
                let cpu = self.cpu_baseline_taken.then(|| system.global_cpu_usage());
                self.cpu_baseline_taken = true;
                meters.push(cpu_meter(cpu));
                system.refresh_memory();
                meters.push(memory_meter(system.total_memory(), system.used_memory()));
            }
            Err(error) => {
                let reason = format!("system information source failed: {error}");
                for id in [MeterId::Cpu, MeterId::Memory] {
                    meters.push(failed(id, &reason));
                }
            }
        }
        meters.push(match &mut self.disks {
            Ok(disks) => {
                disks.refresh(true);
                let volumes: Vec<Volume> = disks
                    .iter()
                    .map(|disk| Volume {
                        mount: disk.mount_point().to_path_buf(),
                        total: disk.total_space(),
                        available: disk.available_space(),
                    })
                    .collect();
                disk_meter(&volumes, &self.data_dir)
            }
            Err(error) => failed(
                MeterId::Disk,
                &format!("disk information source failed: {error}"),
            ),
        });
        meters.extend(self.gpu_meters());
        meters
    }
}

impl SysinfoProbe {
    #[cfg(target_os = "macos")]
    fn gpu_meters(&mut self) -> Vec<Meter> {
        match &mut self.gpus {
            Ok(gpus) => {
                gpus.refresh(true);
                let readings: Vec<GpuReading> = gpus
                    .iter()
                    .map(|gpu| GpuReading {
                        model: gpu.model().map(str::to_owned),
                        utilization: gpu.usage(),
                    })
                    .collect();
                gpu_meters(&readings)
            }
            Err(error) => vec![failed(
                MeterId::Gpu,
                &format!("GPU information source failed: {error}"),
            )],
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn gpu_meters(&mut self) -> Vec<Meter> {
        vec![gpu_unsupported()]
    }
}

fn failed(meter: MeterId, reason: &str) -> Meter {
    let (kind, unit) = crate::model::shape(meter);
    Meter::unavailable(meter, kind, unit, reason)
}

#[cfg(not(target_os = "macos"))]
fn gpu_unsupported() -> Meter {
    Meter::missing(
        MeterId::Gpu,
        MeterState::Unsupported,
        MeterKind::Utilization,
        Unit::Percent,
        None,
        "no vetted GPU utilization source for this platform",
    )
}

/// `percent` is `None` until a second CPU refresh exists.
pub(crate) fn cpu_meter(percent: Option<f32>) -> Meter {
    match percent {
        Some(percent) if percent.is_finite() => Meter::ok(
            MeterId::Cpu,
            MeterKind::Utilization,
            Unit::Percent,
            "all logical CPUs",
            f64::from(percent).clamp(0.0, 100.0),
            None,
        ),
        Some(_) => failed(MeterId::Cpu, "the CPU source returned a non-finite value"),
        None => failed(
            MeterId::Cpu,
            "CPU utilization needs two samples; the first is still pending",
        ),
    }
}

pub(crate) fn memory_meter(total: u64, used: u64) -> Meter {
    if total == 0 {
        return failed(
            MeterId::Memory,
            "the memory source reported zero total memory",
        );
    }
    if used > total {
        return failed(
            MeterId::Memory,
            "the memory source reported more used than total memory",
        );
    }
    Meter::ok(
        MeterId::Memory,
        MeterKind::Capacity,
        Unit::Bytes,
        "physical memory",
        used as f64,
        Some(total as f64),
    )
}

/// Describes the volume that holds `data_dir`: the one with the longest mount
/// point that contains it. Used is total minus the space available to this
/// user, matching `df`.
pub(crate) fn disk_meter(volumes: &[Volume], data_dir: &Path) -> Meter {
    let resolved = match std::fs::canonicalize(data_dir) {
        Ok(path) => comparable(path),
        Err(error) => {
            return failed(
                MeterId::Disk,
                &format!("cannot resolve the data directory: {}", error.kind()),
            )
        }
    };
    let volume = volumes
        .iter()
        .filter(|volume| resolved.starts_with(comparable(volume.mount.clone())))
        .max_by_key(|volume| volume.mount.components().count());
    let Some(volume) = volume else {
        return failed(
            MeterId::Disk,
            "no mounted volume contains the data directory",
        );
    };
    if volume.total == 0 || volume.available > volume.total {
        return failed(
            MeterId::Disk,
            "the volume reported an inconsistent capacity",
        );
    }
    Meter::ok(
        MeterId::Disk,
        MeterKind::Capacity,
        Unit::Bytes,
        volume.mount.display().to_string(),
        (volume.total - volume.available) as f64,
        Some(volume.total as f64),
    )
}

/// One utilization meter per device. A device that reports no utilization is
/// `unavailable`, never 0. No device at all is a single `unavailable` meter.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn gpu_meters(devices: &[GpuReading]) -> Vec<Meter> {
    if devices.is_empty() {
        return vec![failed(MeterId::Gpu, "the system reported no GPU")];
    }
    devices
        .iter()
        .enumerate()
        .map(|(index, device)| {
            let scope = device
                .model
                .clone()
                .unwrap_or_else(|| format!("GPU {index}"));
            match device.utilization {
                Some(percent) if percent.is_finite() => Meter::ok(
                    MeterId::Gpu,
                    MeterKind::Utilization,
                    Unit::Percent,
                    scope,
                    f64::from(percent).clamp(0.0, 100.0),
                    None,
                ),
                _ => Meter::missing(
                    MeterId::Gpu,
                    MeterState::Unavailable,
                    MeterKind::Utilization,
                    Unit::Percent,
                    Some(scope),
                    "the GPU did not report utilization",
                ),
            }
        })
        .collect()
}

/// `std::fs::canonicalize` returns `\\?\C:\...` on Windows while volumes are
/// listed as `C:\`; the two never compare as prefixes of each other.
#[cfg(windows)]
fn comparable(path: PathBuf) -> PathBuf {
    PathBuf::from(strip_verbatim(&path.to_string_lossy()))
}

#[cfg(not(windows))]
fn comparable(path: PathBuf) -> PathBuf {
    path
}

#[cfg(any(windows, test))]
fn strip_verbatim(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(local) = path.strip_prefix(r"\\?\") {
        local.to_owned()
    } else {
        path.to_owned()
    }
}

#[cfg(test)]
mod tests;
