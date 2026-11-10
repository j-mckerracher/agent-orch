/**
 * Live D12 adapter: `usage.summary` (capability `agents`) mapped onto `UsageSource`.
 *
 * Honesty rules (docs/usage-ledger.md, GAP-ANALYSIS D-11):
 * - Unknown is never zero. A bucket with no reported cost is an `unknown` Measured, and a
 *   provider that reported nothing is `none`, never a $0 line.
 * - Costs are decimal strings per currency. Currencies are never added together; a figure
 *   that spans several currencies is `unsupported` here and its amounts go in the reason.
 * - Reported and estimated figures stay separate. Nothing is derived from tokens.
 * - A failed or unsupported call is reported as such. This file never imports fixtures, so
 *   live mode cannot fall back to placeholder numbers.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError, presentError } from "../../daemon/errors";
import type {
  UsageBucketWire,
  UsageCostWire,
  UsageCoverageStatus,
  UsageFiguresWire,
  UsageProviderWire,
  UsageSummaryRequest,
  UsageSummaryWire,
} from "../../daemon/usageWire";
import { measured, notOk, observation, okResult, unsupportedValue } from "./result";
import type { Measured, Provenance, UsageCoverage, UsageSource, UsageSummary } from "./types";

const MAX_WINDOW_MS = 26 * 3_600_000;
const DECIMAL = /^\d+(\.\d+)?$/;
const STATUSES: readonly string[] = ["unknown", "partial", "complete"];

/** The client's local day, as the daemon needs it. The daemon has no timezone database. */
export interface LocalDay {
  timezone: string;
  utcOffsetSeconds: number;
  startMs: number;
  endMs: number;
}

