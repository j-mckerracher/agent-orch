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
  token_warnings?: UsageTokenWarningsWire;
}

export type TokenWarningMetric = "input_tokens" | "cache_read_tokens" | "cache_write_tokens" | "output_tokens";

export interface UsageTokenWarningsWire {
  schema_version: 1;
  state: "off" | "ok" | "invalid_configuration" | "unavailable";
  rules: Array<{
    provider: string;
    metric: TokenWarningMetric;
    /** Decimal strings preserve the full unsigned 64-bit counter range. */
    limit: string;
    observed: string | null;
    exceeded: boolean | null;
  }>;
}

/** Window params shared by `usage.localTools`, `.setEnabled` and `.refresh`; all four are required. */
export interface LocalToolsWindowRequest {
  /** IANA name or label (1..=64 bytes). */
  timezone: string;
  utc_offset_seconds: number;
  /** End is after start and the span is at most 26 hours. */
  window_start_unix_ms: number;
  window_end_unix_ms: number;
}

export interface LocalToolsSetEnabledRequest extends LocalToolsWindowRequest {
  enabled: boolean;
}

export type LocalToolsReaderStateWire =
  | "off"
  | "not_started"
  | "ok"
  | "not_installed"
  | "node_missing"
  | "node_too_old"
  | "unsupported_version"
  | "integrity_mismatch"
  | "timeout"
  | "failed"
  | "bad_output"
  | "output_too_large"
  | "wrong_day"
  | "not_bundled";

export type LocalToolsProviderIdWire = "claude-code" | "codex" | "github-copilot";

export interface LocalToolsCostWire {
  currency: string;
  /** A list-price estimate as a decimal string. */
  amount: string;
}

export interface LocalToolsProviderWire {
  id: LocalToolsProviderIdWire;
  state: "ok" | "no_calls" | "not_run";
  sessions: number | null;
  calls: number | null;
  tokens: {
    input: number | null;
    output: number | null;
    cache_read: number | null;
    cache_write: number | null;
  };
  /** Empty means unknown. Models without a price are not in here; see `unpriced_models`. */
  estimated_costs: LocalToolsCostWire[];
  unpriced_models: number;
  unpriced_tokens: number;
  cost_complete: boolean;
}

/** `usage.localTools` result, schema_version 1. Figures are estimates read from local session files. */
export interface LocalToolsSnapshotWire {
  schema_version: 1;
  enabled: boolean;
  window: {
    start_unix_ms: number;
    end_unix_ms: number;
    timezone: string;
    utc_offset_seconds: number;
  };
  /** The window the provider figures cover; null when there are no figures. */
  data_window: { start_unix_ms: number; end_unix_ms: number; timezone: string } | null;
  reader: {
    state: LocalToolsReaderStateWire;
    version: string | null;
    pinned_version: string;
    node_version: string | null;
    /** Only for `failed`. */
    exit_status: number | null;
    /** Where the reader came from. Null when none was used. */
    source: "bundled" | "override" | "path" | null;
    /** True in development builds, the only builds with an override or a PATH lookup. */
    development_build: boolean;
  };
  refreshing: boolean;
  /** When the provider figures were last read successfully; null when there are none. */
  observed_at_unix_ms: number | null;
  stale: boolean;
  /** Always the three providers, in the order claude-code, codex, github-copilot. */
  providers: LocalToolsProviderWire[];
}
