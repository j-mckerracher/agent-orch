import { useEffect, useState } from "react";
import { APPROVER_REQUIRED_NOTICE } from "../daemon/errors";
import type { Approval } from "../daemon/types";
import { Button } from "../ui/theme";
import type { ChangeRequestHistory, ChangeRequestReceipt, ChangeRequestSource } from "./sources";

/**
 * D8: Request changes and feedback. These pieces only talk to the injected ChangeRequestSource; they have no
 * access to the ReviewClient, so a change request can never be sent as an approvals.resolve decision.
 */
export function useChangeRequests(source: ChangeRequestSource, approval: Approval | null) {
  const [history, setHistory] = useState<ChangeRequestHistory | null>(null);
  const [receipt, setReceipt] = useState<ChangeRequestReceipt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const taskId = approval?.action.task_id ?? null;

  useEffect(() => {
    setHistory(null);
    setReceipt(null);
    setError(null);
    if (!taskId) return;
    let cancelled = false;
    source.loadHistory(taskId).then(
      (h) => !cancelled && setHistory(h),
      (e: unknown) => !cancelled && setHistory({ state: "error", reason: e instanceof Error ? e.message : "failed" }),
    );
    return () => {
      cancelled = true;
    };
  }, [source, taskId]);

  /** Resolves true when the request was sent. */
  async function submit(feedback: string): Promise<boolean> {
    if (!approval) return false;
    setBusy(true);
    setError(null);
    try {
      const result = await source.submit({
        taskId: approval.action.task_id,
        approvalId: approval.id,
        candidateCommit: approval.action.candidate_commit,
        feedback,
      });
      setReceipt(result);
      setHistory(await source.loadHistory(approval.action.task_id));
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request changes failed.");
      return false;
    } finally {
      setBusy(false);
    }
  }

  return { history, receipt, error, busy, submit };
}

export type ChangeRequests = ReturnType<typeof useChangeRequests>;

export function RequestChangesForm({
  source,
  requests,
  approval,
  approver = true,
}: {
  source: ChangeRequestSource;
  requests: ChangeRequests;
  approval: Approval | null;
  /** False when this session did not prove the approver secret; sending needs it (DPES-1105). */
  approver?: boolean;
}) {
  const [feedback, setFeedback] = useState("");
  const { receipt, error, busy } = requests;
  return (
    <section aria-label="Request changes" className="rv-panel">
      {!approver && (
        <p id="change-request-approver-reason" className="rd-reason">
          {APPROVER_REQUIRED_NOTICE}
        </p>
      )}
      <label className="rv-field">
        Feedback
        <textarea value={feedback} onChange={(e) => setFeedback(e.target.value)} rows={4} />
      </label>
      <div>
        <Button
          disabled={!approval || busy || !approver || feedback.trim() === ""}
          aria-describedby={approver ? undefined : "change-request-approver-reason"}
          onClick={() => void requests.submit(feedback).then((sent) => sent && setFeedback(""))}
        >
          {source.labels.submitAction}
        </Button>
      </div>
      {error && <p role="alert">{error}</p>}
      {receipt && (
        <p role="status">
          Receipt {receipt.requestId}: {receipt.resultingState}. {source.labels.receiptNote}
        </p>
      )}
    </section>
  );
}

export function ChangeRequestHistoryList({ requests }: { requests: ChangeRequests }) {
  const { history } = requests;
  if (history === null) return <p role="status">Loading history…</p>;
  if (history.state === "empty") return <p>No changes were requested yet.</p>;
  if (history.state !== "available") {
    return (
      <p>
        Change request history {history.state}: {history.reason}
      </p>
    );
  }
  if (history.items.length === 0) return <p>No changes were requested yet.</p>;
  return (
    <ul aria-label="Change request history">
      {history.items.map((item) => (
        <li key={item.requestId}>
          {item.requestId}: {item.feedback} ({item.resultingState})
        </li>
      ))}
    </ul>
  );
}
