import { useEffect, useRef } from "react";
import type { Measured, UsageCostCoverage, UsageEstimateCoverage, UsagePriceTable, UsageSummary } from "../../sources/surfaces/types";
import { useServices, useSurface, type SurfaceRefresh } from "../services";
import { agentDisplayName } from "../../daemon/agentDisplayNames";
import { PlaceholderBadge } from "../widgets";
import { LocalToolsChip, LocalToolsSection } from "./LocalToolsUsage";
import { chipText } from "./localToolsCopy";
import { useLocalTools } from "./useLocalTools";
import { money } from "./usageMoney";
import { usageHeading } from "./usageWindow";
import "./topbar.css";

const REFRESH: SurfaceRefresh = { pollMs: 60_000, onFocus: true, atLocalMidnight: true };
const MAX_LABEL = 40;

/** Provider and timezone names come from the daemon and host; keep them from blowing out the popover. */
function clip(label: string): { text: string; title?: string } {
  return label.length > MAX_LABEL ? { text: `${label.slice(0, MAX_LABEL - 1)}…`, title: label } : { text: label };
}

function isPartial(coverage: UsageCostCoverage | undefined): coverage is UsageCostCoverage {
  return coverage !== undefined && coverage.status !== "complete";
}

/** Only a daemon with the price table reports estimate coverage; it is partial when some priceable-looking sessions were not priced. */
function partialEstimate(summary: UsageSummary): UsageEstimateCoverage | null {
  const coverage = summary.estimateCoverage;
  return coverage && coverage.sessionsNeedingEstimate > coverage.sessionsEstimated ? coverage : null;
}

function tableLabel(table: UsagePriceTable): string {
  return `price table ${table.version}, ${table.basis}`;
}

function age(ageMs: number): string {
  const minutes = Math.round(ageMs / 60_000);
  return minutes < 1 ? "just now" : `${minutes} min ago`;
}

/** Reasons arrive as free text that may already end in a period. */
function trimStop(text: string): string {
  return text.replace(/[.\s]+$/, "");
}

function windowLabel(windowStart: string, timezone: string): string {
  const date = new Date(windowStart);
  if (Number.isNaN(date.getTime())) return windowStart;
  try {
    return new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short", timeZone: timezone }).format(date);
  } catch {
    return windowStart;
  }
}

function figure(label: string, value: Measured<number>, currency: string): string {
  return value.status === "ok"
    ? `${label}: ${money(value.value, currency)} (observed ${age(value.ageMs)}).`
    : `${label}: ${value.status} (${trimStop(value.reason)}).`;
}

function TokenWarnings({ summary }: { summary: UsageSummary }) {
  const warnings = summary.tokenWarnings;
  if (!warnings) return null;
  if (warnings.state === "off") return <p>Daily token warnings off.</p>;
  if (warnings.state !== "ok") return <p>Daily token warnings unavailable. Check the token warning settings; usage figures remain available.</p>;
  const labels = { input_tokens: "fresh input tokens", cache_read_tokens: "cache read tokens", cache_write_tokens: "cache write tokens", output_tokens: "output tokens" };
  return (
    <section aria-label="Daily token warnings">
      <p>Known recorded tokens only. Missing usage may mean the actual total is higher. These warnings do not stop agents.</p>
      <ul>
        {warnings.rules.map((rule) => (
          <li key={`${rule.provider}:${rule.metric}`}>
            {agentDisplayName(rule.provider)}: {rule.exceeded ? "threshold exceeded, " : ""}
            {rule.observed === null ? "count unavailable" : `${BigInt(rule.observed).toLocaleString()} ${labels[rule.metric]} recorded`}
            {`; daily ${labels[rule.metric]} warning threshold ${BigInt(rule.limit).toLocaleString()}.`}
          </li>
        ))}
      </ul>
      {warnings.rules.length === 0 && <p>No daily token thresholds configured.</p>}
    </section>
  );
}

