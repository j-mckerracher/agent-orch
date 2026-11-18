import { decisionCountText, matchesFilter, openTaskCount, pendingDecisionCount } from "../format";
import { heldSets } from "../../sources/surfaces/holds";
import { useServices, useStore } from "../services";
import { ThemeToggle } from "../theme";
import { NavLink } from "./NavLink";
import { ResourcesSlot } from "./ResourcesSlot";
import type { ShellSlots } from "./slots";

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

/** Counts derived from `tasks.list`, `approvals.list` and `actions.pending`; "—" where the list has not loaded. */
export function TopBarCounts() {
  const { data } = useServices();
  const { tasks, approvals, proposals, holds } = useStore(data);
  const taskList = tasks.data;
  const running = taskList?.filter((task) => matchesFilter(task, "running", taskList, approvals.data ?? [])).length;
  const attention =
    approvals.data && proposals.data ? decisionCountText(pendingDecisionCount(approvals.data, proposals.data, heldSets(holds.data ?? [])), proposals.more)
      : undefined;
  const text = (value: number | string | undefined) => (value === undefined ? "—" : String(value));
  return (
    <p className="shell-counts" data-testid="top-counts">
      <NavLink to={{ name: "global", view: "workstreams" }}>— workstreams</NavLink>
      <span aria-hidden="true"> · </span>
      <NavLink to={{ name: "global", view: "agents" }}>{text(taskList ? openTaskCount(taskList) : undefined)} tasks</NavLink>
      <span aria-hidden="true"> · </span>
      <NavLink to={{ name: "global", view: "agents" }}>{text(running)} running</NavLink>
      <span aria-hidden="true"> · </span>
      <NavLink to={{ name: "global", view: "review" }}>{text(attention)} require attention</NavLink>
    </p>
  );
}

const placeholder = (testId: string, text: string) => (
  <p className="shell-placeholder" data-testid={testId}>
    {text}
  </p>
);

export const defaultShellSlots: ShellSlots = {
  reviewCount: <ReviewCount />,
  workstreamsCount: null,
  workstreamsTree: placeholder("slot-workstreams-tree", "The workstream list is not available yet."),
  themeToggle: <ThemeToggle />,
  topUsage: <span data-testid="slot-top-usage">Usage: not available</span>,
  topSearch: (
    <div role="search" data-testid="slot-top-search">
      <input type="search" disabled aria-label="Search" placeholder="Search is not available yet" />
    </div>
  ),
  topCounts: <TopBarCounts />,
  activity: (
    <div data-testid="slot-activity">
      <h2>Workspace activity</h2>
      {placeholder("slot-activity-note", "Not available yet.")}
      <ResourcesSlot />
    </div>
  ),
  resources: placeholder("slot-resources", "Device resources: not available (no metrics source is connected)."),
};
