// The watched pull requests, a read-only list in the Delivery tab (DPES-1105 Phase 4c). Nothing here merges: a merge is
// proposed by the orchestrator and approved on its card. Titles, branches and check names are remote text and are shown
// as plain text only: no markup, no link, no attribute built from them.
import { useCallback, useEffect, useRef, useState } from "react";
import { asDaemonError } from "../../../daemon/errors";
import { notOk, observation } from "../../../sources/surfaces/result";
import { PULL_REFRESH_COOLDOWN_MS } from "../../../sources/surfaces/pulls";
import type { PullItem } from "../../../sources/surfaces/types";
import { useGate, useServices, useSurface } from "../../services";
import { Badge, Button, EmptyState, Pill } from "../../theme";
import { useRetained } from "../workspace/useRetained";
import {
  agoText,
  checkLine,
  checksSummary,
  mergeableText,
  mergeStateText,
  plainRemote,
  prStateText,
  prStateTone,
  pullsErrorText,
  shortSha,
} from "./pullsModel";

/** Read again while the Delivery tab is open: the panel is only mounted then. */
export const PULLS_POLL_MS = 30_000;

export const NO_PULLS_TEXT = "No pull requests are being watched. Open one from a task branch and it appears here.";

function Row({
  pull,
  fixture,
  cooling,
  busy,
  onRefresh,
}: {
  pull: PullItem;
  fixture: boolean;
  cooling: boolean;
  busy: boolean;
  onRefresh: (pull: PullItem) => void;
}) {
  const open = pull.state === "open";
  const merge = mergeStateText(pull.merge);
  const reasonId = `pull-refresh-reason-${pull.taskId}`;
  return (
    <li className="rd-row-item" data-testid={`pull-${pull.number}`}>
      <span className="rd-row-title">
        <span>
          Pull request {pull.number}: {plainRemote(pull.title)}
        </span>
        <span>
          <Pill tone={prStateTone(pull.state)}>{prStateText(pull.state)}</Pill>
          {fixture && <Badge tone="placeholder">Fixture data</Badge>}
        </span>
      </span>
      <span className="rd-row-sub">
        Branch {plainRemote(pull.branch, 80)} into {plainRemote(pull.baseBranch, 80)} · commit{" "}
        <code aria-label={`Commit ${shortSha(pull.headSha)}`}>{shortSha(pull.headSha)}</code>
      </span>
      <span className="rd-row-sub">
        {checksSummary(pull)} · {mergeableText(pull.mergeable)} · {agoText(pull.observedAgeSecs)}
      </span>
      {merge !== null && <span className="rd-row-sub">{merge}</span>}
      {pull.checks.length > 0 && (
        <ul className="rd-row-sub" aria-label={`Checks for pull request ${pull.number}`}>
          {pull.checks.map((check, index) => (
            <li key={index}>{checkLine(check)}</li>
          ))}
        </ul>
      )}
      {open && (
        <span className="rd-held-actions">
          <Button
            variant="quiet"
            aria-label={`Refresh pull request ${pull.number}`}
            aria-describedby={cooling ? reasonId : undefined}
            disabled={cooling || busy}
            onClick={() => onRefresh(pull)}
          >
            Refresh
          </Button>
          {cooling && (
            <span id={reasonId} className="rd-reason">
              Read a moment ago. Refresh is available again in about 10 seconds.
            </span>
          )}
        </span>
      )}
    </li>
  );
}

/** Renders nothing for a live daemon without the `landing` capability, and for a source that says it is unsupported. */
export function PullRequests() {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.pulls;
  const live = source.provenance.kind === "daemon";
  // Read from the negotiated list, not `gate.can`, so the section does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("landing");
  const [nonce, setNonce] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [cooling, setCooling] = useState<ReadonlySet<string>>(new Set());
  const [announcement, setAnnouncement] = useState("");
  const [fresh, setFresh] = useState<{ base: unknown; rows: Readonly<Record<string, PullItem>> } | null>(null);
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  useEffect(
    () => () => {
      for (const timer of timers.current) clearTimeout(timer);
    },
    [],
  );

  const loaded = useSurface(
    () =>
      supported
        ? source.list()
        : Promise.resolve(notOk<PullItem[]>(source.provenance, "unsupported", "This daemon does not watch pull requests.", observation(Date.now()))),
    `pulls#${nonce}`,
    source.provenance,
    supported ? { pollMs: PULLS_POLL_MS, onFocus: true } : undefined,
  );
  const result = useRetained(loaded, "pulls");

  const refresh = useCallback(
    async (pull: PullItem) => {
      if (busy !== null) return;
      setBusy(pull.taskId);
      // The daemon allows one fresh read per task every 10 seconds, so the button waits as long, whatever the answer.
      setCooling((current) => new Set(current).add(pull.taskId));
      const timer = setTimeout(() => {
        timers.current.delete(timer);
        setCooling((current) => {
          const next = new Set(current);
          next.delete(pull.taskId);
          return next;
        });
      }, PULL_REFRESH_COOLDOWN_MS);
      timers.current.add(timer);
      try {
        const row = await source.refresh(pull.taskId);
        setFresh((current) => ({ base: result, rows: { ...(current !== null && current.base === result ? current.rows : {}), [row.taskId]: row } }));
        setAnnouncement(`Read pull request ${row.number} again: ${checksSummary(row)}.`);
        setNonce((value) => value + 1);
      } catch (error) {
        setAnnouncement(pullsErrorText(asDaemonError(error)));
      } finally {
        setBusy(null);
      }
    },
    [busy, result, source],
  );

  if (!supported || result.status === "unsupported") return null;
  const fixture = !live;
  const rows = result.status === "ok" ? result.data.map((pull) => (fresh !== null && fresh.base === result ? fresh.rows[pull.taskId] : undefined) ?? pull) : [];
  return (
    <section aria-labelledby="rd-pulls-title" className="rd-pulls">
      <h3 id="rd-pulls-title">Pull requests</h3>
      <p className="rd-note">
        Pull requests the app watches for your tasks. This list only reads: merging happens on an approval card, never from here.
      </p>
      <p className="rd-note" role="status" aria-live="polite" data-testid="pulls-status">
        {announcement}
      </p>
      {result.status === "loading" ? (
        <p className="rd-note">Loading pull requests…</p>
      ) : result.status !== "ok" ? (
        <EmptyState title="Pull requests are not available" description={result.reason} />
      ) : rows.length === 0 ? (
        <EmptyState title="No pull requests" description={NO_PULLS_TEXT} />
      ) : (
        <ul className="rd-rows" aria-label="Watched pull requests">
          {rows.map((pull) => (
            <Row
              key={pull.taskId}
              pull={pull}
              fixture={fixture}
              cooling={cooling.has(pull.taskId)}
              busy={busy !== null}
              onRefresh={(item) => void refresh(item)}
            />
          ))}
        </ul>
      )}
    </section>
  );
}