function UsageDetails({ summary }: { summary: UsageSummary }) {
  const { reported, estimated, budget, currency, timezone, windowStart, coverage, costCoverage, priceTable, knownCostCoverage } = summary;
  const zone = clip(timezone);
  const since = `since ${windowLabel(windowStart, timezone)} (${zone.text})`;
  const unpricedEstimate = partialEstimate(summary);
  const unknownSessions = knownCostCoverage ? knownCostCoverage.sessions - knownCostCoverage.sessionsWithKnownCost : 0;
  return (
    <>
      <p title={zone.title}>
        {reported.status === "ok"
          ? `${money(reported.value, currency)} reported ${since}.`
          : `Reported usage ${since} is ${reported.status}: ${trimStop(reported.reason)}.`}
      </p>
      <TokenWarnings summary={summary} />
      {reported.status === "ok" && isPartial(costCoverage) && (
        <p data-testid="top-usage-partial">
          Partial: {costCoverage.sessionsWithCost} of {costCoverage.sessions}{" "}
          {costCoverage.sessions === 1 ? "session" : "sessions"} reported a cost, so this is a lower bound.
        </p>
      )}
      <p data-testid="top-usage-estimate">
        {figure("Estimated, not reported", estimated, currency)} Estimates are never added to the reported figure.
        {priceTable && estimated.status === "ok" ? ` List-price estimate from ${tableLabel(priceTable)}, not a bill.` : ""}
      </p>
      {estimated.status === "ok" && unpricedEstimate && (
        <p data-testid="top-usage-estimate-partial">
          Partial estimate: {unpricedEstimate.sessionsEstimated} of {unpricedEstimate.sessionsNeedingEstimate}{" "}
          {unpricedEstimate.sessionsNeedingEstimate === 1 ? "session" : "sessions"} with tokens but no reported cost could be
          priced, so this is a lower bound.
        </p>
      )}
      {unknownSessions > 0 && (
        <p data-testid="top-usage-unknown">
          Unknown: {unknownSessions} of {knownCostCoverage!.sessions} {knownCostCoverage!.sessions === 1 ? "session has" : "sessions have"}{" "}
          neither a reported nor an estimated cost. Unknown is not zero.
        </p>
      )}
      {priceTable ? (
        <p data-testid="top-usage-price-table">
          Price table {priceTable.version} · basis {priceTable.basis}
          {priceTable.source ? ` · ${priceTable.source}` : ""}
        </p>
      ) : costCoverage ? (
        <p data-testid="top-usage-price-table">No price table: this daemon reports no estimates.</p>
      ) : null}
      <p>{figure("Budget", budget, currency)}</p>
      <ul aria-label="Reporting coverage">
        {coverage.map((entry, index) => {
          const name = clip(agentDisplayName(entry.provider));
          return (
            <li key={`${index}:${entry.provider}`} title={name.title}>
              {name.text}: {entry.reporting === "none" ? "no usage reported" : entry.reporting}
            </li>
          );
        })}
      </ul>
    </>
  );
}

/**
 * Daily usage pill (D12) with an info popover. The headline is the reported amount. With nothing
 * reported it may be a price-table estimate, drawn "≈ $x" and labeled "estimated" with the table
 * version; an estimate is never added to a reported figure. Anything else reads "not available"
 * (unknown) and is never drawn as $0. Fixture amounts carry the PLACEHOLDER badge. The popover keeps
 * the reporting window, timezone, currency, price table and per-provider coverage next to the figure.
 * Other AI tools' estimates (opt-in) get their own chip and popover section and never change the headline.
 */
