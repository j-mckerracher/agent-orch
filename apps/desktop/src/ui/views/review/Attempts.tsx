// Read-only "Attempts" block: per session, the model/effort the contract requested beside the model the
// usage receipts reported. A figure no receipt reported reads "not reported"; it is never drawn as zero.
import { useEffect, useState } from "react";
import type { ReviewAttemptWire } from "../../../daemon/types";

export type AttemptsLoader = (taskId: string, approvalId: string) => Promise<ReviewAttemptWire[]>;

const NOT_REPORTED = "not reported";

const COVERAGE: Record<ReviewAttemptWire["coverage"], string> = {
  reported: "Usage reported",
  partial: "Usage partly reported",
  unknown: "Usage unknown",
};

export function requestedText(attempt: ReviewAttemptWire): string {
  const { requested } = attempt;
  if (!requested || (!requested.model && !requested.effort)) return "Requested: not set";
  return `Requested: model ${requested.model ?? "not set"}, effort ${requested.effort ?? "not set"}`;
}

export function reportedText(attempt: ReviewAttemptWire): string {
  return `Reported by usage receipt: model ${attempt.reported_model ?? NOT_REPORTED}`;
}

const tokens = (value: number | null) => (value === null ? NOT_REPORTED : value.toLocaleString("en-US"));

export function tokensText(attempt: ReviewAttemptWire): string {
  const u = attempt.usage;
  return (
    `Tokens: input ${tokens(u.input_tokens)}, cache read ${tokens(u.cache_read_tokens)}, ` +
    `cache write ${tokens(u.cache_write_tokens)}, output ${tokens(u.output_tokens)}, reasoning ${tokens(u.reasoning_tokens)}`
  );
}

export function costText(attempt: ReviewAttemptWire): string {
  if (!attempt.cost) return `Cost: ${NOT_REPORTED}`;
  const amount = attempt.cost.amount_nanos / 1e9;
  return `Cost: ${amount.toFixed(amount < 1 ? 6 : 2)} ${attempt.cost.currency} (money, not tokens)`;
}

export function AttemptsList({ attempts }: { attempts: readonly ReviewAttemptWire[] }) {
  return (
    <section aria-labelledby="rd-attempts-title" className="rd-card rd-attempts" data-testid="review-attempts">
      <h3 id="rd-attempts-title">Attempts</h3>
      <p className="rd-note">
        Requested values come from the task contract. Reported values come from usage receipts. Neither is a model
        the daemon resolved.
      </p>
      {attempts.length === 0 ? (
        <p className="rd-note">This task has no sessions.</p>
      ) : (
        <ol className="rd-attempt-list">
          {attempts.map((attempt) => (
            <li key={attempt.session_id} className="rd-attempt" data-session={attempt.session_id}>
              <strong>
                Session {attempt.session_id} ({attempt.agent})
              </strong>{" "}
              <span>{COVERAGE[attempt.coverage]}</span>
              <div>{requestedText(attempt)}</div>
              <div>{reportedText(attempt)}</div>
              <div>{tokensText(attempt)}</div>
              <div>{costText(attempt)}</div>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

/** Loads the attempts once per approval; renders nothing while loading or when the daemon cannot answer. */
export function Attempts({ taskId, approvalId, load }: { taskId: string; approvalId: string; load: AttemptsLoader }) {
  const [attempts, setAttempts] = useState<ReviewAttemptWire[] | null>(null);
  useEffect(() => {
    let live = true;
    setAttempts(null);
    load(taskId, approvalId).then(
      (rows) => live && setAttempts(rows),
      () => live && setAttempts(null),
    );
    return () => {
      live = false;
    };
  }, [taskId, approvalId, load]);
  return attempts === null ? null : <AttemptsList attempts={attempts} />;
}
