import { useEffect, useState } from "react";
import { APPROVER_REQUIRED_NOTICE } from "../daemon/errors";
import type { Approval } from "../daemon/types";
import { ActionStatus, useAction } from "../ui/action";
import { Button } from "../ui/theme";
import type { ChangeRequestHistory, ChangeRequestSource } from "./sources";

/**
 * D8: Request changes and feedback. These pieces only talk to the injected ChangeRequestSource; they have no
 * access to the ReviewClient, so a change request can never be sent as an approvals.resolve decision.
 */
export function useChangeRequests(source: ChangeRequestSource, approval: Approval | null) {
  const [history, setHistory] = useState<ChangeRequestHistory | null>(null);
  const action = useAction();
  const { clear } = action;
  const taskId = approval?.action.task_id ?? null;

  useEffect(() => {
    setHistory(null);
    clear();
    if (!taskId) return;
    let cancelled = false;
    source.loadHistory(taskId).then(
      (h) => !cancelled && setHistory(h),
      (e: unknown) => !cancelled && setHistory({ state: "error", reason: e instanceof Error ? e.message : "failed" }),
    );
    return () => {
      cancelled = true;
    };
  }, [source, taskId, clear]);

  /** Resolves true when the request was sent. A refusal keeps the feedback typed, so Retry sends the same text. */
  async function submit(feedback: string): Promise<boolean> {
    if (!approval) return false;
    let sent = false;
    await action.run("Sending…", async () => {
      const result = await source
        .submit({
          taskId: approval.action.task_id,
          approvalId: approval.id,
          candidateCommit: approval.action.candidate_commit,
          feedback,
        })
        .catch((e: unknown) => {
          throw new Error(`Your feedback was not sent. ${e instanceof Error && e.message ? e.message : "Try again."}`);
        });
      sent = true;
      // The history is read after the send; if that read fails, the history says so and the send still counts.
      void source.loadHistory(approval.action.task_id).then(setHistory, (e: unknown) =>
        setHistory({ state: "error", reason: e instanceof Error ? e.message : "failed" }),
      );
      return { kind: "saved", text: `Receipt ${result.requestId}: ${result.resultingState}. ${source.labels.receiptNote}`.trim() };
    });
    return sent;
  }

  return { history, action, submit };
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
  const { action } = requests;
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
          pending={action.pending}
          disabled={!approval || !approver || feedback.trim() === ""}
          aria-describedby={approver ? undefined : "change-request-approver-reason"}
          onClick={() => void requests.submit(feedback).then((sent) => sent && setFeedback(""))}
        >
          {source.labels.submitAction}
        </Button>
        <ActionStatus action={action} />
      </div>
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
