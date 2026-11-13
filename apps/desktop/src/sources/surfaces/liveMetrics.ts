/**
 * Live D13 adapter (M7-21): `host.metrics` (capability `host`) mapped to
 * `SystemMetricsSource`. Contract: `docs/host-metrics.md`.
 *
 * Rules:
 * - Never imports fixtures. A failed call is `unavailable`, or `unsupported`
 *   when the daemon has no `host` capability or method; it is never answered
 *   with fixture data.
 * - A meter that is not `ok` keeps its state and the daemon's reason. Missing,
 *   malformed or out-of-range readings are `unknown`. None is ever 0.
 * - GPU is passed through: the daemon reports `unsupported` where it has no
 *   vetted source, and this adapter does not turn that into a value.
 * - Capacity meters (memory, disk) become percent used with the absolute
 *   amounts in the label, because `MeterValue` has no total field.
 * - `watch` polls at the contract's `sample_interval_ms` and sends no request
 *   while the window is hidden.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError, isMissingFeature, presentError } from "../../daemon/errors";
import type { HostMeterWire, HostMetricsWire } from "../../daemon/hostMetrics";
import { measured, notOk, observation, okResult, oldest } from "./result";
import type { Measured, MeterValue, Observation, Provenance, SurfaceResult, SystemMetrics, SystemMetricsSource } from "./types";

const SCHEMA_VERSION = 1;
/** The daemon samples at most every 500 ms; polling faster only re-reads the cache. */
const MIN_POLL_MS = 500;
const DEFAULT_POLL_MS = 2000;

type Resource = "cpu" | "memory" | "disk" | "gpu";

const PROVENANCE: Provenance = { kind: "daemon", id: "D13", methods: ["host.metrics"] };

const FALLBACK_LABEL: Record<Resource, string> = { cpu: "CPU", memory: "Memory", disk: "Disk", gpu: "GPU" };

const unknown = <T>(reason: string): Measured<T> => ({ status: "unknown", reason });

const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);

function formatBytes(bytes: number): string {
  const units = ["B", "kB", "MB", "GB", "TB", "PB"];
  let value = bytes;
  let index = 0;
  while (value >= 1000 && index < units.length - 1) {
    value /= 1000;
    index += 1;
  }
  return `${Number(value.toFixed(1))} ${units[index]}`;
}

function reasonOf(meter: HostMeterWire, resource: Resource): string {
  return typeof meter.reason === "string" && meter.reason !== "" ? meter.reason : `The daemon gave no ${resource} reading and no reason.`;
}

/** Maps one meter that the daemon reported as `ok`. Anything it cannot trust is `unknown`. */
function mapOk(meter: HostMeterWire, resource: Resource, nowMs: number): Measured<MeterValue> {
  const { value, total } = meter;
  if (!finite(value) || value < 0) return unknown(`The daemon reported an unusable ${resource} value.`);
  if (typeof meter.observed_at !== "string" || !Number.isFinite(Date.parse(meter.observed_at))) {
    return unknown(`The daemon reported ${resource} without a valid observation time.`);
  }
  const observedMs = Date.parse(meter.observed_at);
  const obs: Observation = { observedAt: new Date(observedMs).toISOString(), ageMs: Math.max(0, nowMs - observedMs) };
  const label = typeof meter.scope === "string" && meter.scope !== "" ? meter.scope : FALLBACK_LABEL[resource];

  if (meter.unit === "percent") {
    if (value > 100) return unknown(`The daemon reported ${resource} at ${value}%, outside 0-100%.`);
    return measured({ value, unit: "%", kind: meter.kind === "capacity" ? "capacity" : "utilization", label }, obs);
  }
  if (meter.unit === "bytes") {
    if (!finite(total) || total <= 0) {
      return measured({ value: Number((value / 1e9).toFixed(1)), unit: "GB", kind: "capacity", label }, obs);
    }
    if (value > total) return unknown(`The daemon reported more ${resource} used than its total.`);
    return measured(
      { value: (value / total) * 100, unit: "%", kind: "capacity", label: `${label} · ${formatBytes(value)} of ${formatBytes(total)}` },
      obs,
    );
  }
  return unknown(`The daemon reported ${resource} in an unrecognised unit.`);
}

function mapMeter(meter: HostMeterWire, resource: Resource, nowMs: number): Measured<MeterValue> {
  if (meter.state === "ok") return mapOk(meter, resource, nowMs);
  if (meter.state === "unavailable" || meter.state === "unsupported") {
    return { status: meter.state, reason: reasonOf(meter, resource) };
  }
  return unknown(`The daemon reported an unrecognised ${resource} state.`);
}

/** CPU, memory and disk have one meter each; the first entry wins if a daemon repeats one. */
function mapSingle(meters: readonly HostMeterWire[], resource: Resource, nowMs: number): Measured<MeterValue> {
  const meter = meters.find((item) => item.meter === resource);
  return meter ? mapMeter(meter, resource, nowMs) : unknown(`The daemon returned no ${resource} meter.`);
}

/**
 * The UI has one GPU row; the daemon lists one meter per device. Rule: show the busiest measured
 * device and say how many were reported. With no measured device, `unavailable` wins over
 * `unsupported` (a source exists for at least one device), and both keep the daemon's reason.
 */