/** `[local midnight, next local midnight)` from the host clock, so 23 and 25 hour DST days are exact. */
export function hostLocalDay(nowMs: number): LocalDay {
  const now = new Date(nowMs);
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime();
  const utcOffsetSeconds = -now.getTimezoneOffset() * 60 || 0;
  const label = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const sign = utcOffsetSeconds < 0 ? "-" : "+";
  const minutes = Math.abs(utcOffsetSeconds) / 60;
  const fallback = `UTC${sign}${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
  return { timezone: label || fallback, utcOffsetSeconds, startMs: start, endMs: end };
}

export function usageRequest(day: LocalDay, nowMs: number): UsageSummaryRequest {
  const span = day.endMs - day.startMs;
  const pinned = span > 0 && span <= MAX_WINDOW_MS;
  return {
    timezone: day.timezone,
    utc_offset_seconds: day.utcOffsetSeconds,
    ...(pinned ? { window_start_unix_ms: day.startMs, window_end_unix_ms: day.endMs } : { now_unix_ms: nowMs }),
  };
}

function malformed(what: string): never {
  throw new DaemonError("protocol", `usage.summary returned a malformed ${what}`);
}

function record(value: unknown, what: string): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : malformed(what);
}

function count(value: unknown, what: string): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : malformed(what);
}

function text(value: unknown, what: string): string {
  return typeof value === "string" && value.length > 0 ? value : malformed(what);
}

function status(value: unknown, what: string): UsageCoverageStatus {
  return typeof value === "string" && STATUSES.includes(value) ? (value as UsageCoverageStatus) : malformed(what);
}

function figures(value: unknown, what: string): UsageFiguresWire {
  const raw = record(value, what);
  if (!Array.isArray(raw.costs)) return malformed(`${what} costs`);
  const seen = new Set<string>();
  const costs = raw.costs.map<UsageCostWire>((entry) => {
    const cost = record(entry, `${what} cost`);
    const currency = text(cost.currency, `${what} currency`);
    const amount = text(cost.amount, `${what} amount`);
    // One entry per currency: a repeat would have to be added, and currencies are never merged.
    if (!DECIMAL.test(amount) || seen.has(currency)) return malformed(`${what} cost`);
    seen.add(currency);
    return { currency, amount };
  });
  return { ...(raw as unknown as UsageFiguresWire), costs };
}

function bucket(value: unknown, what: string): UsageBucketWire {
  const raw = record(value, what);
  return {
    sessions: count(raw.sessions, `${what} sessions`),
    sessions_with_tokens: count(raw.sessions_with_tokens, `${what} sessions_with_tokens`),
    sessions_with_cost: count(raw.sessions_with_cost, `${what} sessions_with_cost`),
    token_status: status(raw.token_status, `${what} token_status`),
    cost_status: status(raw.cost_status, `${what} cost_status`),
    reported: figures(raw.reported, `${what} reported figures`),
    estimated: figures(raw.estimated, `${what} estimated figures`),
  };
}

/** Validates the untyped result; anything unexpected is a protocol failure, never a guessed value. */
export function parseUsageSummary(value: unknown): UsageSummaryWire {
  const raw = record(value, "result");
  const window = record(raw.window, "window");
  const start = window.start_unix_ms;
  const end = window.end_unix_ms;
  if (typeof start !== "number" || typeof end !== "number" || !Number.isFinite(start) || !Number.isFinite(end)) {
    return malformed("window");
  }
  if (!Array.isArray(raw.providers)) return malformed("providers");
  return {
    window: {
      start_unix_ms: start,
      end_unix_ms: end,
      timezone: text(window.timezone, "window timezone"),
      utc_offset_seconds: typeof window.utc_offset_seconds === "number" ? window.utc_offset_seconds : malformed("window offset"),
    },
    totals: bucket(raw.totals, "totals"),
    providers: raw.providers.map<UsageProviderWire>((entry) => ({
      ...bucket(entry, "provider"),
      provider: text((entry as { provider?: unknown } | null)?.provider, "provider name"),
    })),
  };
}

const listed = (costs: readonly UsageCostWire[]) => costs.map((cost) => `${cost.currency} ${cost.amount}`).join(", ");

/**
 * One cost list as one number in the pill's single currency. An empty list is unknown; a list in
 * another or several currencies is not summed or converted.
 */
function figure(costs: readonly UsageCostWire[], currency: string, nowMs: number, none: string): Measured<number> {
  if (costs.length === 0) return { status: "unknown", reason: none };
  const [only] = costs;
  if (costs.length === 1 && only!.currency === currency) {
    return measured(Number(only!.amount), observation(nowMs));
  }
  return unsupportedValue(
    `Costs were reported in ${listed(costs)}. The usage pill shows one currency and never adds or converts currencies`,
  );
}

function reporting(provider: UsageBucketWire): UsageCoverage["reporting"] {
  if (provider.reported.costs.length > 0) return provider.cost_status === "complete" ? "provider-reported" : "partial";
  if (provider.estimated.costs.length > 0) return "estimated";
  return provider.sessions_with_tokens > 0 || provider.token_status !== "unknown" ? "tokens-only" : "none";
}

export function mapUsageSummary(wire: UsageSummaryWire, nowMs: number): UsageSummary {
  const { reported, estimated } = wire.totals;
  // The pill has one currency: the reported one when it is unambiguous, else the estimate's.
  const reportedCurrencies = reported.costs.map((cost) => cost.currency);
  const estimatedCurrencies = estimated.costs.map((cost) => cost.currency);
  const sole = (list: string[]) => (list.length === 1 ? list[0]! : undefined);
  const currency = sole(reportedCurrencies) ?? (reportedCurrencies.length === 0 ? sole(estimatedCurrencies) : undefined) ?? "unknown";
  const sessions = wire.totals.sessions;
  return {
    windowStart: new Date(wire.window.start_unix_ms).toISOString(),
    timezone: wire.window.timezone,
    currency,
    reported: figure(
      reported.costs,
      currency,
      nowMs,
      `No provider reported a cost in this window (${sessions} ${sessions === 1 ? "session" : "sessions"} started). A missing cost is not zero`,
    ),
    estimated: figure(estimated.costs, currency, nowMs, "No estimate was recorded; the daemon has no price table"),
    budget: unsupportedValue("The daemon records no budget."),
    coverage: wire.providers.map((provider) => ({ provider: provider.provider, reporting: reporting(provider) })),
  };
}

function failure<T>(provenance: Provenance, error: unknown) {
  const failed = asDaemonError(error);
  const reason = `${presentError(failed).reason} (${failed.kind}: ${failed.message})`;
  const unsupported =
    failed.kind === "method-unavailable" || failed.kind === "capability-unavailable" || failed.kind === "capability-not-negotiated";
  return notOk<T>(provenance, unsupported ? "unsupported" : "unavailable", reason);
}

export function createLiveUsageSource(
  connection: ConnectionManager,
  now: () => number = Date.now,
  localDay: (nowMs: number) => LocalDay = hostLocalDay,
): UsageSource {
  const provenance: Provenance = { kind: "daemon", id: "D12", methods: ["usage.summary"] };
  return {
    provenance,
    async usage() {
      try {
        const at = now();
        const wire = parseUsageSummary(await connection.api().usageSummary(usageRequest(localDay(at), at)));
        return okResult(provenance, mapUsageSummary(wire, at), observation(at));
      } catch (error) {
        return failure<UsageSummary>(provenance, error);
      }
    },
  };
}
