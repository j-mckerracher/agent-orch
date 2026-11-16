/** Task controls: reassign through a new attempt (tasks.retry); amend and decomposition stay disabled with reasons. */
import { useState } from "react";
import { isFixtureId } from "../../../daemon/api";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { SimulatedReceipt } from "../../../sources/types";
import { useGate, useServices, useSource } from "../../services";
import { ErrorNotice, SimulatedReceipts } from "../../widgets";
import { Control, DisabledAction, NOT_LINKED_REASON } from "./common";
import { taskRef, type WorkRow } from "./model";

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

  const noLink = live && !gate.can("tasks") ? NOT_LINKED_REASON : null;

  const reassign = async () => {
    setNote(null);
    setFailure(null);
    if (!live) {
      void sources.retry
        .simulateRetry(ref)
        .then((r) => addReceipt({ ...r, notice: `${r.notice} (new attempt ${r.newAttemptId})` }));
      return;
    }
    if (!realRetry) return;
    setBusy(true);
    try {
      await realRetry.call(sources.retry, ref);
      setNote("The request for a new attempt was accepted by the daemon.");
      void data.watchTask(row.id);
      void data.refreshAll();
    } catch (error) {
      setFailure({ error: asDaemonError(error), action: "Reassigning the task" });
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
  if (retryOffered && live && !realRetry) {
    retryOffered = false;
    retryReason = "Not available: this task cannot be retried through the daemon.";
  }

  return (
    <section aria-label="Task controls" className="agents-controls">
      <h4>Task controls</h4>
      <Control
        label={live ? "Reassign via new attempt" : "Preview reassign via new attempt"}
        disabled={busy || !retryOffered || noLink !== null}
        reason={retryReason}
        onClick={() => void reassign()}
      />
      <DisabledAction
        label="Amend future TaskContract"
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
