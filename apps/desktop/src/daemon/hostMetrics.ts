/**
 * Wire types of `host.metrics` (M7-04; capability `host`). Mirrors
 * `crates/orchestrator-host-metrics/src/model.rs` and `docs/host-metrics.md`.
 * Every field is always present; absent data is `null`. Enum values stay
 * `string` so a variant added later is handled as unknown instead of coerced.
 */
export interface HostMeterWire {
  /** `cpu`, `memory`, `disk` or `gpu`. */
  meter: string;
  /** `ok`, `unavailable` or `unsupported`. */
  state: string;
  /** `utilization` or `capacity`. */
  kind: string;
  /** `percent` or `bytes`. */
  unit: string;
  /** Device, volume or CPU scope the reading covers. */
  scope: string | null;
  /** Present exactly when `state` is `ok`. */
  value: number | null;
  /** Capacity of `scope` in `unit`; set for `capacity` meters when `ok`. */
  total: number | null;
  /** RFC 3339 UTC time of the sampler's last attempt; null before the first sample. */
  observed_at: string | null;
  sample_interval_ms: number;
  /** Present exactly when `state` is not `ok`. */
  reason: string | null;
}

export interface HostMetricsWire {
  schema_version: number;
  sample_interval_ms: number;
  meters: HostMeterWire[];
}
