/**
 * Workspace activity panel (M7-12): the content of the shell's `activity` slot.
 *
 * Plan: /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md
 *
 * Data and contracts:
 * - Queue and attention come from D11 `surfaces.activity` (derived from `tasks.list` and
 *   `approvals.list` when a daemon is bound; PLACEHOLDER fixtures in `?fixture=1` mode).
 *   The panel re-reads the source whenever the connection changes or `AppData` refreshes
 *   `tasks`/`approvals` after a live event, and keeps the last answer on screen meanwhile.
 * - The badge is the `AppData` pending-approval count, the exact value `ReviewCount` and the top
 *   bar show, so the three agree. Without a loaded list (including fixture mode) it shows
 *   "—", never 0 and never a sample number; the sample attention rows stay PLACEHOLDER-labelled.
 * - Queued tasks are grouped honestly: "Up next" only for capacity-waiting or source-named next
 *   tasks, "Waiting" for known blockers, "Unclassified" when the daemon reports no reason.
 * - Slots, dispatch order, pause, stop-all and queue settings have no daemon data yet
 *   (D11 live: M7-05/M7-20; budget D12: M7-06). Fixture mode labels them PLACEHOLDER; live
 *   mode shows "not available" and disables the controls with a visible reason.
 * - The only real mutation is per-task Stop = `AppData.cancelTask` (`tasks.cancel`). It needs a
 *   live connection with the `tasks` capability, refuses `fixture:` ids, and asks for
 *   confirmation. Fixture rows use the source's simulated stop and never reach the daemon.
 *
 * Layout updated 2026-11-11 18:12:43 UTC. The shell docks this `<aside>` on the right at
 * >=961 px and places its capped popover above main at 960 px and below (`shell.css`).
 * This component renders no landmark of its own.
 */
import { useEffect, useId, useRef, useState } from "react";
import { isFixtureId } from "../../daemon/api";
import type { DaemonError } from "../../daemon/errors";
import type {
  ActivitySnapshot,
  AttentionItem,
  Measured,
  Provenance,
  QueueEntry,
  SurfaceReceipt,
  SurfaceResult,
  UsageSummary,
  WaitingReason,
} from "../../sources/surfaces/types";
import { basename, pendingApprovals } from "../format";
import { useGate, useServices, useStore } from "../services";
import { Badge, Button, Disclosure, EmptyState, Pill, type Tone } from "../theme";
import { ErrorNotice, SimulatedReceipts } from "../widgets";
import { NavLink } from "./NavLink";
import "./ActivityPanel.css";

type Loaded<T> = { status: "loading" } | SurfaceResult<T>;

/**
 * Loads a surface and re-runs when `deps` change. Unlike `useSurface`, the previous answer
 * stays visible until the next one resolves, so an event-driven refresh does not flash
 * "loading". A rejected load becomes `unavailable` with the source's own provenance.
 */
