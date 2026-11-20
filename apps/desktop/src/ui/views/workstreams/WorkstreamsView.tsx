import { useEffect, useId, useState, type ReactNode } from "react";
import { basename } from "../../format";
import { useServices, useStore } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { formatAge } from "../../shell/ResourceStrip";
import { Disclosure, EmptyState, Pill } from "../../theme";
import { PlaceholderBadge } from "../../widgets";
import { WorkstreamDetail } from "./WorkstreamDetail";
import { ARCHIVED_REASON, ListMenu, OutcomeNotice, useMutationRunner, type Controls } from "./Manage";
import {
  progressOf,
  reloadWorkstreams,
  taskCount,
  taskListNotice,
  useWorkstreamModel,
  workstreamStatus,
  type WsGroup,
} from "./model";
import { WsTabs } from "./WsTabs";
import "./workstreams.css";

const STALE_REASON = "Disabled: the connection was lost. Changes resume when the daemon is reachable again.";
const UNREAD_REASON = "Disabled: this workstream's details could not be read. Use Refresh to read them again.";
const AGE_TICK_MS = 5_000;

type Status = "active" | "suspended" | "archived";
const TABS: ReadonlyArray<{ id: Status; label: string }> = [
  { id: "active", label: "Active" },
  { id: "suspended", label: "Suspended" },
  { id: "archived", label: "Archived" },
];

/** Re-renders on an interval so "Updated N s ago" keeps moving between daemon reads. */
function useNow(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), AGE_TICK_MS);
    return () => clearInterval(timer);
  }, []);
  return now;
}

/** When the daemon last answered for this workstream: the observation time of its D9 counts. */
function updatedAt(group: WsGroup): { observedAt: string; ageMs: number } | null {
  return group.counts.status === "ok" ? { observedAt: group.counts.observedAt, ageMs: group.counts.ageMs } : null;
}

function WorkstreamRow({ group, now, onOpen }: { group: WsGroup; now: number; onOpen: () => void }) {
  const status = workstreamStatus(group);
  const progress = progressOf(group);
  const updated = updatedAt(group);
  const projects = group.projectIds.map(basename);
  return (
    <li>
      <button type="button" className="ws-row-card" onClick={onOpen} data-testid="ws-row">
        <span className="ws-dot" data-tone={status.tone} aria-hidden="true" />
        <span className="ws-row-main">
          <span className="ws-row-name">{group.title}</span>
          <small>{projects.length > 3 ? `${projects.length} projects` : projects.join(", ")}</small>
        </span>
        <Pill tone={status.tone}>{status.label}</Pill>
        <span className="ws-progress">
          {progress === null ? (
            "Tasks unknown"
          ) : (
            <>
              <span>
                {progress.done} of {taskCount(progress.total)} done
              </span>
              <span className="ws-bar" aria-hidden="true">
                <i style={{ width: `${progress.total === 0 ? 0 : (progress.done / progress.total) * 100}%` }} />
              </span>
            </>
          )}
        </span>
        <span className="ws-row-updated">
          {updated ? `Updated ${formatAge(updated.observedAt, updated.ageMs, now)}` : "Not read"}
        </span>
        <span aria-hidden="true">›</span>
      </button>
    </li>
  );
}

