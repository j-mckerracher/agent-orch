import { useEffect, useState } from "react";
import type { Approval } from "../daemon/types";
import { PlaceholderTag } from "./EvidencePanel";
import type { ChangeRequestHistory, ChangeRequestReceipt, ChangeRequestSource } from "./sources";

/**
 * D8: Request changes and feedback. This panel only talks to the injected
 * ChangeRequestSource; it has no access to the ReviewClient, so a change
 * request can never be sent as an approvals.resolve decision.
 */
export function ChangeRequestPanel({ source, approval }: { source: ChangeRequestSource; approval: Approval | null }) {
  const [history, setHistory] = useState<ChangeRequestHistory | null>(null);
  const [open, setOpen] = useState(false);
  const [feedback, setFeedback] = useState("");
  const [busy, setBusy] = useState(false);
  const [receipt, setReceipt] = useState<ChangeRequestReceipt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const taskId = approval?.action.task_id ?? null;
  const fixture = source.provenance === "fixture";

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

  async function submit() {
    if (!approval) return;
    setBusy(true);
    setError(null);
    try {
      const result = await source.submit({
        taskId: approval.action.task_id,
        candidateCommit: approval.action.candidate_commit,
        feedback,
      });
      setReceipt(result);
      setFeedback("");
      setOpen(false);
      setHistory(await source.loadHistory(approval.action.task_id));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request changes failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label="Request changes" className="panel">
      <h3>
        Request changes <PlaceholderTag show={fixture} />
      </h3>
      <p>{source.labels.description}</p>
      <button type="button" disabled={!approval || busy} onClick={() => setOpen((o) => !o)}>
        Request changes
      </button>
      {open && (
        <div>
          <label>
            Feedback <PlaceholderTag show={fixture} />
            <textarea value={feedback} onChange={(e) => setFeedback(e.target.value)} rows={3} />
          </label>
          <button type="button" disabled={busy || feedback.trim() === ""} onClick={() => void submit()}>
            {source.labels.submitAction}
          </button>
        </div>
      )}
      {error && (
        <p role="alert">
          {error} <PlaceholderTag show={fixture} />
        </p>
      )}
      {receipt && (
        <p role="status">
          Receipt {receipt.requestId}: {receipt.resultingState}. {source.labels.receiptNote} <PlaceholderTag show={receipt.provenance === "fixture"} />
        </p>
      )}
      {history?.state === "available" && (
        <ul aria-label="Change request history">
          {history.items.map((item) => (
            <li key={item.requestId}>
              {item.requestId}: {item.feedback} ({item.resultingState}) <PlaceholderTag show={item.provenance === "fixture"} />
            </li>
          ))}
        </ul>
      )}
      {history && history.state !== "available" && (
        <p>
          Change request history {history.state}: {history.reason} <PlaceholderTag show={fixture} />
        </p>
      )}
    </section>
  );
}