function mapGpu(meters: readonly HostMeterWire[], nowMs: number): Measured<MeterValue> {
  const devices = meters.filter((item) => item.meter === "gpu");
  if (devices.length === 0) return unknown("The daemon returned no gpu meter.");
  const mapped = devices.map((device) => mapMeter(device, "gpu", nowMs));
  const ok = mapped.filter((item): item is Extract<Measured<MeterValue>, { status: "ok" }> => item.status === "ok");
  if (ok.length > 0) {
    const busiest = ok.reduce((best, item) => (item.value.value > best.value.value ? item : best));
    if (devices.length === 1) return busiest;
    return { ...busiest, value: { ...busiest.value, label: `${busiest.value.label} (busiest of ${devices.length} GPUs)` } };
  }
  const failed = mapped.filter((item): item is Exclude<Measured<MeterValue>, { status: "ok" }> => item.status !== "ok");
  return failed.find((item) => item.status === "unavailable") ?? failed.find((item) => item.status === "unknown") ?? failed[0];
}

/** Maps one `host.metrics` result. Exported for tests. */
export function mapHostMetrics(wire: HostMetricsWire, nowMs: number): SurfaceResult<SystemMetrics> {
  if (typeof wire !== "object" || wire === null || !Array.isArray(wire.meters)) {
    return notOk(PROVENANCE, "unknown", "The daemon answered host.metrics with an unrecognised shape.", observation(nowMs));
  }
  if (wire.schema_version !== SCHEMA_VERSION) {
    return notOk(PROVENANCE, "unsupported", `host.metrics schema ${String(wire.schema_version)} is not understood (expected ${SCHEMA_VERSION}).`, observation(nowMs));
  }
  // A null or non-object entry is treated as a meter the daemon did not list.
  const meters = wire.meters.filter((item): item is HostMeterWire => typeof item === "object" && item !== null);
  const data: SystemMetrics = {
    cpu: mapSingle(meters, "cpu", nowMs),
    memory: mapSingle(meters, "memory", nowMs),
    disk: mapSingle(meters, "disk", nowMs),
    gpu: mapGpu(meters, nowMs),
  };
  const observed = Object.values(data).flatMap((item) => (item.status === "ok" ? [{ observedAt: item.observedAt, ageMs: item.ageMs }] : []));
  return okResult(PROVENANCE, data, oldest(observed) ?? observation(nowMs));
}

/** The contract's own interval, never below the daemon's sampling floor. */
export function pollIntervalMs(wire: unknown): number {
  const interval = (wire as Partial<HostMetricsWire> | null)?.sample_interval_ms;
  return finite(interval) && interval > 0 ? Math.max(MIN_POLL_MS, interval) : DEFAULT_POLL_MS;
}

export interface Visibility {
  hidden(): boolean;
  /** Calls `listener` whenever visibility changes; returns the unsubscribe function. */
  subscribe(listener: () => void): () => void;
}

const documentVisibility: Visibility = {
  hidden: () => typeof document !== "undefined" && document.visibilityState === "hidden",
  subscribe: (listener) => {
    if (typeof document === "undefined") return () => undefined;
    document.addEventListener("visibilitychange", listener);
    return () => document.removeEventListener("visibilitychange", listener);
  },
};

export interface LiveMetricsOptions {
  now?: () => number;
  visibility?: Visibility;
}

export function createLiveMetrics(connection: ConnectionManager, options: LiveMetricsOptions = {}): SystemMetricsSource {
  const now = options.now ?? Date.now;
  const visibility = options.visibility ?? documentVisibility;
  /** The interval from the last successful answer; kept across failures so a retry stays at the contract's pace. */
  let intervalMs = DEFAULT_POLL_MS;

  async function read(): Promise<SurfaceResult<SystemMetrics>> {
    let wire: HostMetricsWire;
    try {
      wire = await connection.api().hostMetrics();
    } catch (error) {
      const failed = asDaemonError(error);
      // Only a DaemonError message is the daemon's own words; any other error text is a JS detail and stays out of the UI.
      const detail = error instanceof DaemonError ? ` (${failed.kind}: ${failed.message})` : "";
      const reason = `${presentError(failed).reason}${detail}`;
      return notOk(PROVENANCE, isMissingFeature(failed) ? "unsupported" : "unavailable", reason, observation(now()));
    }
    intervalMs = pollIntervalMs(wire);
    try {
      return mapHostMetrics(wire, now());
    } catch {
      return notOk(PROVENANCE, "unknown", "The daemon answered host.metrics with data this app cannot read.", observation(now()));
    }
  }

  return {
    provenance: PROVENANCE,
    metrics: read,
    watch(onResult) {
      let stopped = false;
      let inFlight = false;
      let timer: ReturnType<typeof setTimeout> | null = null;

      const clear = () => {
        if (timer !== null) clearTimeout(timer);
        timer = null;
      };

      const poll = async () => {
        clear();
        if (stopped || inFlight || visibility.hidden()) return;
        inFlight = true;
        const result = await read();
        inFlight = false;
        if (stopped) return;
        onResult(result);
        if (!visibility.hidden()) timer = setTimeout(() => void poll(), intervalMs);
      };

      const onVisibility = () => {
        if (visibility.hidden()) clear();
        else void poll();
      };
      const unsubscribeVisibility = visibility.subscribe(onVisibility);
      // A new connection should not wait out the retry interval of a read that failed while offline.
      let generation = connection.getSnapshot().generation;
      const unsubscribeConnection = connection.subscribe(() => {
        const next = connection.getSnapshot();
        if (next.generation === generation) return;
        generation = next.generation;
        if (next.status === "connected") void poll();
      });
      void poll();
      return () => {
        stopped = true;
        clear();
        unsubscribeVisibility();
        unsubscribeConnection();
      };
    },
  };
}