function useRefreshing<T>(load: () => Promise<SurfaceResult<T>>, provenance: Provenance, deps: readonly unknown[]): Loaded<T> {
  const [result, setResult] = useState<Loaded<T>>({ status: "loading" });
  useEffect(() => {
    let live = true;
    load().then(
      (value) => live && setResult(value),
      (error: unknown) =>
        live &&
        setResult({
          status: "unavailable",
          reason: error instanceof Error ? error.message : "source failed",
          provenance,
        }),
    );
    return () => {
      live = false;
    };
    // `load` closes over the source; `deps` is the re-run contract.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return result;
}

const WAITING_LABEL: Record<WaitingReason, { text: string; tone: Tone }> = {
  dependency: { text: "Waiting on a dependency", tone: "warn" },
  capacity: { text: "Waiting for capacity", tone: "warn" },
  approval: { text: "Waiting for approval", tone: "warn" },
  paused: { text: "Dispatch paused", tone: "warn" },
  unknown: { text: "Waiting (reason not reported)", tone: "neutral" },
};

function Placeholder({ id }: { id: "D11" | "D12" }) {
  return (
    <Badge tone="placeholder" data-testid={`badge-${id}`}>
      PLACEHOLDER DATA ({id})
    </Badge>
  );
}

function unavailableText<T>(value: Measured<T>): string {
  return value.status === "ok" ? "" : value.reason;
}

/** Pending approvals are P1/P2 in fixtures; a live daemon reports no priority. */
function PriorityPill({ priority }: { priority: AttentionItem["priority"] }) {
  if (priority === "P1") return <Pill tone="bad">P1</Pill>;
  if (priority === "P2") return <Pill tone="warn">P2</Pill>;
  return <Pill tone="neutral">Priority not reported</Pill>;
}

interface StopControl {
  /** Null when stopping is allowed. */
  blockedReason: string | null;
  describedBy: string;
  /** Resolves true when the outcome is announced in the panel notice, which then takes focus. */
  onStop: (taskId: string) => Promise<boolean>;
}

function QueueRow({ entry, stop, isNext }: { entry: QueueEntry; stop: StopControl; isNext: boolean }) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const stopId = useId();
  const confirmId = useId();
  const settled = useRef(false);
  // Keyboard focus follows the swap between Stop and its confirmation buttons.
  useEffect(() => {
    if (confirming) document.getElementById(confirmId)?.focus();
    else if (settled.current) document.getElementById(stopId)?.focus();
  }, [confirming, confirmId, stopId]);

  const waiting = entry.waiting ? WAITING_LABEL[entry.waiting.reason] : null;
  const label = `${entry.objective} (${entry.taskId})`;
  return (
    <li className="activity-row" data-testid={`activity-task-${entry.taskId}`}>
      <div className="activity-row-main">
        <span className="activity-row-title">{entry.objective}</span>
        <span className="activity-row-meta">
          {basename(entry.projectId)} · {entry.agent} · {entry.taskId}
        </span>
        <span className="activity-row-pills">
          <Pill tone={entry.state === "Running" || entry.state === "Verifying" ? "info" : "neutral"}>{entry.state}</Pill>
          {isNext && <Pill tone="info">Next to dispatch</Pill>}
          {waiting && <Pill tone={waiting.tone}>{waiting.text}</Pill>}
        </span>
        {entry.waiting && <span className="activity-row-meta">{entry.waiting.detail}</span>}
      </div>
      {confirming ? (
        <div className="activity-row-actions" role="group" aria-label={`Confirm stop of ${label}`}>
          <Button
            id={confirmId}
            variant="danger"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              const noticeTookFocus = await stop.onStop(entry.taskId);
              setBusy(false);
              // A stopped row may unmount at the next refresh, so the notice owns focus then.
              settled.current = !noticeTookFocus;
              setConfirming(false);
            }}
          >
            Confirm stop
          </Button>
          <Button
            variant="quiet"
            disabled={busy}
            onClick={() => {
              settled.current = true;
              setConfirming(false);
            }}
          >
            Keep
          </Button>
        </div>
      ) : (
        <Button
          id={stopId}
          variant="danger"
          aria-label={`Stop ${label}`}
          aria-describedby={stop.blockedReason ? stop.describedBy : undefined}
          disabled={stop.blockedReason !== null}
          onClick={() => setConfirming(true)}
        >
          Stop
        </Button>
      )}
    </li>
  );
}

function TaskGroup({
  title,
  entries,
  empty,
  note,
  stop,
  nextId,
  defaultOpen,
}: {
  title: string;
  entries: QueueEntry[];
  empty: string;
  note?: string;
  stop: StopControl;
  nextId?: string;
  defaultOpen?: boolean;
}) {
  return (
    <Disclosure summary={title} count={entries.length} defaultOpen={defaultOpen} className="activity-sub" data-testid={`activity-queue-${title.toLowerCase().replace(/\s+/g, "-")}`}>
      {note && <p className="activity-note">{note}</p>}
      {entries.length === 0 ? (
        <p className="activity-empty">{empty}</p>
      ) : (
        <ul className="activity-list">
          {entries.map((entry) => (
            <QueueRow key={entry.taskId} entry={entry} stop={stop} isNext={entry.taskId === nextId} />
          ))}
        </ul>
      )}
    </Disclosure>
  );
}

