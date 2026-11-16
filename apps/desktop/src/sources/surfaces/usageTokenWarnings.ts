import type { TokenWarningMetric, UsageTokenWarningsWire } from "../../daemon/usageWire";

const METRICS: readonly string[] = ["input_tokens", "cache_read_tokens", "cache_write_tokens", "output_tokens"];
const U64_MAX = 18_446_744_073_709_551_615n;

function counter(value: unknown, positive: boolean): value is string {
  return typeof value === "string" && /^(0|[1-9][0-9]{0,19})$/.test(value) && BigInt(value) <= U64_MAX && (!positive || BigInt(value) > 0n);
}

/** A malformed optional warning must remain visible without hiding the usage figures. */
export function parseTokenWarnings(value: unknown): UsageTokenWarningsWire | undefined {
  if (value === undefined) return undefined;
  const unavailable = (): UsageTokenWarningsWire => ({ schema_version: 1, state: "unavailable", rules: [] });
  if (typeof value !== "object" || value === null || Array.isArray(value)) return unavailable();
  const raw = value as Record<string, unknown>;
  if (raw.schema_version !== 1 || !Array.isArray(raw.rules) || raw.rules.length > 32) return unavailable();
  if (raw.state === "off" || raw.state === "invalid_configuration" || raw.state === "unavailable") {
    return raw.rules.length === 0 ? { schema_version: 1, state: raw.state, rules: [] } : unavailable();
  }
  if (raw.state !== "ok") return unavailable();
  const seen = new Set<string>();
  const rules: UsageTokenWarningsWire["rules"] = [];
  for (const value of raw.rules) {
    if (typeof value !== "object" || value === null || Array.isArray(value)) return unavailable();
    const rule = value as Record<string, unknown>;
    if (
      typeof rule.provider !== "string" || !/^[a-zA-Z0-9_-]{1,64}$/.test(rule.provider) ||
      typeof rule.metric !== "string" || !METRICS.includes(rule.metric) || !counter(rule.limit, true)
    ) return unavailable();
    const key = `${rule.provider}:${rule.metric}`;
    if (seen.has(key)) return unavailable();
    seen.add(key);
    if (rule.observed === null) {
      if (rule.exceeded !== null) return unavailable();
    } else {
      if (!counter(rule.observed, false)) return unavailable();
      if (rule.exceeded !== (BigInt(rule.observed) > BigInt(rule.limit))) return unavailable();
    }
    rules.push({ provider: rule.provider, metric: rule.metric as TokenWarningMetric, limit: rule.limit, observed: rule.observed as string | null, exceeded: rule.exceeded as boolean | null });
  }
  return { schema_version: 1, state: "ok", rules };
}
