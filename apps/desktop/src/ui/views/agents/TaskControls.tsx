/** Task and session controls. Cancel and Retry follow the accepted TaskDetail; the rest are disabled with reasons. */
import { useId, useState } from "react";
import { isFixtureId } from "../../../daemon/api";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { SimulatedReceipt } from "../../../sources/types";
import { TERMINAL_STATES } from "../../format";
import { useGate, useServices, useSource } from "../../services";
import { Button } from "../../theme";
import { ErrorNotice, SimulatedReceipts } from "../../widgets";
import { DisabledAction } from "./common";
import { taskRef, type WorkRow } from "./model";

function Control({
  label,
  disabled,
  reason,
  variant,
  onClick,
}: {
  label: string;
  disabled: boolean;
  /** Shown beside the button whenever it explains something: always when disabled. */
  reason: string | null;
  variant?: "danger" | "primary" | "secondary";
  onClick: () => void;
}) {
  const id = useId();
  return (
    <div className="agents-action">
      <Button variant={variant} disabled={disabled} aria-describedby={reason ? id : undefined} onClick={onClick}>
        {label}
      </Button>
      {reason && (
        <span id={id} className="agents-reason">
          {reason}
        </span>
      )}
    </div>
  );
}

export function TaskControls({ row, live }: { row: WorkRow; live: boolean }) {
  const { data, sources } = useServices();
  const gate = useGate();
  const [receipts, setReceipts] = useState<SimulatedReceipt[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [failure, setFailure] = useState<{ error: DaemonError; action: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const ref = taskRef(row);
  // Fixture ids can never reach a mutation: only a real task in live mode is sent.
  const real = live && row.origin === "real" && !isFixtureId(row.id);
  const realRetry = real ? sources.retry.retry : undefined;
  const retryInfo = useSource(() => sources.retry.retryFor(ref), `agents-retry:${row.id}:${row.state}`, sources.retry.provenance);
  const addReceipt = (receipt: SimulatedReceipt) => setReceipts((list) => [...list, receipt]);

  const terminal = TERMINAL_STATES.has(row.state);
  const noLink = live && !gate.can("tasks") ? "Not connected, or the tasks capability was not negotiated." : null;
  const cancelReason = terminal ? `The task is ${row.state}; it can no longer be cancelled.` : noLink;

  const cancel = async () => {
    setNote(null);
    setFailure(null);
    if (!live) {
      void sources.execution.simulateCancel(row.id).then(addReceipt);
      return;
    }
    if (!real) return;
    setBusy(true);
    const result = await data.cancelTask(row.id);
    setBusy(false);
    if (result.ok) {
      setNote(
        `Cancel request acknowledged; the daemon now records the task as ${result.value.state}. This is not proof that any process has stopped.`,
      );
    } else {
      setFailure({ error: result.error, action: "Cancelling the task" });
    }
  };

  const retry = async () => {
    setNote(null);
    setFailure(null);
    if (!live) {
      void sources.retry.simulateRetry(ref).then((r) => addReceipt({ ...r, notice: `${r.notice} (new attempt ${r.newAttemptId})` }));
      return;
    }
    if (!realRetry) return;
    setBusy(true);
    try {
      const result = await realRetry.call(sources.retry, ref);
      setNote(`Retry accepted; the daemon now records the task as ${result.state} with ${result.attempts} attempt(s).`);
      void data.watchTask(row.id);
      void data.refreshAll();
    } catch (error) {
      setFailure({ error: asDaemonError(error), action: "Retrying the task" });
    } finally {
      setBusy(false);
    }
  };

  let retryReason: string | null;
  let retryOffered = false;
  switch (retryInfo.state) {
    case "loading":
      retryReason = "Checking whether a retry is possible…";
      break;
    case "ready":
      retryOffered = retryInfo.data.available;
      retryReason = retryInfo.data.reason;
      break;
    case "empty":
      retryReason = "No retry information is available.";
      break;
    case "unavailable":
      retryReason = retryInfo.reason;
      break;
    case "error":
      retryReason = retryInfo.message;
      break;
  }
  if (retryOffered && noLink) retryReason = noLink;

  return (
    <section aria-label="Task controls" className="agents-controls">
      <h4>Task controls</h4>
      <Control
        label={live ? "Request cancel" : "Simulate cancel"}
        variant="danger"
        disabled={busy || cancelReason !== null}
        reason={cancelReason}
        onClick={() => void cancel()}
      />
      <Control
        label={live ? "Retry task" : "Preview retry"}
        disabled={busy || !retryOffered || noLink !== null}
        reason={retryReason}
        onClick={() => void retry()}
      />
      <DisabledAction
        label="Amend TaskContract"
        reason="Not available: the daemon has no method to amend a task contract."
      />
      <DisabledAction
        label="Request new decomposition"
        reason="Not available: the daemon has no method to request a new decomposition."
      />
      {note && <p role="status">{note}</p>}
      {failure && <ErrorNotice error={failure.error} action={failure.action} />}
      {failure?.error.indeterminate && (
        <p role="status">The outcome is unknown. Trying again reuses the same request key.</p>
      )}
      {receipts.length > 0 && <SimulatedReceipts label={`Simulated receipts for ${row.id}`} receipts={receipts} />}
    </section>
  );
}
