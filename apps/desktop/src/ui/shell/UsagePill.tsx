import { useEffect, useRef } from "react";
import type { Measured, UsageSummary } from "../../sources/surfaces/types";
import { useServices, useSurface } from "../services";
import { PlaceholderBadge } from "../widgets";
import "./topbar.css";

function money(value: number, currency: string): string {
  try {
    return new Intl.NumberFormat("en-US", { style: "currency", currency }).format(value);
  } catch {
    return `${value.toFixed(2)} ${currency}`;
  }
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

function UsageDetails({ summary }: { summary: UsageSummary }) {
  const { reported, estimated, budget, currency, timezone, windowStart, coverage } = summary;
  const since = `since ${windowLabel(windowStart, timezone)} (${timezone})`;
  return (
    <>
      <p>
        {reported.status === "ok"
          ? `${money(reported.value, currency)} reported ${since}.`
          : `Reported usage ${since} is ${reported.status}: ${trimStop(reported.reason)}.`}
      </p>
      <p>{figure("Estimated, not reported", estimated, currency)} Estimates are never added to the reported figure.</p>
      <p>{figure("Budget", budget, currency)}</p>
      <ul aria-label="Reporting coverage">
        {coverage.map((entry) => (
          <li key={entry.provider}>
            {entry.provider}: {entry.reporting === "none" ? "no usage reported" : entry.reporting}
          </li>
        ))}
      </ul>
    </>
  );
}

/**
 * Daily usage pill (D12) with an info popover. A figure is rendered only when the source
 * answered `ok` with a reported amount; anything else reads "not available" and is never
 * drawn as $0. Fixture amounts carry the PLACEHOLDER badge. The popover keeps the
 * reporting window, timezone, currency and per-provider coverage next to the figure.
 */
export function UsagePill() {
  const { surfaces } = useServices();
  const source = surfaces.usage;
  const result = useSurface(() => source.usage(), "top-usage", source.provenance);
  const details = useRef<HTMLDetailsElement>(null);
  const summary = result.status === "ok" ? result.data : null;
  const reported = summary?.reported.status === "ok" ? summary.reported.value : null;
  const text =
    result.status === "loading"
      ? "loading…"
      : summary && reported !== null
        ? money(reported, summary.currency)
        : "not available";
  const reason =
    result.status === "loading" || result.status === "ok" ? null : `Usage is ${result.status}: ${trimStop(result.reason)}.`;

  const close = () => {
    if (details.current) details.current.open = false;
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
      {summary && reported !== null && <span>reported</span>}
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
          <strong>Today’s AI usage</strong>
          {summary ? (
            <UsageDetails summary={summary} />
          ) : (
            <p>{reason ?? "Loading usage."} A missing figure is not zero spend.</p>
          )}
          {source.provenance.kind === "fixture" && <p>Figures are placeholder data.</p>}
        </div>
      </details>
    </div>
  );
}
