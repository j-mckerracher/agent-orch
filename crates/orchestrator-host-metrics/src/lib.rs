//! Read-only host telemetry: CPU, memory, disk and GPU meters.
//!
//! The crate is independent of the daemon, IPC and store. [`HostMetrics`] owns
//! one sampler thread that refreshes a cached [`Snapshot`] every
//! [`HostMetrics::interval`]; [`HostMetrics::snapshot`] only copies that cache,
//! so a request never waits for the operating system.
//!
//! A meter is `ok` only with a measured value. A source that is missing,
//! failed, not yet warmed up or stale is reported as `unavailable` (or
//! `unsupported` when the platform has no vetted source) with a `reason`. A
//! missing reading is never reported as zero.
//!
//! The wire contract is documented on [`Snapshot`] and in `docs/host-metrics.md`.

mod model;
mod probe;
mod service;

pub use model::{Meter, MeterId, MeterKind, MeterState, Snapshot, Unit, SCHEMA_VERSION};
pub use probe::SysinfoProbe;
pub use service::{HostMetrics, Probe, DEFAULT_INTERVAL, MIN_INTERVAL, STALE_AFTER_INTERVALS};
