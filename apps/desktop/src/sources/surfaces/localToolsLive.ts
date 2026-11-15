/**
 * Live adapter for other AI tools' usage: `usage.localTools`, `usage.localTools.setEnabled` and
 * `usage.localTools.refresh` (capability `agents`), mapped onto `LocalToolsSource`.
 *
 * These figures are estimates read from Claude Code, Codex and GitHub Copilot session files on this
 * machine. They are kept apart from `usageLive.ts`, so they never enter the reported or estimated totals.
 * Honesty rules:
 * - The parse is strict. An unknown reader state, provider, or currency, a wrong type, or a missing
 *   field is a protocol failure. No value is guessed.
 * - Unknown is never zero. A cost with no price is `null`, and a provider with no calls is `no_calls`.
 * - Costs are USD decimal strings. A cost in any other currency is refused, never converted.
 * - This file never imports fixtures, so live mode cannot fall back to placeholder figures.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError } from "../../daemon/errors";
import type {
  LocalToolsProviderIdWire,
  LocalToolsProviderWire,
  LocalToolsReaderStateWire,
  LocalToolsSnapshotWire,
  LocalToolsWindowRequest,
} from "../../daemon/usageWire";
import { observation, okResult } from "./result";
import { failure, hostLocalDay, type LocalDay } from "./usageLive";
import type { LocalToolsProvider, LocalToolsSnapshot, LocalToolsSource, LocalToolsWindow, Provenance, SurfaceResult } from "./types";

const READER_STATES: readonly LocalToolsReaderStateWire[] = [
  "off",
  "not_started",
  "ok",
  "not_installed",
  "node_missing",
  "node_too_old",
  "unsupported_version",
  "integrity_mismatch",
  "timeout",
  "failed",
  "bad_output",
  "output_too_large",
  "wrong_day",
  "not_bundled",
];
const READER_SOURCES: readonly ("bundled" | "override" | "path")[] = ["bundled", "override", "path"];
const PROVIDER_STATES: readonly LocalToolsProviderWire["state"][] = ["ok", "no_calls", "not_run"];
/** The daemon always sends these three, in this order. */
const PROVIDERS: ReadonlyArray<{ id: LocalToolsProviderIdWire; label: string }> = [
  { id: "claude-code", label: "Claude Code" },
  { id: "codex", label: "Codex" },
  { id: "github-copilot", label: "GitHub Copilot" },
];
const CURRENCY = "USD";
const DECIMAL = /^\d+(\.\d+)?$/;

function malformed(what: string): never {
  throw new DaemonError("protocol", `usage.localTools returned a malformed ${what}`);
}

function record(value: unknown, what: string): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : malformed(what);
}

function count(value: unknown, what: string): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : malformed(what);
}

function nullableCount(value: unknown, what: string): number | null {
  return value === null ? null : count(value, what);
}

function integer(value: unknown, what: string): number {
  return typeof value === "number" && Number.isSafeInteger(value) ? value : malformed(what);
}

function nullableInteger(value: unknown, what: string): number | null {
  return value === null ? null : integer(value, what);
}

function text(value: unknown, what: string): string {
  return typeof value === "string" && value.length > 0 ? value : malformed(what);
}

function nullableText(value: unknown, what: string): string | null {
  return value === null ? null : text(value, what);
}

function flag(value: unknown, what: string): boolean {
  return typeof value === "boolean" ? value : malformed(what);
}

function oneOf<T extends string>(value: unknown, options: readonly T[], what: string): T {
  return typeof value === "string" && (options as readonly string[]).includes(value) ? (value as T) : malformed(what);
}

function nullableOneOf<T extends string>(value: unknown, options: readonly T[], what: string): T | null {
  return value === null ? null : oneOf(value, options, what);
}

/** A window the daemon echoes or reports: it must cover some time, so an empty or reversed one is refused. */
function span(value: unknown, what: string): { start_unix_ms: number; end_unix_ms: number; timezone: string } {
  const raw = record(value, what);
  const start = count(raw.start_unix_ms, `${what} start`);
  const end = count(raw.end_unix_ms, `${what} end`);
  if (end <= start) malformed(`${what} span`);
  return { start_unix_ms: start, end_unix_ms: end, timezone: text(raw.timezone, `${what} timezone`) };
}

function provider(value: unknown, index: number): LocalToolsProviderWire {
  const raw = record(value, "provider");
  const expected = PROVIDERS[index]!;
  if (raw.id !== expected.id) malformed("provider order");
  const tokens = record(raw.tokens, "provider tokens");
  if (!Array.isArray(raw.estimated_costs)) malformed("provider estimated_costs");
  const costs = raw.estimated_costs.map((entry) => {
    const cost = record(entry, "provider cost");
    if (cost.currency !== CURRENCY || typeof cost.amount !== "string" || !DECIMAL.test(cost.amount)) malformed("provider cost");
    return { currency: CURRENCY, amount: cost.amount };
  });
  // One USD figure per provider; a repeat would have to be added, and the daemon never sends one.
  if (costs.length > 1) malformed("provider cost list");
  return {
    id: expected.id,
    state: oneOf(raw.state, PROVIDER_STATES, "provider state"),
    sessions: nullableCount(raw.sessions, "provider sessions"),
    calls: nullableCount(raw.calls, "provider calls"),
    tokens: {
      input: nullableCount(tokens.input, "provider input tokens"),
      output: nullableCount(tokens.output, "provider output tokens"),
      cache_read: nullableCount(tokens.cache_read, "provider cache_read tokens"),
      cache_write: nullableCount(tokens.cache_write, "provider cache_write tokens"),
    },
    estimated_costs: costs,
    unpriced_models: count(raw.unpriced_models, "provider unpriced_models"),
    unpriced_tokens: count(raw.unpriced_tokens, "provider unpriced_tokens"),
    cost_complete: flag(raw.cost_complete, "provider cost_complete"),
  };
}

