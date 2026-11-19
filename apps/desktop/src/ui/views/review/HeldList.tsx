// The Held tab's rows (DPES-1105 Phase 4d): one per hold in force. A hold never approves, rejects or expires a decision,
// so "Review now" stays available and Release ends the hold early.
import type { Approval } from "../../../daemon/types";
import type { HoldItem, ProposalItem } from "../../../sources/surfaces/types";
import { Badge, Button, EmptyState, Pill } from "../../theme";
import { cardTitle } from "../workspace/actionCardModel";
import { HOLD_REASON_ID } from "./HoldControl";
import { heldUntilText } from "./holdDates";
import type { HoldsModel } from "./useHolds";

interface HeldListProps {
  holds: HoldsModel;
  approvals: readonly Approval[];
  proposals: readonly ProposalItem[];
  onReview: (approvalId: string) => void;
  /** Names the row a change started from, so focus can come back to it. */
  onStart: (rowKey: string) => void;
}

/** What the row is about, in words, and the approval "Review now" opens (null for a held action card). */
function subjectOf(hold: HoldItem, approvals: readonly Approval[], proposals: readonly ProposalItem[]) {
  if (hold.subjectKind === "approval") {
    const approval = approvals.find((item) => item.action.task_id === hold.subjectId && item.state.state === "requested");
    return { label: `Task ${hold.subjectId}`, sub: "Pending approval", review: approval?.id ?? null };
  }
  const item = proposals.find((candidate) => candidate.proposal.id === hold.subjectId);
  return {
    label: item === undefined ? `Proposed action ${hold.subjectId}` : cardTitle(item.proposal.kind, item.proposal.params),
    sub: "Proposed action waiting for your decision",
    review: null,
  };
}

export function HeldList({ holds, approvals, proposals, onReview, onStart }: HeldListProps) {
  if (holds.unavailable !== null) {
    return <EmptyState title="Held decisions are not available" description={holds.unavailable} />;
  }
  if (holds.items.length === 0) {
    return holds.loading ? (
      <p className="rd-note">Loading held decisions…</p>
    ) : (
      <EmptyState title="Nothing is held" description="Hold a decision from the Actionable tab and it waits here." />
    );
  }
  return (
    <ul className="rd-rows" aria-label="Held decisions">
      {holds.items.map((hold) => {
        const subject = subjectOf(hold, approvals, proposals);
        const review = subject.review;
        const rowKey = `hold:${hold.id}`;
        return (
          <li key={hold.id} className="rd-row-item rd-held-row" data-row-key={rowKey} tabIndex={-1}>
            <span className="rd-row-title">
              {subject.label}
              <span>
                <Pill tone="warn">Held</Pill>
                {holds.fixture && <Badge tone="placeholder">Fixture data</Badge>}
              </span>
            </span>
            <span className="rd-row-sub">
              {heldUntilText(hold.untilDate)} · {subject.sub}
            </span>
            <span className="rd-held-actions">
              {review !== null && (
                <Button variant="quiet" aria-label={`Review now: ${subject.label}`} onClick={() => onReview(review)}>
                  Review now
                </Button>
              )}
              <Button
                aria-label={`Release: ${subject.label}`}
                aria-describedby={holds.blocked !== null ? HOLD_REASON_ID : undefined}
                disabled={holds.blocked !== null || holds.busy !== null}
                onClick={() => {
                  onStart(rowKey);
                  void holds.release(hold, subject.label);
                }}
              >
                Release
              </Button>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
