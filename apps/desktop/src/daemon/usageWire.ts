/**
 * Wire shapes of `usage.summary` (M7-06; `crates/orchestrator-daemon/src/api_usage.rs`,
 * `docs/usage-ledger.md`). Capability `agents`.
 *
 * Costs are decimal strings per currency and are never merged across currencies.
 * A token figure nothing reported is `null`; an empty `costs` list means unknown.
 */

export interface UsageSummaryRequest {
  /** Label echoed back by the daemon (1..=64 bytes). */
  timezone: string;
  /** The client's UTC offset, -43200..=50400. The daemon has no timezone database. */
  utc_offset_seconds: number;
  now_unix_ms?: number;
  /** Both or neither; pins a DST day. At most 26 hours. */
  window_start_unix_ms?: number;
  window_end_unix_ms?: number;
}

export type UsageCoverageStatus = "unknown" | "partial" | "complete";

export interface UsageCostWire {
  currency: string;
  amount: string;
}

export interface UsageFiguresWire {
  input_tokens: number | null;
  cache_read_tokens: number | null;
  cache_write_tokens: number | null;
  output_tokens: number | null;
  reasoning_tokens: number | null;
  costs: UsageCostWire[];
}

export interface UsageBucketWire {
  sessions: number;
  sessions_with_tokens: number;
  sessions_with_cost: number;
  token_status: UsageCoverageStatus;
  cost_status: UsageCoverageStatus;
  /**
   * M7-26, absent from an older daemon: sessions with tokens but no reported cost, how many of those
   * the native price table could price, and sessions with a reported or estimated cost.
   */
  sessions_needing_estimate?: number;
  sessions_with_estimated_cost?: number;
  sessions_with_known_cost?: number;
  estimated_cost_status?: UsageCoverageStatus;
  known_cost_status?: UsageCoverageStatus;
  reported: UsageFiguresWire;
  estimated: UsageFiguresWire;
}

export interface UsageProviderWire extends UsageBucketWire {
  provider: string;
}

/** M7-26: the native price table behind `estimated` figures. Absent from an older daemon. */
export interface UsagePriceTableWire {
  version: string;
  basis: string;
  source?: string;
  currency?: string;
  models?: number;
}

export interface UsageSummaryWire {
  window: {
    start_unix_ms: number;
    end_unix_ms: number;
    timezone: string;
    utc_offset_seconds: number;
  };
  totals: UsageBucketWire;
  providers: UsageProviderWire[];
  price_table?: UsagePriceTableWire;
}