/** Validates the untyped `usage.localTools` result. Anything unexpected is a protocol failure, never a guessed value. */
export function parseLocalTools(value: unknown): LocalToolsSnapshotWire {
  const raw = record(value, "result");
  if (raw.schema_version !== 1) malformed("schema_version");
  const window = record(raw.window, "window");
  const reader = record(raw.reader, "reader");
  if (!Array.isArray(raw.providers) || raw.providers.length !== PROVIDERS.length) malformed("providers");
  const dataWindow = raw.data_window === null ? null : span(raw.data_window, "data_window");
  const observedAt = nullableCount(raw.observed_at_unix_ms, "observed_at_unix_ms");
  // Figures without the time they were read cannot be shown with an honest age.
  if (dataWindow !== null && observedAt === null) malformed("figures without observed_at_unix_ms");
  return {
    schema_version: 1,
    enabled: flag(raw.enabled, "enabled"),
    window: {
      ...span(window, "window"),
      utc_offset_seconds: integer(window.utc_offset_seconds, "window utc_offset_seconds"),
    },
    data_window: dataWindow,
    reader: {
      state: oneOf<LocalToolsReaderStateWire>(reader.state, READER_STATES, "reader state"),
      version: nullableText(reader.version, "reader version"),
      pinned_version: text(reader.pinned_version, "reader pinned_version"),
      node_version: nullableText(reader.node_version, "reader node_version"),
      exit_status: nullableInteger(reader.exit_status, "reader exit_status"),
      source: nullableOneOf(reader.source, READER_SOURCES, "reader source"),
      development_build: flag(reader.development_build, "reader development_build"),
    },
    refreshing: flag(raw.refreshing, "refreshing"),
    observed_at_unix_ms: observedAt,
    stale: flag(raw.stale, "stale"),
    providers: raw.providers.map((entry, index) => provider(entry, index)),
  };
}

function mapProvider(wire: LocalToolsProviderWire): LocalToolsProvider {
  const label = PROVIDERS.find((known) => known.id === wire.id)!.label;
  const [cost] = wire.estimated_costs;
  return {
    id: wire.id,
    label,
    state: wire.state,
    sessions: wire.sessions,
    calls: wire.calls,
    tokens: {
      input: wire.tokens.input,
      output: wire.tokens.output,
      cacheRead: wire.tokens.cache_read,
      cacheWrite: wire.tokens.cache_write,
    },
    estimatedUsd: cost ? Number(cost.amount) : null,
    unpricedModels: wire.unpriced_models,
    unpricedTokens: wire.unpriced_tokens,
    costComplete: wire.cost_complete,
  };
}

const windowOf = (value: { start_unix_ms: number; end_unix_ms: number; timezone: string }): LocalToolsWindow => ({
  startMs: value.start_unix_ms,
  endMs: value.end_unix_ms,
  timezone: value.timezone,
});

export function mapLocalTools(wire: LocalToolsSnapshotWire): LocalToolsSnapshot {
  return {
    enabled: wire.enabled,
    window: windowOf(wire.window),
    figures: wire.data_window ? windowOf(wire.data_window) : null,
    reader: {
      state: wire.reader.state,
      version: wire.reader.version,
      pinnedVersion: wire.reader.pinned_version,
      nodeVersion: wire.reader.node_version,
      exitStatus: wire.reader.exit_status,
      source: wire.reader.source,
    },
    refreshing: wire.refreshing,
    observedAtMs: wire.observed_at_unix_ms,
    stale: wire.stale,
    providers: wire.providers.map(mapProvider),
  };
}

/**
 * The live D12 local-tools source. Every call names the host's local day, the same day the usage
 * pill reports, so the daemon's cache and the pill agree at midnight.
 */
export function createLiveLocalToolsSource(
  connection: ConnectionManager,
  now: () => number = Date.now,
  localDay: (nowMs: number) => LocalDay = hostLocalDay,
): LocalToolsSource {
  const provenance: Provenance = {
    kind: "daemon",
    id: "D12",
    methods: ["usage.localTools", "usage.localTools.setEnabled", "usage.localTools.refresh"],
  };

  async function read(call: (request: LocalToolsWindowRequest) => Promise<unknown>): Promise<SurfaceResult<LocalToolsSnapshot>> {
    try {
      const at = now();
      const day = localDay(at);
      const request: LocalToolsWindowRequest = {
        timezone: day.timezone,
        utc_offset_seconds: day.utcOffsetSeconds,
        window_start_unix_ms: day.startMs,
        window_end_unix_ms: day.endMs,
      };
      return okResult(provenance, mapLocalTools(parseLocalTools(await call(request))), observation(at));
    } catch (error) {
      return failure<LocalToolsSnapshot>(provenance, error, now);
    }
  }

  return {
    provenance,
    local: () => read((request) => connection.api().usageLocalTools(request)),
    setEnabled: (enabled) => read((request) => connection.api().usageLocalToolsSetEnabled({ ...request, enabled })),
    refresh: () => read((request) => connection.api().usageLocalToolsRefresh(request)),
  };
}
