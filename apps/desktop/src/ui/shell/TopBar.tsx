import { decisionCountText, matchesFilter, pendingDecisionCount } from "../format";
import { heldSets } from "../../sources/surfaces/holds";
import { useGate, useServices, useStore, useSurface } from "../services";
import { Badge } from "../theme";
import type { GlobalView } from "../router";
import { NavLink } from "./NavLink";
import { TopSearch } from "./Search";
import { UsagePill } from "./UsagePill";
import { useWorkstreamsVersion } from "../views/workstreams/model";
import type { ShellSlots } from "./slots";
import "./topbar.css";

/** A count is a number once observed. Otherwise it is "loading" (an answer may still arrive) or "unavailable"; never 0. */
type Count = number | "loading" | "unavailable";

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

/** `attentionMore`: the daemon held more pending proposed actions than it sent, so the attention count reads "N+". */
function CountsLine({
  values,
  placeholder,
  attentionMore = false,
}: {
  values: CountValues;
  placeholder: string | null;
  attentionMore?: boolean;
}) {
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
              {typeof value === "number" ? (
                `${key === "attention" ? decisionCountText(value, attentionMore) : value} ${label}`
              ) : (
                <>
                  <span aria-hidden="true">— {label}</span>
                  <span className="top-sr">
                    {label}: {value === "loading" ? "loading" : "not available"}
                  </span>
                </>
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

/** A surface that has not answered yet is loading; one that failed or cannot be read is unavailable. */
const surfaceCount = (status: string, value: number | undefined): Count =>
  value ?? (status === "loading" ? "loading" : "unavailable");

/** Same task, approval and proposal lists as the nav, lists and Review count, so the numbers cannot drift. */
function LiveCounts() {
  const { data, surfaces } = useServices();
  const { tasks, approvals, proposals, holds } = useStore(data);
  const gate = useGate();
  const taskList = tasks.data;
  // The daemon emits no workstream events, so D9 re-reads when a workstream mutation lands
  // (the version) or when the set of tasks changes, never on a timer.
  const reloads = useWorkstreamsVersion();
  const membership = taskList?.map((task) => `${task.contract.id}@${task.contract.project_id}`).join("\n") ?? "-";
  const workstreams = useSurface(
    () => surfaces.workstreams.workstreams(),
    `top-counts:${reloads}:${membership}`,
    surfaces.workstreams.provenance,
  );
  const approvalList = approvals.data ?? [];
  // A list nobody has read is loading only while a daemon answer can still arrive.
  const pendingRead = (status: string): Count => (status !== "error" && (gate.connected || gate.settling) ? "loading" : "unavailable");
  return (
    <CountsLine
      placeholder={null}
      attentionMore={proposals.more}
      values={{
        workstreams: surfaceCount(workstreams.status, workstreams.status === "ok" ? workstreams.data.length : undefined),
        tasks: taskList ? taskList.length : pendingRead(tasks.status),
        running: taskList
          ? taskList.filter((task) => matchesFilter(task, "running", taskList, approvalList)).length
          : pendingRead(tasks.status),
        // Pending approvals plus pending proposed actions, the number the Review nav and the activity badge show.
        attention:
          approvals.data && proposals.data
            ? pendingDecisionCount(approvals.data, proposals.data, heldSets(holds.data ?? []))
            : pendingRead(approvals.data ? proposals.status : approvals.status),
      }}
    />
  );
}

/** Fixture demo mode: the store is empty, so every count comes from the labeled D9/D11 fixtures. */
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
        workstreams: surfaceCount(workstreams.status, ws?.length),
        tasks: surfaceCount(workstreams.status, ws?.reduce((total, item) => total + item.tasks.length, 0)),
        running: surfaceCount(activity.status, act?.running.length),
        attention: surfaceCount(activity.status, act?.attention.length),
      }}
    />
  );
}

/**
 * Portfolio counts (workstreams, tasks, running, require attention). With a daemon they
 * come from the same loaded lists the rest of the shell reads and update with its events;
 * in fixture mode they come from the PLACEHOLDER-labeled D9/D11 fixtures. Unloaded values
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
