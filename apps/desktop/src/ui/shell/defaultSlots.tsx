import { matchesFilter, pendingApprovals } from "../format";
import { useServices, useStore } from "../services";
import { NavLink } from "./NavLink";
import type { ShellSlots } from "./slots";

/** Pending approvals from `approvals.list`; "—" (not 0) when no list has loaded. */
export function ReviewCount() {
  const { data } = useServices();
  const { approvals } = useStore(data);
  if (approvals.data === null) {
    return (
      <span className="shell-count" aria-label="pending approvals: not available" data-testid="nav-review-count">
        —
      </span>
    );
  }
  const count = pendingApprovals(approvals.data).length;
  return (
    <span className="shell-count" aria-label={`${count} pending approvals`} data-testid="nav-review-count">
      {count}
    </span>
  );
}

/** Counts derived from `tasks.list` and `approvals.list`; "—" where the list has not loaded. */
export function TopBarCounts() {
  const { data } = useServices();
  const { tasks, approvals } = useStore(data);
  const taskList = tasks.data;
  const running = taskList?.filter((task) => matchesFilter(task, "running", taskList, approvals.data ?? [])).length;
  const attention = approvals.data ? pendingApprovals(approvals.data).length : undefined;
  const text = (value: number | undefined) => (value === undefined ? "—" : String(value));
  return (
    <p className="shell-counts" data-testid="top-counts">
      <NavLink to={{ name: "global", view: "workstreams" }}>— workstreams</NavLink>
      <span aria-hidden="true"> · </span>
      <NavLink to={{ name: "global", view: "agents" }}>{text(taskList?.length)} tasks</NavLink>
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
  themeToggle: null,
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
    </div>
  ),
  resources: placeholder("slot-resources", "Device resources: not available (no metrics source is connected)."),
};