/** The `#/workstreams` global view (OX-S4). Register it through `register.tsx`. */
export function WorkstreamsView() {
  const model = useWorkstreamModel();
  const { surfaces, router } = useServices();
  const headingId = useId();
  const mutations = useMutationRunner();
  const now = useNow();
  // A route that names a workstream opens it on first render and whenever navigation names one (DPES-1105).
  const route = useStore(router);
  const routeWorkstream = route.name === "global" ? route.workstreamId : undefined;
  const [selected, setSelected] = useState<string | null>(routeWorkstream ?? null);
  useEffect(() => {
    if (route.name === "global" && route.workstreamId !== undefined) setSelected(route.workstreamId);
  }, [route]);
  const [filter, setFilter] = useState("");
  const live = surfaces.workstreams.provenance.kind !== "fixture";

  const menu = (
    <ListMenu
      run={mutations.run}
      busy={mutations.busy}
      live={live}
      onRefresh={() => {
        mutations.clear();
        reloadWorkstreams();
      }}
    />
  );
  const heading = (count: number | null, marker: ReactNode) => (
    <header className="ws-head">
      <div className="ws-head-title">
        <div className="ws-title-row">
          <h1 id={headingId}>Workstreams</h1>
          {count !== null && <span className="ws-subtle">{count} active</span>}
          {marker}
        </div>
        <p className="ws-head-summary">Each workstream groups related projects and tasks toward one goal.</p>
      </div>
      <div className="ws-head-actions">
        <NavLink className="ui-btn ui-btn-primary ws-new" to={{ name: "global", view: "workspace" }}>
          + New workstream
        </NavLink>
        {menu}
      </div>
    </header>
  );

  let body;
  if (model.status === "loading") {
    body = (
      <>
        {heading(null, null)}
        <p role="status">Loading workstreams…</p>
      </>
    );
  } else if (model.status === "offline") {
    body = (
      <>
        {heading(null, null)}
        <p role="status">
          Offline: no cached workstreams. This is not an empty list. · observed{" "}
          <time dateTime={model.observedAt}>{formatAge(model.observedAt, model.ageMs, now)}</time>
        </p>
      </>
    );
  } else if (model.status === "error") {
    body = (
      <>
        {heading(null, null)}
        <p role="alert" className="notice notice-error">
          Workstreams failed to load: {model.message} · observed{" "}
          <time dateTime={model.observedAt}>{formatAge(model.observedAt, model.ageMs, now)}</time>
        </p>
      </>
    );
  } else if (model.status !== "ready") {
    body = (
      <>
        {heading(null, null)}
        <p role="status">
          <PlaceholderBadge provenance={model.provenance} />{" "}
          {model.status === "unsupported" ? "Not supported" : model.status === "unknown" ? "Unknown" : "Unavailable"}:{" "}
          {model.reason} · observed <time dateTime={model.observedAt}>{formatAge(model.observedAt, model.ageMs, now)}</time>
        </p>
      </>
    );
  } else {
    const { groups, mode, unlinked, unlinkedKnown, taskList, stale } = model;
    const fixture = mode === "fixture";
    const marker = fixture ? <PlaceholderBadge provenance={model.provenance} compact /> : null;
    const current = groups.find((group) => group.id === selected) ?? null;
    const first = groups.find((group) => group.state !== "archived")?.id;
    const needle = filter.trim().toLowerCase();
    const by = (state: Status) => groups.filter((group) => group.state === state);
    const matching = (state: Status) =>
      by(state).filter((group) => `${group.title} ${group.projectIds.map(basename).join(" ")}`.toLowerCase().includes(needle));
    const rows = (list: WsGroup[], empty: string) =>
      list.length === 0 ? (
        <p className="ws-subtle" role="status">
          {empty}
        </p>
      ) : (
        <ul className="ws-items" aria-label="Workstreams">
          {list.map((group) => (
            <WorkstreamRow key={group.id} group={group} now={now} onOpen={() => setSelected(group.id)} />
          ))}
        </ul>
      );
    const controls: Controls | null = current
      ? {
          run: mutations.run,
          busy: mutations.busy,
          locked: stale
            ? STALE_REASON
            : current.unreadReason !== null
              ? UNREAD_REASON
              : current.state === "archived"
                ? ARCHIVED_REASON
                : null,
        }
      : null;
    const freshest = groups
      .map(updatedAt)
      .reduce<{ observedAt: string; ageMs: number } | null>(
        (latest, next) => (next && (!latest || Date.parse(next.observedAt) > Date.parse(latest.observedAt)) ? next : latest),
        null,
      );

    body = (
      <>
        {stale && <p className="ws-source"><strong>Stale: the connection was lost; this is the last data received.</strong></p>}
        {live && taskList.status === "error" && (
          <p role="alert" className="ws-source">
            {taskListNotice(taskList)}
          </p>
        )}
        {live && <OutcomeNotice outcome={mutations.outcome} />}
        {current && controls ? (
          <WorkstreamDetail
            key={current.id}
            group={current}
            first={current.id === first}
            fixture={fixture}
            marker={marker}
            unlinked={unlinked}
            unlinkedKnown={unlinkedKnown}
            controls={controls}
            taskList={taskList}
            stale={stale}
            onBack={() => setSelected(null)}
          />
        ) : (
          <>
            {heading(by("active").length, marker)}
            {groups.length === 0 ? (
              <EmptyState
                title="No workstreams"
                description={
                  fixture
                    ? "There are no placeholder workstreams."
                    : "No workstreams exist yet. Ask the global orchestrator to create one, then link tasks to it."
                }
              />
            ) : (
              <>
                <WsTabs
                  label="Workstream status"
                  actions={
                    <input
                      type="search"
                      className="ws-search"
                      aria-label="Search workstreams"
                      placeholder="Search workstreams"
                      value={filter}
                      onChange={(event) => setFilter(event.target.value)}
                    />
                  }
                  tabs={TABS.map(({ id, label }) => ({
                    id,
                    label: `${label} ${by(id).length}`,
                    content: (
                      <>
                        {rows(matching(id), needle === "" ? `No ${id} workstreams.` : "No workstream matches the search.")}
                        {id === "active" && by("archived").length > 0 && (
                          <Disclosure summary="Archived" count={by("archived").length} data-testid="ws-archived">
                            {rows(matching("archived"), "No archived workstream matches the search.")}
                          </Disclosure>
                        )}
                      </>
                    ),
                  }))}
                />
                <footer className="ws-foot">
                  {live && !unlinkedKnown && <span data-testid="ws-unlinked">Tasks not in any workstream: unknown.</span>}
                  {live && unlinked.length > 0 && (
                    <span data-testid="ws-unlinked">{taskCount(unlinked.length)} not in any workstream.</span>
                  )}
                  <span className="ws-spacer" />
                  {freshest && (
                    <span data-testid="ws-updated">
                      ↻ Updated <time dateTime={freshest.observedAt}>{formatAge(freshest.observedAt, freshest.ageMs, now)}</time>
                    </span>
                  )}
                </footer>
              </>
            )}
          </>
        )}
      </>
    );
  }

  return (
    <section className="ws-view" aria-label="Workstreams" data-testid="workstreams-view">
      {body}
    </section>
  );
}
