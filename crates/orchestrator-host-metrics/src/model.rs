//! Wire types of the `host.metrics` result.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;

/// Which resource a meter describes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterId {
    Cpu,
    Memory,
    Disk,
    Gpu,
}

/// `ok` carries a measured `value`. `unavailable` means this platform has a
/// source but it produced no usable reading now (warming up, failed, stale, no
/// device). `unsupported` means this build has no vetted source for the
/// platform. Both carry a `reason` and no `value`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterState {
    Ok,
    Unavailable,
    Unsupported,
}

/// `utilization` is a share of busy time (`unit` percent, 0 to 100).
/// `capacity` is an amount in use out of `total` (`unit` bytes).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterKind {
    Utilization,
    Capacity,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Bytes,
}

/// One meter. Fields are always present; absent data is `null`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Meter {
    pub meter: MeterId,
    pub state: MeterState,
    pub kind: MeterKind,
    pub unit: Unit,
    /// Device or volume this reading covers, for example `all logical CPUs`,
    /// a volume mount point or a GPU model. Never a path inside the data
    /// directory.
    pub scope: Option<String>,
    /// Measured amount in `unit`. Present exactly when `state` is `ok`.
    pub value: Option<f64>,
    /// Capacity of `scope` in `unit`; set for `capacity` meters when `ok`.
    pub total: Option<f64>,
    /// RFC 3339 UTC time (millisecond precision) at which the sampler last
    /// attempted this meter; `null` before the first sample completes.
    pub observed_at: Option<String>,
    /// Milliseconds between samples.
    pub sample_interval_ms: u64,
    /// Human-readable cause. Present exactly when `state` is not `ok`.
    pub reason: Option<String>,
}

/// Result of `host.metrics`.
///
/// `meters` always holds one `cpu`, one `memory` and one `disk` entry. `gpu`
/// has one entry per detected device, or a single `unavailable`/`unsupported`
/// entry when there is none.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub sample_interval_ms: u64,
    pub meters: Vec<Meter>,
}

impl Meter {
    pub(crate) fn ok(
        meter: MeterId,
        kind: MeterKind,
        unit: Unit,
        scope: impl Into<String>,
        value: f64,
        total: Option<f64>,
    ) -> Self {
        Self {
            meter,
            state: MeterState::Ok,
            kind,
            unit,
            scope: Some(scope.into()),
            value: Some(value),
            total,
            observed_at: None,
            sample_interval_ms: 0,
            reason: None,
        }
    }

    pub(crate) fn missing(
        meter: MeterId,
        state: MeterState,
        kind: MeterKind,
        unit: Unit,
        scope: Option<String>,
        reason: impl Into<String>,
    ) -> Self {
        debug_assert!(state != MeterState::Ok);
        Self {
            meter,
            state,
            kind,
            unit,
            scope,
            value: None,
            total: None,
            observed_at: None,
            sample_interval_ms: 0,
            reason: Some(reason.into()),
        }
    }

    pub(crate) fn unavailable(
        meter: MeterId,
        kind: MeterKind,
        unit: Unit,
        reason: impl Into<String>,
    ) -> Self {
        Self::missing(meter, MeterState::Unavailable, kind, unit, None, reason)
    }
}

/// Meter kind and unit are fixed per resource.
pub(crate) const fn shape(meter: MeterId) -> (MeterKind, Unit) {
    match meter {
        MeterId::Cpu | MeterId::Gpu => (MeterKind::Utilization, Unit::Percent),
        MeterId::Memory | MeterId::Disk => (MeterKind::Capacity, Unit::Bytes),
    }
}

pub(crate) const ALL_METERS: [MeterId; 4] =
    [MeterId::Cpu, MeterId::Memory, MeterId::Disk, MeterId::Gpu];

/// Formats `time` as `YYYY-MM-DDTHH:MM:SS.mmmZ`. Times before the Unix epoch
/// are clamped to it.
pub(crate) fn rfc3339_utc(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let seconds = since.as_secs();
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let in_day = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        in_day / 3_600,
        in_day % 3_600 / 60,
        in_day % 60,
        since.subsec_millis()
    )
}

/// Proleptic Gregorian date for a day count since 1970-01-01 (Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64, millis: u32) -> SystemTime {
        UNIX_EPOCH + Duration::new(seconds, millis * 1_000_000)
    }

    #[test]
    fn formats_known_instants() {
        assert_eq!(rfc3339_utc(at(0, 0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339_utc(at(951_782_400, 0)), "2000-02-29T00:00:00.000Z");
        assert_eq!(
            rfc3339_utc(at(1_000_000_000, 123)),
            "2001-09-09T01:46:40.123Z"
        );
        assert_eq!(
            rfc3339_utc(at(1_794_335_000, 7)),
            "2026-11-10T18:23:20.007Z"
        );
    }

    #[test]
    fn clamps_times_before_the_epoch() {
        assert_eq!(
            rfc3339_utc(UNIX_EPOCH - Duration::from_secs(5)),
            "1970-01-01T00:00:00.000Z"
        );
    }
}