function AttentionRow({ item }: { item: AttentionItem }) {
  return (
    <li className="activity-row" data-testid={`activity-attention-${item.approvalId}`}>
      <div className="activity-row-main">
        <span className="activity-row-pills">
          <PriorityPill priority={item.priority} />
        </span>
        <span className="activity-row-title">{item.kind}</span>
        <span className="activity-row-meta">
          {item.projectId ? `${basename(item.projectId)} · ` : ""}
          {item.taskId}
        </span>
      </div>
    </li>
  );
}

function ReasonButton({ label, reason, onClick, disabled }: { label: string; reason: string | null; onClick?: () => void; disabled?: boolean }) {
  const id = useId();
  const blocked = reason !== null;
  return (
    <div className="activity-control">
      <Button variant="secondary" disabled={blocked || disabled} aria-describedby={blocked ? id : undefined} onClick={onClick}>
        {label}
      </Button>
      {blocked && (
        <span id={id} className="activity-reason">
          Not available: {reason}
        </span>
      )}
    </div>
  );
}

function formatBudget(usage: Loaded<UsageSummary>): { text: string; fixture: boolean } {
  if (usage.status === "loading") return { text: "Budget: loading…", fixture: false };
  const fixture = usage.provenance.kind === "fixture";
  if (usage.status !== "ok") return { text: `Budget: not available (${usage.reason})`, fixture };
  const { budget, currency } = usage.data;
  if (budget.status !== "ok") return { text: `Budget: not available (${budget.reason})`, fixture };
  return { text: `Budget: ${budget.value.toFixed(2)} ${currency}`, fixture };
}

