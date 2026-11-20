import { decisionCountText, matchesFilter, pendingDecisionCount } from "../format";
import { heldSets } from "../../sources/surfaces/holds";
import { useServices, useStore } from "../services";
import type { ShellSlots } from "./slots";
import { NeedYouCount } from "./TopBar";

/**
 * Pending approvals plus pending proposed actions (`approvals.list`, `actions.pending`); "—" (not 0) until both have
 * loaded, and "100+" when the daemon held more proposed actions than it sent.
 */
export function ReviewCount() {
  const { data } = useServices();
  const { approvals, proposals, holds } = useStore(data);
  if (approvals.data === null || proposals.data === null) {
    return (
      <span className="shell-count" aria-label="pending decisions: not available" data-testid="nav-review-count">
        —
      </span>
    );
  }
  // Held decisions are not counted; until the holds have loaded nothing is held, so the count is as before.
  const held = heldSets(holds.data ?? []);
  const count = decisionCountText(pendingDecisionCount(approvals.data, proposals.data, held), proposals.more);
  const approvalCount = pendingDecisionCount(approvals.data, [], held);
  const proposalCount = decisionCountText(pendingDecisionCount([], proposals.data, held), proposals.more);
  return (
    <span
      className="shell-count"
      aria-label={`${count} pending decisions: ${approvalCount} approvals, ${proposalCount} proposed actions`}
      data-testid="nav-review-count"
    >
      {count}
    </span>
  );
}

/**
 * Running tasks from `tasks.list`, the same rule the Agents view's running filter uses. Hidden while the list has not
 * loaded and when none run, so an unknown count is never shown as 0.
 */
export function RunningCount() {
  const { data } = useServices();
  const { tasks, approvals } = useStore(data);
  const list = tasks.data;
  const running = list?.filter((task) => matchesFilter(task, "running", list, approvals.data ?? [])).length;
  if (!running) return null;
  return (
    <span className="shell-count" aria-label={`${running} running`} data-testid="nav-agents-count">
      {running}
    </span>
  );
}

export const defaultShellSlots: ShellSlots = {
  activity: null,
  agentsCount: <RunningCount />,
  reviewCount: <ReviewCount />,
  topUsage: <span data-testid="slot-top-usage">Usage: not available</span>,
  topSearch: (
    <div role="search" data-testid="slot-top-search">
      <input type="search" disabled aria-label="Search" placeholder="Search is not available yet" />
    </div>
  ),
  topNeedYou: <NeedYouCount />,
};
