/** Session controls for the selected session: cancel its attempt (real in live mode); resume and checkpoint stay disabled with reasons. */
import { useState } from "react";
import { isFixtureId } from "../../../daemon/api";
import type { DaemonError } from "../../../daemon/errors";
import type { SimulatedReceipt } from "../../../sources/types";
import { TERMINAL_STATES } from "../../format";
import { useGate, useServices } from "../../services";
import { ErrorNotice, SimulatedReceipts } from "../../widgets";
import { Control, DisabledAction, NOT_LINKED_REASON } from "./common";
import type { WorkRow, WorkSession } from "./model";

export function SessionControls({ row, session, live }: { row: WorkRow; session: WorkSession | null; live: boolean }) {
  const { data, sources } = useServices();
  const gate = useGate();
  const [receipts, setReceipts] = useState<SimulatedReceipt[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [failure, setFailure] = useState<{ error: DaemonError; action: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // Fixture ids can never reach a mutation: only a real task in live mode is sent.
  const real = live && row.origin === "real" && !isFixtureId(row.id);
  const attemptNumber = row.attempts.find((attempt) => attempt.id === session?.attemptId)?.number ?? null;
  const attemptText = attemptNumber === null ? "" : ` ${attemptNumber}`;

  const terminal = TERMINAL_STATES.has(row.state);
  const noLink = live && !gate.can("tasks") ? NOT_LINKED_REASON : null;
  const cancelReason = terminal ? `The task is ${row.state}; it can no longer be cancelled.` : noLink;

  const cancel = async () => {
    setNote(null);
    setFailure(null);
    if (!live) {
      void sources.execution.simulateCancel(row.id).then((receipt) => setReceipts((list) => [...list, receipt]));
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

  return (
    <section aria-label="Session controls" className="agents-controls">
      <h4>Session controls</h4>
      <Control
        label={live ? `Cancel attempt${attemptText}` : `Simulate cancel attempt${attemptText}`}
        variant="danger"
        disabled={busy || cancelReason !== null}
        reason={cancelReason}
        onClick={() => void cancel()}
      />
      <DisabledAction
        label="Resume unavailable until capability verified"
        reason="Not available: the daemon has no method to resume a session, whatever an adapter reports for session resumption."
      />
      <DisabledAction
        label="Inspect retained checkpoint"
        reason="Not available: the daemon does not expose retained checkpoints."
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
