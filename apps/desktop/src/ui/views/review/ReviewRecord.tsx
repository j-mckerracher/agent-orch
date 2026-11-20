// The record under a candidate's Technical details: the six facts, attempts and tokens, and audit transitions.
import { evidenceBlockers, type ActionContext } from "../../../review/binding";
import type { ReviewState } from "../../../review/useReview";
import { Attempts, type AttemptsLoader } from "./Attempts";
import { SixFacts, deriveFacts } from "./SixFacts";

/** Only `approval`, `evidence` and `diff` read by evidenceBlockers; rest is inert here. */
function bindingContext(state: ReviewState): ActionContext {
  return {
    connected: true,
    // Only the evidence blockers read this context, so the approver role is not checked here.
    approver: true,
    loading: false,
    stale: false,
    refreshRequired: false,
    approval: state.approval,
    task: state.task,
    diff: state.diff,
    evidence: state.evidence,
  };
}

export function SixFactsRecord({ state, loadAttempts }: { state: ReviewState; loadAttempts?: AttemptsLoader }) {
  if (!state.approval) return null;
  return (
    <>
      <SixFacts
        facts={deriveFacts({
          approval: state.approval,
          task: state.task,
          evidence: state.evidence,
          delivery: state.mutation.delivery,
          evidenceProblems: state.loading ? [] : evidenceBlockers(bindingContext(state)),
        })}
      />
      {loadAttempts && <Attempts taskId={state.approval.action.task_id} approvalId={state.approval.id} load={loadAttempts} />}
      <section aria-label="Task audit transitions" className="rd-audit">
        <h3>Task audit transitions</h3>
        {state.task === null ? (
          <p className="rd-note">The task record is not available, so no transitions are listed.</p>
        ) : !state.task.events || state.task.events.length === 0 ? (
          <p className="rd-note">The daemon has recorded no transitions for this task.</p>
        ) : (
          <ol aria-label="Task audit transitions" className="rd-events">
            {state.task.events.map((event, index) => (
              <li key={`${index}-${event}`}>{event}</li>
            ))}
          </ol>
        )}
      </section>
    </>
  );
}