export function ActivityPanel() {
  const { surfaces, data, connection: connectionStore } = useServices();
  const connection = useStore(connectionStore);
  const appData = useStore(data);
  const gate = useGate();
  const stopNoteId = useId();
  const [receipts, setReceipts] = useState<SurfaceReceipt[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<DaemonError | null>(null);
  const noticeRef = useRef<HTMLParagraphElement>(null);
  const [noticeFocus, setNoticeFocus] = useState(0);
  const announce = (text: string) => {
    setNotice(text);
    setNoticeFocus((n) => n + 1);
  };
  useEffect(() => {
    if (noticeFocus > 0) noticeRef.current?.focus();
  }, [noticeFocus]);
  const source = surfaces.activity;
  const fixture = source.provenance.kind === "fixture";
  const connectionDeps = [connection.generation, connection.status, connection.history.reason] as const;
  const activity = useRefreshing(() => source.activity(), source.provenance, [
    source,
    ...connectionDeps,
    appData.tasks.data,
    appData.approvals.data,
  ]);
  const usage = useRefreshing(() => surfaces.usage.usage(), surfaces.usage.provenance, [surfaces.usage, ...connectionDeps]);

  const snapshot: ActivitySnapshot | null = activity.status === "ok" ? activity.data : null;
  const running = snapshot?.running ?? [];
  const queued = snapshot?.queued ?? [];
  const attention = snapshot?.attention ?? [];
  const waitingEntries = queued.filter((e) => e.waiting && ["dependency", "approval", "paused"].includes(e.waiting.reason));
  const nextId = snapshot?.upNext.status === "ok" ? snapshot.upNext.value : undefined;
  // "Up next" claims readiness, so it holds only tasks known to wait for capacity or named by the source.
  const upNextEntries = queued
    .filter((e) => !waitingEntries.includes(e) && (e.waiting?.reason === "capacity" || e.taskId === nextId))
    .sort((a, b) => Number(b.taskId === nextId) - Number(a.taskId === nextId));
  const unclassifiedEntries = queued.filter((e) => !waitingEntries.includes(e) && !upNextEntries.includes(e));

  const stopBlockedReason = fixture
    ? null
    : !gate.can("tasks")
      ? "stopping a task needs a live daemon connection."
      : null;
  const stop: StopControl = {
    blockedReason: stopBlockedReason,
    describedBy: stopNoteId,
    onStop: async (taskId) => {
      setError(null);
      setNotice(null);
      if (fixture) {
        const receipt = await source.simulateStop(taskId);
        setReceipts((all) => [...all, receipt]);
        return false;
      }
      if (isFixtureId(taskId)) {
        announce("Sample tasks cannot be stopped.");
        return true;
      }
      const result = await data.cancelTask(taskId);
      if (!result.ok) {
        setError(result.error);
        return false;
      }
      announce(
        `Stop request acknowledged; the daemon now records ${taskId} as ${result.value.state}. This is not proof that any process has stopped.`,
      );
      return true;
    },
  };

  // The badge uses the same pending-approval count as the Review nav and top bar, so the three
  // cannot disagree. Fixture mode has no AppData list, so it shows "—" rather than a sample number.
  const attentionCount = appData.approvals.data ? pendingApprovals(appData.approvals.data).length : null;
  const badge = attentionCount === null ? "—" : String(attentionCount);
  const attentionKnown = !snapshot?.degraded?.approvals;
  const statusLine = snapshot
    ? `${running.length} running, ${queued.length} queued, ${attentionKnown ? `${attention.length} require attention` : "attention not available"}.`
    : activity.status === "loading"
      ? "Loading workspace activity."
      : `Workspace activity is ${activity.status}.`;
  const offline = activity.status === "loading" || activity.status === "ok" ? null : activity;

  const pauseReason = fixture
    ? null
    : snapshot?.dispatchPaused.status === "ok"
      ? "pausing dispatch is not wired to the daemon yet."
      : (snapshot ? unavailableText(snapshot.dispatchPaused) : "the daemon has no global dispatch pause.");
  const slotsReason = !snapshot
    ? "no activity source answered."
    : snapshot.slots.status === "ok"
      ? null
      : unavailableText(snapshot.slots);
  const budget = formatBudget(usage);

  const summary = (
    <>
      Workspace activity {fixture && <Placeholder id="D11" />}
    </>
  );
  const count = (
    <span className="activity-badge" data-testid="activity-badge">
      <span aria-hidden="true">{badge}</span>
      <span className="activity-visually-hidden">
        {attentionCount === null
          ? fixture
            ? "attention count not available (sample data is not counted)"
            : "attention count not available"
          : `${attentionCount} require attention`}
      </span>
    </span>
  );

  return (
    <div className="activity-panel" data-testid="workspace-activity" data-source={source.provenance.kind}>
      <Disclosure summary={summary} count={count} defaultOpen className="activity-root">
        <p className="activity-visually-hidden" role="status" data-testid="activity-status">
          {statusLine}
        </p>
        {offline && (
          <p className="activity-note" role="status" data-testid="activity-offline">
            Activity is {offline.status}: {offline.reason}
          </p>
        )}
        {stopBlockedReason && (
          <p id={stopNoteId} className="activity-note">
            Stop is disabled: {stopBlockedReason}
          </p>
        )}
        {notice && (
          <p ref={noticeRef} tabIndex={-1} role="status" className="activity-note" data-testid="activity-notice">
            {notice}
          </p>
        )}
        {error && <ErrorNotice error={error} action="Stopping the task" />}
        {receipts.length > 0 && <SimulatedReceipts label="Simulated activity actions" receipts={receipts} />}

        <Disclosure summary="Execution queue" count={snapshot ? running.length + queued.length : "—"} defaultOpen className="activity-group" data-testid="activity-queue">
          {fixture && <Placeholder id="D11" />}
          {snapshot ? (
            <>
              {snapshot.degraded?.tasks && (
                <p className="activity-note" data-testid="activity-tasks-degraded">
                  Task details are not available, so objectives and agents are missing and tasks being verified are not listed: {snapshot.degraded.tasks}
                </p>
              )}
              <TaskGroup title="Running" entries={running} empty="No tasks are running." stop={stop} defaultOpen />
              <TaskGroup
                title="Up next"
                entries={upNextEntries}
                empty="No queued task is known to be next to dispatch."
                note={snapshot.upNext.status === "ok" ? undefined : `Dispatch order is not available: ${snapshot.upNext.reason}`}
                stop={stop}
                nextId={nextId}
              />
              <TaskGroup title="Waiting" entries={waitingEntries} empty="No task is waiting on a dependency or approval." stop={stop} />
              <TaskGroup
                title="Unclassified"
                entries={unclassifiedEntries}
                empty="Every queued task has a reported wait reason."
                note="The daemon does not say why these queued tasks are not running or when they will dispatch."
                stop={stop}
              />
            </>
          ) : (
            <p className="activity-empty">Queue not available.</p>
          )}
        </Disclosure>

        <Disclosure summary="Attention" count={badge} defaultOpen className="activity-group" data-testid="activity-attention">
          {fixture && <Placeholder id="D11" />}
          {snapshot ? (
            <>
              <NavLink to={{ name: "global", view: "review" }} className="activity-review-all">
                Review all decisions
              </NavLink>
              {!attentionKnown ? (
                <p className="activity-empty" data-testid="activity-attention-degraded">
                  Attention list not available: {snapshot.degraded?.approvals}
                </p>
              ) : attention.length === 0 ? (
                <EmptyState title="Nothing needs attention." description="Pending approvals appear here." />
              ) : (
                <ul className="activity-list">
                  {attention.map((item) => (
                    <AttentionRow key={item.approvalId} item={item} />
                  ))}
                </ul>
              )}
            </>
          ) : (
            <p className="activity-empty">Attention list not available.</p>
          )}
        </Disclosure>

        <Disclosure
          summary="Agent slots"
          count={snapshot?.slots.status === "ok" ? `${snapshot.slots.value.used} / ${snapshot.slots.value.limit}` : "not available"}
          className="activity-group"
          data-testid="activity-slots"
        >
          {fixture && <Placeholder id="D11" />}
          {snapshot?.slots.status === "ok" ? (
            <p data-testid="activity-slots-value">
              {snapshot.slots.value.used} of {snapshot.slots.value.limit} slots in use ·{" "}
              {Math.max(0, snapshot.slots.value.limit - snapshot.slots.value.used)} available
            </p>
          ) : (
            <p data-testid="activity-slots-value">Slots: not available{slotsReason ? ` (${slotsReason})` : ""}</p>
          )}
          {snapshot?.dispatchPaused.status === "ok" && (
            <p>New dispatch: {snapshot.dispatchPaused.value ? "paused" : "running"}</p>
          )}
          <p data-testid="activity-budget">
            {budget.text} {budget.fixture && <Placeholder id="D12" />}
          </p>
          <div className="activity-controls">
            <ReasonButton
              label="Pause new dispatch"
              reason={pauseReason}
              onClick={async () => {
                const receipt = await source.simulatePauseDispatch();
                setReceipts((all) => [...all, receipt]);
              }}
            />
            <ReasonButton label="Stop menu" reason="stopping everything at once is not supported; stop a single task from the queue." />
            <ReasonButton
              label="Queue settings"
              reason={fixture ? "queue settings are not simulated." : "the daemon does not expose scheduler limits."}
            />
          </div>
        </Disclosure>
      </Disclosure>
    </div>
  );
}
