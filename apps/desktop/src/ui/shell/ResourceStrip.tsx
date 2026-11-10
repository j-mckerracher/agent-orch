import { useEffect, useState } from "react";
import type { Measured, MeterValue, SurfaceResult, SystemMetrics, SystemMetricsSource } from "../../sources/surfaces/types";
import type { Provenance } from "../../sources/types";
import { useServices, useSurface, type SurfaceState } from "../services";
import { Disclosure, Meter } from "../theme";
import { PlaceholderBadge } from "../widgets";
import "./ResourceStrip.css";

/**
 * Bottom resource strip (OX-A9, M7-17): CPU, memory, GPU and disk of the device, read from
 * the D13 `SystemMetricsSource`.
 *
 * Honesty rules, enforced here and covered by `ResourceStrip.test.tsx`:
 * - A meter (`role="meter"`) exists only for a measured percentage in 0-100. `unavailable`,
 *   `unsupported`, `unknown` and loading are written out as text and never drawn as 0 or as
 *   a healthy low value.
 * - A measured value in another unit (for example GiB of memory) has no reported total, so
 *   it is shown as number and unit without a gauge position rather than being scaled
 *   against an invented maximum.
 * - The green-to-red scale is a redundant, display-only signal. It does not claim an alert
 *   threshold; the number, unit and scope are always printed beside it.
 * - Values from the fixture adapter carry the PLACEHOLDER DATA badge (also while collapsed).
 *
 * - Only CPU and disk (and any other 0-100% value) are `role="meter"`. ARIA requires a meter
 *   to carry a range. Memory is measured in absolute units with no reported total, so a
 *   range would be invented, and unavailable/unsupported meters (GPU) have no value at all.
 *   Those rows are `role="group"` with the same name, value, scope and age as visible text.
 *
 * The strip renders inside the shell's `<section aria-label="Device resources">`, so it adds
 * no landmark of its own. The shell lays that region out in normal flow below the main
 * content at every width (it is never sticky, so an open strip cannot cover content on a
 * narrow window), which is what keeps content, composers and dialogs from covering it.
 */

type MeterKey = "cpu" | "memory" | "gpu" | "disk";

/** `name` is the short visible label; `accessibleName` states what is measured. */
const METERS: ReadonlyArray<{ key: MeterKey; name: string; accessibleName: string }> = [
  { key: "cpu", name: "CPU", accessibleName: "CPU utilization" },
  { key: "memory", name: "Memory", accessibleName: "Memory capacity used" },
  { key: "gpu", name: "GPU", accessibleName: "GPU utilization" },
  { key: "disk", name: "Disk", accessibleName: "Disk capacity used" },
];

/** What one meter can show. `gauge` is the only state that draws a scale position. */
type RowState =
  | { kind: "gauge"; level: number; text: string; scope: string; observedAt: string; ageMs: number }
  | { kind: "value"; text: string; scope: string; observedAt: string; ageMs: number }
  | { kind: "loading" }
  | { kind: "unavailable" | "unsupported" | "unknown"; reason: string };

function formatNumber(value: number): string {
  return String(Number(value.toFixed(1)));
}

function formatValue(meter: MeterValue): string {
  const unit = meter.unit.trim();
  return unit === "%" || unit === "" ? `${formatNumber(meter.value)}${unit}` : `${formatNumber(meter.value)} ${unit}`;
}

function rowState(measured: Measured<MeterValue>): RowState {
  if (measured.status !== "ok") return { kind: measured.status, reason: measured.reason };
  const { value } = measured;
  const common = { scope: value.label, observedAt: measured.observedAt, ageMs: measured.ageMs };
  if (!Number.isFinite(value.value)) {
    return { kind: "unknown", reason: "The source reported a value that is not a number." };
  }
  if (value.unit.trim() !== "%") return { kind: "value", text: formatValue(value), ...common };
  if (value.value < 0 || value.value > 100) {
    return { kind: "unknown", reason: `The source reported ${formatValue(value)}, outside 0-100%.` };
  }
  return { kind: "gauge", level: value.value, text: formatValue(value), ...common };
}

function rowsFor(state: SurfaceState<SystemMetrics>): Array<{ key: MeterKey; row: RowState }> {
  return METERS.map(({ key }) => {
    if (state.status === "loading") return { key, row: { kind: "loading" } };
    if (state.status === "ok") return { key, row: rowState(state.data[key]) };
    return { key, row: { kind: state.status, reason: state.reason } };
  });
}

