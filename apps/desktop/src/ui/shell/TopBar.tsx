import { matchesFilter, pendingApprovals } from "../format";
import { useServices, useStore, useSurface } from "../services";
import { Badge } from "../theme";
import type { GlobalView } from "../router";
import { NavLink } from "./NavLink";
import { TopSearch } from "./Search";
import { UsagePill } from "./UsagePill";
import type { ShellSlots } from "./slots";
import "./topbar.css";

/** `undefined` means "not loaded or not available"; it is never rendered as 0. */
type Count = number | undefined;

interface CountValues {
  workstreams: Count;
  tasks: Count;
  running: Count;
  attention: Count;
}

const ITEMS: Array<{ key: keyof CountValues; label: string; view: GlobalView }> = [
  { key: "workstreams", label: "workstreams", view: "workstreams" },
  { key: "tasks", label: "tasks", view: "agents" },
  { key: "running", label: "running", view: "agents" },
  { key: "attention", label: "require attention", view: "review" },
];

function CountsLine({ values, placeholder }: { values: CountValues; placeholder: string | null }) {
  return (
    <p className="top-counts" data-testid="top-counts">
      {ITEMS.map(({ key, label, view }, index) => {
        const value = values[key];
        return (
          <span key={key}>
            {index > 0 && (
              <span className="top-counts-sep" aria-hidden="true">
                {"· "}
              </span>
            )}
            <NavLink to={{ name: "global", view }}>
              {value === undefined ? (
                <>
                  <span aria-hidden="true">— {label}</span>
                  <span className="top-sr">{label}: not available</span>
                </>
              ) : (
                `${value} ${label}`
              )}
            </NavLink>
          </span>
        );
      })}
      {placeholder && (
        <Badge tone="placeholder" data-testid="top-counts-placeholder">
          PLACEHOLDER DATA ({placeholder})
        </Badge>
      )}
    </p>
  );
}

/** Same task and approval lists as the nav, lists and Review count, so the numbers cannot drift. */
function LiveCounts() {
  const { data, surfaces } = useServices();
  const { projects, tasks, approvals } = useStore(data);
  const taskList = tasks.data;
  // Workstreams (D9) change only when projects or task-to-project membership change.
  const membership = `${projects.data?.map((project) => project.id).join("\n") ?? "-"}|${
    taskList?.map((task) => `${task.contract.id}@${task.contract.project_id}`).join("\n") ?? "-"
  }`;
  const workstreams = useSurface(() => surfaces.workstreams.workstreams(), `top-counts:${membership}`, surfaces.workstreams.provenance);
  const approvalList = approvals.data ?? [];
  return (
    <CountsLine
      placeholder={null}
      values={{
        workstreams: workstreams.status === "ok" ? workstreams.data.length : undefined,
        tasks: taskList?.length,
        running: taskList?.filter((task) => matchesFilter(task, "running", taskList, approvalList)).length,
        attention: approvals.data ? pendingApprovals(approvals.data).length : undefined,
      }}
    />
  );
}

/** Fixture demo mode: the store is empty, so every count comes from the labelled D9/D11 fixtures. */
function FixtureCounts() {
  const { surfaces } = useServices();
  const workstreams = useSurface(() => surfaces.workstreams.workstreams(), "top-counts", surfaces.workstreams.provenance);
  const activity = useSurface(() => surfaces.activity.activity(), "top-counts", surfaces.activity.provenance);
  const ws = workstreams.status === "ok" ? workstreams.data : null;
  const act = activity.status === "ok" ? activity.data : null;
  return (
    <CountsLine
      placeholder="D9, D11"
      values={{
        workstreams: ws?.length,
        tasks: ws?.reduce((total, item) => total + item.tasks.length, 0),
        running: act?.running.length,
        attention: act?.attention.length,
      }}
    />
  );
}

/**
 * Portfolio counts (workstreams, tasks, running, require attention). With a daemon they
 * come from the same loaded lists the rest of the shell reads and update with its events;
 * in fixture mode they come from the PLACEHOLDER-labelled D9/D11 fixtures. Unloaded values
 * read "—", never 0.
 */
export function TopBarCounts() {
  const { surfaces } = useServices();
  return surfaces.workstreams.provenance.kind === "fixture" ? <FixtureCounts /> : <LiveCounts />;
}

/** The three top-bar slots; pass as `shellSlots` to `<App>` (see REGISTRATION.md). */
export const topBarSlots: Pick<ShellSlots, "topUsage" | "topSearch" | "topCounts"> = {
  topUsage: <UsagePill />,
  topSearch: <TopSearch />,
  topCounts: <TopBarCounts />,
};

export { TopSearch, UsagePill };
