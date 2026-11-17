// Six-fact completion record for the selected candidate. Shared by the global Review & Decisions
// view and the project review route so both derive the same facts from the same review state.
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

export function SixFactsRecord({
  state,
  fixture,
  loadAttempts,
}: {
  state: ReviewState;
  fixture: boolean;
  loadAttempts?: AttemptsLoader;
}) {
  if (!state.approval) return null;
  return (
    <>
      <SixFacts
        fixture={fixture}
        facts={deriveFacts({
          approval: state.approval,
          task: state.task,
          evidence: state.evidence,
          delivery: state.mutation.delivery,
          evidenceProblems: state.loading ? [] : evidenceBlockers(bindingContext(state)),
        })}
      />
      {loadAttempts && <Attempts taskId={state.approval.action.task_id} approvalId={state.approval.id} load={loadAttempts} />}
    </>
  );
}
