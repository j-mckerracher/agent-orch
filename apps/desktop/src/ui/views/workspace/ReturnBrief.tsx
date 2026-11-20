import { useEffect, useId, useRef, useState } from "react";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { Brief } from "../../../sources/surfaces/types";
import { useGate, useServices, useSurface } from "../../services";
import { Button } from "../../theme";
import {
  BRIEF_ACK_FAILED_TEXT,
  BRIEF_ACK_TEXT,
  BRIEF_ATTENTION_HEADING,
  BRIEF_GRANTS_HEADING,
  BRIEF_READY_TEXT,
  BRIEF_WAITING_HEADING,
  ackedText,
  briefHeading,
  briefLines,
  briefWindowText,
  grantStatusText,
  grantUsesText,
  itemText,
} from "./briefModel";
import { useRetained } from "./useRetained";

/** The daemon has no push stream for the brief, so the panel reads again on this interval. */
export const BRIEF_POLL_MS = 5_000;

/**
 * The "While you were away" line (DPES-1105 Phase 5b). One line with Show and Dismiss: Show opens the detail, Dismiss
 * acknowledges a closed brief and hides a live "so far" brief. Hidden quietly for a daemon without
 * `standing_approvals`. Only counts, state words and short titles are shown, never ids or text an agent wrote.
 */
export function ReturnBrief() {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.monitoring;
  const live = source.provenance.kind === "daemon";
  // Read from the negotiated list, so the panel does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("standing_approvals");
  const [nonce, setNonce] = useState(0);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [hiddenSoFar, setHiddenSoFar] = useState<number | null>(null);
  const [message, setMessage] = useState<{ text: string; quiet: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const working = useRef(false);
  const announced = useRef<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const headingId = useId();
  const detailId = useId();

  const loaded = useSurface(
    () =>
      supported
        ? source.brief()
        : Promise.resolve(notOk<Brief | null>(source.provenance, "unsupported", "This daemon has no return brief.", observation(Date.now()))),
    `brief#${nonce}`,
    source.provenance,
    supported ? { pollMs: BRIEF_POLL_MS, onFocus: true } : undefined,
  );
  const result = useRetained(loaded, "brief");
  const brief = supported && result.status === "ok" ? result.data : null;
  const key = brief ? `${brief.fromMs}:${brief.toMs}` : null;
  const shown = brief !== null && (brief.open ? brief.fromMs !== hiddenSoFar : key !== dismissed);

  // Announce a closed brief once when it first appears. The region is always mounted, so the change is spoken.
  useEffect(() => {
    if (brief && !brief.open && key !== dismissed && announced.current !== key) {
      announced.current = key;
      setMessage({ text: BRIEF_READY_TEXT, quiet: true });
    }
  }, [brief, key, dismissed]);

  if (!supported) return null;

  const dismiss = async () => {
    if (brief?.open) {
      setHiddenSoFar(brief.fromMs);
      return;
    }
    if (working.current || key === null) return;
    working.current = true;
    setBusy(true);
    try {
      const receipt = await source.briefAck();
      setDismissed(key);
      setMessage({ text: ackedText(receipt.simulated), quiet: !receipt.simulated });
      setNonce((value) => value + 1);
      // The panel is gone, so focus moves to the region that says what happened.
      rootRef.current?.focus();
    } catch {
      setMessage({ text: BRIEF_ACK_FAILED_TEXT, quiet: false });
    } finally {
      working.current = false;
      setBusy(false);
    }
  };

  return (
    <div ref={rootRef} tabIndex={-1} className="ws-brief-root">
      <p role="status" className={message?.quiet ? "ws-brief-quiet" : "ws-small"}>
        {message?.text ?? ""}
      </p>
      {shown && brief && (
        <section aria-labelledby={headingId} className="ws-brief">
          <div className="ws-brief-line">
            <h3 id={headingId}>{briefHeading(brief)}</h3>
            <p>{briefLines(brief).slice(0, 2).join(" · ")}</p>
            <Button variant="quiet" aria-expanded={expanded} aria-controls={detailId} onClick={() => setExpanded((open) => !open)}>
              {expanded ? "Hide" : "Show"}
            </Button>
            <Button variant="quiet" disabled={busy} onClick={() => void dismiss()}>
              {BRIEF_ACK_TEXT}
            </Button>
          </div>
          <div id={detailId} className="ws-brief-detail" hidden={!expanded}>
            <p className="ws-small">{briefWindowText(brief)}</p>
            <ul>
              {briefLines(brief).map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
            <BriefList title={BRIEF_WAITING_HEADING} lines={brief.waiting.map(itemText)} />
            <BriefList title={BRIEF_ATTENTION_HEADING} lines={brief.attention.map(itemText)} />
            {brief.grants.length > 0 && (
              <>
                <h4>{BRIEF_GRANTS_HEADING}</h4>
                <ul>
                  {brief.grants.map((grant) => (
                    <li key={grant.id}>
                      {grant.scopeLabel}: {grantStatusText(grant)}. {grantUsesText(grant)}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        </section>
      )}
    </div>
  );
}

function BriefList({ title, lines }: { title: string; lines: string[] }) {
  if (lines.length === 0) return null;
  return (
    <>
      <h4>{title}</h4>
      <ul>
        {lines.map((line, at) => (
          <li key={`${at}:${line}`}>{line}</li>
        ))}
      </ul>
    </>
  );
}
