import { useEffect, useId, useMemo, useState, type KeyboardEvent } from "react";
import type { AppDataSnapshot } from "../../daemon/appData";
import { isFixtureId } from "../../daemon/api";
import type { SearchHit, SearchResults, SurfaceResult } from "../../sources/surfaces/types";
import type { Route } from "../router";
import { useServices, useStore } from "../services";
import { Badge } from "../theme";
import { PlaceholderBadge } from "../widgets";
import "./topbar.css";

const MAX_SESSION_HITS = 20;
const WORKSTREAMS: Route = { name: "global", view: "workstreams" };
const AGENTS: Route = { name: "global", view: "agents" };
const REVIEW: Route = { name: "global", view: "review" };

type GroupKey = "workstream" | "task" | "session" | "decision";
const GROUP_ORDER: readonly GroupKey[] = ["workstream", "task", "session", "decision"];

interface Option {
  key: string;
  group: GroupKey;
  title: string;
  detail: string;
  fixture: boolean;
  route: Route;
}

function groupLabel(group: GroupKey, fixture: boolean): string {
  switch (group) {
    case "workstream":
      return fixture ? "Workstreams" : "Projects";
    case "task":
      return "Tasks";
    case "session":
      return "Sessions";
    case "decision":
      return "Approvals";
  }
}

function projectOfTask(store: AppDataSnapshot, taskId: string): string | null {
  return store.tasks.data?.find((task) => task.contract.id === taskId)?.contract.project_id ?? null;
}

/**
 * Where a hit opens. Fixture records open the matching global view, never a daemon-backed
 * record route, so a fixture id cannot reach a request. A record the loaded lists no
 * longer contain falls back to its global view.
 */
function routeFor(hit: SearchHit, store: AppDataSnapshot): Route {
  const fixture = hit.origin === "fixture" || isFixtureId(hit.id);
  switch (hit.kind) {
    case "workstream":
      return fixture ? WORKSTREAMS : { name: "project", projectId: hit.id };
    case "task": {
      const projectId = fixture ? null : projectOfTask(store, hit.id);
      return projectId ? { name: "task", projectId, taskId: hit.id } : fixture ? WORKSTREAMS : AGENTS;
    }
    case "decision": {
      const approval = fixture ? undefined : store.approvals.data?.find((item) => item.id === hit.id);
      const projectId = approval ? projectOfTask(store, approval.action.task_id) : null;
      return approval && projectId ? { name: "review", projectId, taskId: approval.action.task_id } : REVIEW;
    }
  }
}

/** Sessions are not part of D15; match the already-loaded `sessions.list` rows here. */
function sessionOptions(store: AppDataSnapshot, query: string): Option[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  const options: Option[] = [];
  for (const entry of store.sessions.data ?? []) {
    const projectId = projectOfTask(store, entry.task_id);
    const { session } = entry;
    const haystack = [session.id, session.adapter, session.state, entry.task_id].join("\n").toLowerCase();
    if (!projectId || !terms.every((term) => haystack.includes(term))) continue;
    options.push({
      key: `session:${entry.task_id}:${session.id}`,
      group: "session",
      title: session.id,
      detail: `${session.state} - ${session.adapter} - task ${entry.task_id}`,
      fixture: false,
      route: { name: "task", projectId, taskId: entry.task_id },
    });
    if (options.length >= MAX_SESSION_HITS) break;
  }
  return options;
}

type SearchState = { status: "idle" } | { status: "done"; query: string; result: SurfaceResult<SearchResults> };

function summarize(state: SearchState, count: number, notice: string | null, pending: boolean): string {
  if (notice) return notice;
  if (state.status === "idle") return pending ? "Searching…" : "";
  const { result, query } = state;
  if (result.status !== "ok") return `Search is ${result.status}: ${result.reason}`;
  if (count === 0) return `No loaded records match “${query}”.`;
  const noun = count === 1 ? "result" : "results";
  return result.data.truncated ? `${count} ${noun} shown for “${query}”; more exist, refine the query.` : `${count} ${noun} for “${query}”.`;
}

/**
 * Local search over records already loaded in this window (D15 for projects/workstreams,
 * tasks and approvals; the loaded session list for sessions). It adds no daemon method.
 * ARIA combobox: ArrowUp/Down move through the grouped results, Enter opens the active
 * (or first) result, Escape closes. Result counts are announced through a live region.
 */
