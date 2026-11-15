import { useId } from "react";
import type { LocalToolsProvider, LocalToolsSnapshot, LocalToolsSource, LocalToolsWindow } from "../../sources/surfaces/types";
import { Badge, Button } from "../theme";
import {
  CHIP_TITLE,
  OPT_IN_LABEL,
  OPT_IN_STATEMENT,
  providerCopy,
  readerLine,
  readerSourceLine,
  staleLine,
  totalLine,
  windowLabel,
} from "./localToolsCopy";
import type { LocalToolsState } from "./useLocalTools";

const TITLE = "Other AI tools on this machine";

/** The secondary chip in the pill. It opens the popover, where the figures are explained. */
export function LocalToolsChip({ text, onOpen }: { text: string; onOpen: () => void }) {
  return (
    <button type="button" className="top-usage-chip" data-testid="top-usage-chip" title={CHIP_TITLE} onClick={onOpen}>
      {text}
    </button>
  );
}

function ProviderItem({ provider, figures }: { provider: LocalToolsProvider; figures: LocalToolsWindow | null }) {
  const copy = providerCopy(provider, figures);
  return (
    <li data-testid={`top-usage-tools-${provider.id}`}>
      <strong>{copy.heading}</strong>
      {copy.lines.map((line, index) => (
        <p key={index}>{line}</p>
      ))}
    </li>
  );
}

/** The figures, shown only while the opt-in is on. Every figure is labeled as an estimate. */
function Figures({ snapshot }: { snapshot: LocalToolsSnapshot }) {
  const { figures, observedAtMs, stale, reader, providers } = snapshot;
  const reasonLine = readerLine(snapshot, true);
  // Figures are never shown as current when the reader has failed since they were read.
  const showStale = figures !== null && observedAtMs !== null && (stale || reader.state !== "ok");
  const total = totalLine(providers);
  return (
    <>
      {reasonLine !== null && <p data-testid="top-usage-tools-reader">{reasonLine}</p>}
      {showStale && <p>{staleLine(Date.now() - observedAtMs)}</p>}
      {figures !== null && <p>Figures cover {windowLabel(figures)}.</p>}
      {figures !== null && figures.startMs !== snapshot.window.startMs && <p>They cover a different day from this window.</p>}
      {figures !== null && (
        <ul aria-label="Estimates by tool">
          {providers.map((provider) => (
            <ProviderItem key={provider.id} provider={provider} figures={figures} />
          ))}
        </ul>
      )}
      {total !== null && <p>{total}</p>}
      {figures === null && reasonLine === null && <p>No figures for this window yet.</p>}
    </>
  );
}

interface SectionProps {
  source: LocalToolsSource;
  tools: LocalToolsState;
}

/**
 * The popover section for other AI tools: the opt-in control (off by default), its plain statement, the
 * estimates while opted in, and a refresh. Its text stands apart from the reported figure above it.
 */
export function LocalToolsSection({ source, tools }: SectionProps) {
  const statementId = useId();
  const heading = (
    <>
      <strong>{TITLE}</strong>
      {source.provenance.kind === "fixture" && <Badge tone="placeholder" data-testid="top-usage-tools-placeholder">PLACEHOLDER DATA</Badge>}
    </>
  );
  if (tools.result === null) {
    return (
      <section aria-label={TITLE} className="top-usage-tools" data-testid="top-usage-tools">
        {heading}
        <p>Loading other AI tools.</p>
      </section>
    );
  }
  if (tools.result.status !== "ok") {
    const message =
      tools.result.status === "unsupported"
        ? "Other AI tools are not available on this daemon."
        : `Other AI tools are unavailable: ${tools.result.reason.replace(/[.\s]+$/, "")}.`;
    return (
      <section aria-label={TITLE} className="top-usage-tools" data-testid="top-usage-tools">
        {heading}
        <p>{message}</p>
      </section>
    );
  }
  const snapshot = tools.result.data;
  // While a change is saving, the control shows the value being saved, so turning it off hides figures at once.
  const enabled = tools.pending ?? snapshot.enabled;
  const sourceLine = readerSourceLine(snapshot.reader);
  return (
    <section aria-label={TITLE} className="top-usage-tools" data-testid="top-usage-tools">
      {heading}
      <label className="top-usage-tools-opt-in">
        <input
          type="checkbox"
          checked={enabled}
          disabled={tools.pending !== null}
          aria-describedby={statementId}
          onChange={(event) => tools.setEnabled(event.currentTarget.checked)}
        />
        <span>{OPT_IN_LABEL}</span>
      </label>
      <p id={statementId}>{OPT_IN_STATEMENT}</p>
      {tools.pending !== null && <p>Saving the setting.</p>}
      {tools.note !== null && <p role="alert">{tools.note}</p>}
      {enabled && sourceLine !== null && <p data-testid="top-usage-tools-source">{sourceLine}</p>}
      {enabled && <Figures snapshot={snapshot} />}
      {enabled && (
        <Button className="top-usage-tools-refresh" onClick={tools.refresh} disabled={tools.refreshing}>
          {tools.refreshing ? "Refreshing…" : "Refresh now"}
        </Button>
      )}
    </section>
  );
}