export function UsagePill() {
  const { surfaces } = useServices();
  const source = surfaces.usage;
  const result = useSurface(() => source.usage(), "top-usage", source.provenance, source.provenance.kind === "daemon" ? REFRESH : undefined);
  const tools = useLocalTools(surfaces.localTools);
  const details = useRef<HTMLDetailsElement>(null);
  const summary = result.status === "ok" ? result.data : null;
  const tokenWarnings = summary?.tokenWarnings;
  const tokenWarning = tokenWarnings?.state === "ok" && tokenWarnings.rules.some((rule) => rule.exceeded === true);
  const tokenWarningUnavailable = tokenWarnings && tokenWarnings.state !== "ok" && tokenWarnings.state !== "off";
  const toolsSnapshot = tools.result?.status === "ok" ? tools.result.data : null;
  const toolsChip = chipText(toolsSnapshot, tools.pending ?? toolsSnapshot?.enabled ?? false);
  const reported = summary?.reported.status === "ok" ? summary.reported.value : null;
  // With nothing reported, a price-table estimate may stand in, marked "estimated" and never as reported.
  const estimate = summary && reported === null && summary.estimated.status === "ok" ? summary.estimated.value : null;
  const text =
    result.status === "loading"
      ? "loading…"
      : summary && reported !== null
        ? money(reported, summary.currency)
        : summary && estimate !== null
          ? `≈ ${money(estimate, summary.currency)}`
          : summary && summary.reported.status === "unknown" && summary.costCoverage?.sessions === 0
            ? "nothing recorded yet"
            : "not available";
  const reason =
    result.status === "loading" || result.status === "ok" ? null : `Usage is ${result.status}: ${trimStop(result.reason)}.`;

  const close = () => {
    if (details.current) details.current.open = false;
  };
  const open = () => {
    if (details.current) details.current.open = true;
  };

  // WebKit does not focus a clicked <summary>, so blur alone cannot close the popover.
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (details.current?.open && !details.current.contains(event.target as Node | null)) close();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, []);

  return (
    <div className="top-usage" role="group" aria-label="Daily usage" data-testid="top-usage">
      <span>Usage:</span>
      <strong data-testid="top-usage-value">{text}</strong>
      {(tokenWarning || tokenWarningUnavailable) && (
        <button type="button" className="top-usage-token-warning" data-testid="top-token-warning" onClick={open}>
          {tokenWarning ? "Token threshold exceeded" : "Token warnings unavailable"}
        </button>
      )}
      {summary && reported !== null && (
        <span title={isPartial(summary.costCoverage) ? "Only some sessions reported a cost; this is a lower bound." : undefined}>
          {isPartial(summary.costCoverage) ? "reported (partial)" : "reported"}
        </span>
      )}
      {summary && estimate !== null && (
        <span
          data-testid="top-usage-basis"
          title={`${partialEstimate(summary) ? "Only some sessions could be priced; this is a lower bound. " : ""}List-price estimate${summary.priceTable ? ` from ${tableLabel(summary.priceTable)}` : ""}, not a bill.`}
        >
          {partialEstimate(summary) ? "estimated (partial)" : "estimated"}
          {summary.priceTable ? ` · table ${summary.priceTable.version}` : ""}
        </span>
      )}
      {toolsChip !== null && <LocalToolsChip text={toolsChip} onOpen={open} />}
      {source.provenance.kind === "fixture" && result.status !== "loading" && <PlaceholderBadge provenance={source.provenance} />}
      <details
        ref={details}
        className="top-usage-info"
        onKeyDown={(event) => {
          if (event.key === "Escape" && details.current?.open) {
            event.stopPropagation();
            close();
            details.current.querySelector("summary")?.focus();
          }
        }}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) close();
        }}
      >
        <summary aria-label="More information about usage">i</summary>
        <div className="top-usage-popup" role="note" data-testid="top-usage-info">
          <strong>{summary ? usageHeading(summary.windowStart, summary.timezone, Date.now()) : "AI usage"}</strong>
          {summary ? (
            <UsageDetails summary={summary} />
          ) : (
            <p>{reason ?? "Loading usage."} A missing figure is not zero spend.</p>
          )}
          <LocalToolsSection source={surfaces.localTools} tools={tools} />
          {source.provenance.kind === "fixture" && <p>Figures are placeholder data.</p>}
        </div>
      </details>
    </div>
  );
}