export function TopSearch({ debounceMs = 200 }: { debounceMs?: number }) {
  const { surfaces, data, router } = useServices();
  const store = useStore(data);
  const ids = useId();
  const listId = `${ids}-list`;
  const hintId = `${ids}-hint`;
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const [notice, setNotice] = useState<string | null>(null);
  const [state, setState] = useState<SearchState>({ status: "idle" });

  const trimmed = query.trim();
  const { projects, tasks, approvals } = store;
  // Earlier results stay on screen while a new query is debounced; the effect also re-runs
  // when the loaded lists change so results follow store events.
  useEffect(() => {
    if (!trimmed) {
      setState((previous) => (previous.status === "idle" ? previous : { status: "idle" }));
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      surfaces.search.search(trimmed).then(
        (result) => live && setState({ status: "done", query: trimmed, result }),
        (error: unknown) =>
          live &&
          setState({
            status: "done",
            query: trimmed,
            result: {
              status: "unavailable",
              reason: error instanceof Error ? error.message : "search failed",
              provenance: surfaces.search.provenance,
            },
          }),
      );
    }, debounceMs);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [trimmed, debounceMs, surfaces, projects, tasks, approvals]);

  // The displayed results answer an older query until the debounced search returns.
  const pending = trimmed !== "" && !(state.status === "done" && state.query === trimmed);
  const groups = useMemo(() => {
    if (state.status !== "done" || !trimmed) return [];
    const options: Option[] = [];
    if (state.result.status === "ok") {
      for (const hit of state.result.data.hits) {
        options.push({
          key: `${hit.kind}:${hit.id}`,
          group: hit.kind,
          title: hit.title,
          detail: hit.detail,
          fixture: hit.origin === "fixture",
          route: routeFor(hit, store),
        });
      }
    }
    options.push(...sessionOptions(store, state.query));
    return GROUP_ORDER.map((group) => ({ group, options: options.filter((option) => option.group === group) })).filter(
      (entry) => entry.options.length > 0,
    );
  }, [state, store, trimmed]);
  const flat = groups.flatMap((entry) => entry.options);
  const activeOption = flat[active];
  const optionId = (option: Option) => `${ids}-opt-${flat.indexOf(option)}`;
  const skipped = state.status === "done" && state.result.status === "ok" ? state.result.data.skipped : [];
  const fixtureSource = surfaces.search.provenance.kind === "fixture";
  // Keep the last announced text while a newer query is pending so the live region does not
  // repeat "Searching…" on every keystroke once results exist.
  const status = summarize(state, flat.length, notice, pending);

  const choose = (option: Option) => {
    router.navigate(option.route);
    setOpen(false);
    setQuery("");
    setActive(-1);
    setNotice(`Opened ${option.title}.`);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp": {
        if (flat.length === 0) return;
        event.preventDefault();
        setOpen(true);
        setActive((current) =>
          event.key === "ArrowDown" ? (current + 1) % flat.length : current <= 0 ? flat.length - 1 : current - 1,
        );
        break;
      }
      case "Enter": {
        const option = activeOption ?? flat[0];
        if (open && option && !pending) {
          event.preventDefault();
          choose(option);
        }
        break;
      }
      case "Escape":
        if (open) setOpen(false);
        else if (query) setQuery("");
        setActive(-1);
        break;
    }
  };

  return (
    <div
      className="top-search"
      role="search"
      aria-label="Search loaded records"
      data-testid="top-search"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <input
        type="search"
        role="combobox"
        className="top-search-input"
        aria-label="Search"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={open && flat.length > 0 ? listId : undefined}
        aria-activedescendant={open && activeOption ? optionId(activeOption) : undefined}
        aria-describedby={hintId}
        placeholder="Search projects, tasks, sessions, approvals"
        autoComplete="off"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
          setActive(-1);
          setNotice(null);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
        data-testid="top-search-input"
      />
      <span id={hintId} className="top-sr">
        Searches loaded records only.
      </span>
      <div role="status" className="top-sr" data-testid="top-search-status">
        {status}
      </div>
      {open && (
        <div className="top-search-panel" data-testid="top-search-panel" onMouseDown={(event) => event.preventDefault()}>
          <p className="top-search-note" data-testid="top-search-note">
            Searches loaded records only: projects, tasks, sessions and approvals already loaded in this window. Nothing is
            searched on the daemon. {fixtureSource && <PlaceholderBadge provenance={surfaces.search.provenance} />}
          </p>
          {status && !flat.length && (
            <p className="top-search-summary" aria-hidden="true">
              {pending ? "Searching…" : status}
            </p>
          )}
          {skipped.length > 0 && (
            <p className="top-search-skipped" data-testid="top-search-skipped">
              Not searched: {skipped.map((item) => `${item.source} (${item.status}: ${item.reason})`).join("; ")}. A missing
              match there is not evidence of absence.
            </p>
          )}
          {flat.length > 0 && (
            <div role="listbox" id={listId} aria-label="Search results" className="top-search-groups">
              {groups.map(({ group, options }) => {
                const titleId = `${ids}-g-${group}`;
                return (
                  <div key={group} role="group" aria-labelledby={titleId}>
                    <div id={titleId} className="top-search-group-title" role="presentation">
                      {groupLabel(group, options[0].fixture)} ({options.length})
                    </div>
                    {options.map((option) => (
                      <div
                        key={option.key}
                        id={optionId(option)}
                        role="option"
                        aria-selected={option === activeOption}
                        className="top-search-option"
                        onClick={() => choose(option)}
                      >
                        <span className="top-search-option-title">
                          {option.title} {option.fixture && <Badge tone="placeholder">PLACEHOLDER DATA</Badge>}
                        </span>
                        <span className="top-search-option-detail">{option.detail}</span>
                      </div>
                    ))}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