/** Age relative to `nowMs`; falls back to the age the source reported if the timestamp is unusable. */
export function formatAge(observedAt: string, reportedAgeMs: number, nowMs: number): string {
  const observed = Date.parse(observedAt);
  const ageMs = Number.isFinite(observed) ? Math.max(0, nowMs - observed) : reportedAgeMs;
  const seconds = Math.round(ageMs / 1000);
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} h ago` : `${Math.floor(hours / 24)} d ago`;
}

const STATE_TEXT = { loading: "loading…", unavailable: "unavailable", unsupported: "unsupported", unknown: "unknown" } as const;

function compactText(rows: Array<{ key: MeterKey; row: RowState }>): string {
  return rows
    .map(({ key, row }) => {
      const name = METERS.find((meter) => meter.key === key)!.name;
      return `${name} ${row.kind === "gauge" || row.kind === "value" ? row.text : STATE_TEXT[row.kind]}`;
    })
    .join(" · ");
}

export interface ResourceStripViewProps {
  state: SurfaceState<SystemMetrics>;
  /** Provenance of the source that produced `state`; a fixture provenance shows the PLACEHOLDER badge. */
  provenance: Provenance;
  /** Current time, for the observation age. */
  nowMs: number;
  defaultOpen?: boolean;
}

/** Presentational strip; `ResourceStrip` wires it to the D13 source. */
export function ResourceStripView({ state, provenance, nowMs, defaultOpen = false }: ResourceStripViewProps) {
  const rows = rowsFor(state);
  const effective = state.status === "loading" || state.status === "ok" ? provenance : state.provenance;
  return (
    <Disclosure
      className="rs-strip"
      data-testid="resource-strip"
      defaultOpen={defaultOpen}
      summary={
        <span className="rs-summary">
          <span className="rs-title">Device resources</span>
          {effective.kind === "fixture" && <PlaceholderBadge provenance={effective} />}
        </span>
      }
      count={<span data-testid="resource-strip-compact">{compactText(rows)}</span>}
    >
      <div className="rs-list">
        {rows.map(({ key, row }) => {
          const meter = METERS.find((item) => item.key === key)!;
          return <ResourceItem key={key} name={meter.name} accessibleName={meter.accessibleName} row={row} nowMs={nowMs} />;
        })}
      </div>
      <p className="rs-note">
        Scale colours (green to red) are a display aid, not an alert threshold. Disk is capacity used, not activity.
      </p>
    </Disclosure>
  );
}

function ResourceItem({ name, accessibleName, row, nowMs }: { name: string; accessibleName: string; row: RowState; nowMs: number }) {
  if (row.kind === "gauge") {
    const age = formatAge(row.observedAt, row.ageMs, nowMs);
    return (
      <div className="rs-item">
        <Meter
          label={name}
          aria-label={accessibleName}
          value={row.level}
          valueText={row.text}
          aria-valuetext={`${row.text}, scope ${row.scope}, observed ${age}`}
        />
        <p className="rs-meta">
          Scope: {row.scope} · observed <time dateTime={row.observedAt}>{age}</time>
        </p>
      </div>
    );
  }
  const value = row.kind === "value" ? row.text : STATE_TEXT[row.kind];
  return (
    <div className="rs-item" role="group" aria-label={accessibleName} data-state={row.kind === "value" ? "ok" : row.kind}>
      <div className="ui-meter ui-meter-unavailable">
        <span className="ui-meter-label">{name}</span>
        <span className="ui-meter-track" aria-hidden="true" />
        <span className="ui-meter-value">{value}</span>
      </div>
      <p className="rs-meta">
        {row.kind === "value" ? (
          <>
            Scope: {row.scope} · observed <time dateTime={row.observedAt}>{formatAge(row.observedAt, row.ageMs, nowMs)}</time> · no
            total reported, so no scale is drawn
          </>
        ) : row.kind === "loading" ? (
          "Waiting for the metrics source."
        ) : (
          row.reason
        )}
      </p>
    </div>
  );
}

const AGE_TICK_MS = 5000;

export interface ResourceStripProps {
  /** Overrides the D13 source from `useServices().surfaces.metrics`. */
  source?: SystemMetricsSource;
  defaultOpen?: boolean;
  /** Clock for the observation age; defaults to `Date.now`. */
  now?: () => number;
}

/**
 * Shell `resources` slot. A source with `watch` (the live adapter) is refreshed by that
 * source at its own interval and not at all while the window is hidden; any other source is
 * read once per connection generation. Either way the shown observation age keeps moving, so a
 * snapshot that stops refreshing visibly ages instead of looking current.
 */
export function ResourceStrip({ source, defaultOpen, now = Date.now }: ResourceStripProps) {
  const services = useServices();
  const metrics = source ?? services.surfaces.metrics;
  return metrics.watch ? (
    <WatchedStrip metrics={metrics} defaultOpen={defaultOpen} now={now} />
  ) : (
    <ReadOnceStrip metrics={metrics} defaultOpen={defaultOpen} now={now} />
  );
}

interface StripBodyProps {
  metrics: SystemMetricsSource;
  defaultOpen?: boolean;
  now: () => number;
}

function ReadOnceStrip(props: StripBodyProps) {
  const state = useSurface(() => props.metrics.metrics(), "resource-strip", props.metrics.provenance);
  return <AgingStrip {...props} state={state} />;
}

function WatchedStrip(props: StripBodyProps) {
  const [watched, setWatched] = useState<{ source: SystemMetricsSource; result: SurfaceResult<SystemMetrics> } | null>(null);
  useEffect(() => props.metrics.watch?.((result) => setWatched({ source: props.metrics, result })), [props.metrics]);
  const state: SurfaceState<SystemMetrics> = watched && watched.source === props.metrics ? watched.result : { status: "loading" };
  return <AgingStrip {...props} state={state} />;
}

function AgingStrip({ metrics, defaultOpen, now, state }: StripBodyProps & { state: SurfaceState<SystemMetrics> }) {
  const [nowMs, setNowMs] = useState(now);
  useEffect(() => {
    setNowMs(now());
    const timer = setInterval(() => setNowMs(now()), AGE_TICK_MS);
    return () => clearInterval(timer);
  }, [now, state]);
  return <ResourceStripView state={state} provenance={metrics.provenance} nowMs={nowMs} defaultOpen={defaultOpen} />;
}
