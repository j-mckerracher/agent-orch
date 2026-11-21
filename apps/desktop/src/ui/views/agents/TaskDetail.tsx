/** One task's own view: Chat, Output, Attempts and the event feed, an Actions menu, and technical detail one click deeper. */
import { useEffect, useId, useState } from "react";
import { timelineKey, type Timeline } from "../../../daemon/appData";
import { agentDisplayName } from "../../../daemon/agentDisplayNames";
import { useServices, useStore } from "../../services";
import { Button, Disclosure, EmptyState } from "../../theme";
import { PlaceholderBadge } from "../../widgets";
import { TaskPermissionBadge, ToolsNotice } from "../../TaskPermission";
import { ActionMenu } from "../workstreams/ActionMenu";
import { WsTabs } from "../workstreams/WsTabs";
import { StatePill } from "./common";
import { OutputTail } from "./OutputTail";
import { isRunningSession, rowFromTask, type WorkRow } from "./model";
import { TaskConversation } from "../../TaskConversation";
import { useTaskActions } from "./TaskActions";
import { taskSubtitle } from "./TaskList";

const PAYLOAD_LIMIT = 200;

function payloadText(payload: unknown): string {
  const text = typeof payload === "string" ? payload : (JSON.stringify(payload) ?? "");
  return text.length > PAYLOAD_LIMIT ? `${text.slice(0, PAYLOAD_LIMIT)}…` : text;
}

function uniqueKinds(entries: ReadonlyArray<{ kind: string }>): string[] {
  return [...new Set(entries.map((entry) => entry.kind))];
}

export function TaskDetail({ initial, live, onBack }: { initial: WorkRow; live: boolean; onBack: () => void }) {
  const { data, sources } = useServices();
  const snapshot = useStore(data);
  const [chosen, setChosen] = useState<string | null>(null);
  const [kind, setKind] = useState("");
  const sessionPickId = useId();
  const real = live && initial.origin === "real";

  useEffect(() => {
    if (real) void data.watchTask(initial.id);
  }, [real, initial.id, data]);

  const record = real ? snapshot.taskDetails[initial.id]?.data : null;
  const row = record ? rowFromTask(record, snapshot.taskSessions[initial.id]?.data ?? null, initial.blocked) : initial;
  const sessions = row.sessions ?? [];
  const session = sessions.find((s) => s.id === chosen) ?? sessions[sessions.length - 1] ?? null;
  const running = session !== null && isRunningSession(session.state);
  const actions = useTaskActions(row, live);

  useEffect(() => {
    if (!real || !session) return;
    void data.watchSession(row.id, session.id);
    void data.loadTimeline(row.id, session.id, "reset");
  }, [real, row.id, session?.id, data]);

  const timeline = real && session ? snapshot.timelines[timelineKey(row.id, session.id)] : undefined;
  const kinds = uniqueKinds(real ? (timeline?.events ?? []) : row.timeline);
  const activeKind = kinds.includes(kind) ? kind : "";

  const noSession = (what: string) =>
    row.sessions === null ? (
      <p role="status">Sessions unknown: this task's session list has not loaded.</p>
    ) : (
      <EmptyState title={`No ${what}`} description="This task has no session yet." />
    );

  return (
    <section className="ws-detail" aria-label={`Task ${row.objective}`} data-testid="agents-detail">
      <header className="ws-detail-head">
        <button type="button" className="ws-back" onClick={onBack}>
          <span aria-hidden="true">←</span> Agents
        </button>
        <div className="ws-title-row">
          <h1>{row.objective}</h1>
          <StatePill state={row.state} />
          <ToolsNotice notice={row.toolsNotice} />
          <TaskPermissionBadge level={row.permission} />
          {!live && <PlaceholderBadge provenance={sources.execution.provenance} compact />}
          <span className="ws-spacer" />
          {actions.items.length > 0 && (
            <ActionMenu label="Actions" trigger={<>Actions <span aria-hidden="true">▾</span></>} items={actions.items} />
          )}
          {/* Outside the menu's condition: the last item can leave the menu while its result is still shown. */}
          {actions.statuses}
        </div>
        <p className="ws-subtle">{taskSubtitle(row, true)}</p>
      </header>

      {sessions.length > 1 && (
        <div className="agents-filter">
          <label htmlFor={sessionPickId}>Session</label>
          <select id={sessionPickId} value={session?.id ?? ""} onChange={(event) => setChosen(event.target.value)}>
            {sessions.map((s) => (
              <option key={s.id} value={s.id}>
                {s.id} ({s.state})
              </option>
            ))}
          </select>
        </div>
      )}

      <WsTabs
        label={`Views of ${row.objective}`}
        tabs={[
          {
            // Always the first tab, so the window opens on it every time (WsTabs starts on the first tab).
            id: "chat",
            label: "Chat",
            content: real ? (
              <>
                {row.sessions === null && <p role="status">Sessions unknown: this task's session list has not loaded.</p>}
                {/* The session picker above picks the thread. `tasks.message` addresses the task: the daemon queues the
                    text for the task's next run and has no per-session or live-steering message. */}
                <TaskConversation
                  taskId={row.id}
                  state={row.state}
                  sessionId={session?.id ?? null}
                  running={running}
                  agent={agentDisplayName(row.agent)}
                />
              </>
            ) : (
              <EmptyState title="No conversation" description="The conversation is only read from the daemon." />
            ),
          },
          {
            id: "output",
            label: "Output",
            content:
              real && session ? (
                <OutputTail key={session.id} taskId={row.id} sessionId={session.id} running={running} />
              ) : real ? (
                noSession("output")
              ) : (
                <EmptyState title="No output" description="Output is only read from the daemon." />
              ),
          },
          {
            id: "attempts",
            label: `Attempts ${row.attempts.length}`,
            content: <Attempts row={row} />,
          },
          {
            id: "feed",
            label: "Event feed",
            content: (
              <>
                {kinds.length > 0 && <TraceFilter kinds={kinds} value={activeKind} onChange={setKind} />}
                {live ? (
                  <Feed
                    timeline={timeline}
                    sessionsKnown={row.sessions !== null}
                    sessionId={session?.id ?? null}
                    taskId={row.id}
                    kind={activeKind}
                  />
                ) : (
                  <PreviewFeed entries={row.timeline} kind={activeKind} />
                )}
              </>
            ),
          },
        ]}
      />

      <Disclosure summary="Technical details" count="ids, adapter, scope notes" data-testid="agents-technical">
        <dl className="agents-facts">
          <dt>Task</dt>
          <dd>
            <code>{row.id}</code>
          </dd>
          <dt>Agent</dt>
          <dd>
            {agentDisplayName(row.agent)} (<code>{row.agent}</code>)
          </dd>
          <dt>Source</dt>
          <dd>{live ? "Daemon" : "Placeholder preview data"}</dd>
        </dl>
        <p className="agents-sub">
          Cancel applies to the whole task: the daemon has no attempt-level cancel. Side effects and supplied context are
          not recorded by the daemon, so none are listed.
        </p>
      </Disclosure>
    </section>
  );
}

/** Every attempt with the sessions it ran. Sessions the daemon links to no listed attempt follow under "Other sessions". */
function Attempts({ row }: { row: WorkRow }) {
  if (row.attempts.length === 0 && (row.sessions ?? []).length === 0) return <p>No attempts recorded.</p>;
  const known = new Set(row.attempts.map((attempt) => attempt.id));
  const sessionList = (items: NonNullable<WorkRow["sessions"]>) =>
    items.length === 0 ? null : (
      <ul className="agents-plain">
        {items.map((s) => (
          <li key={s.id}>
            session <code>{s.id}</code> <StatePill state={s.state} />
            {s.adapter && <span className="agents-sub"> {agentDisplayName(s.adapter)}</span>}
          </li>
        ))}
      </ul>
    );
  const sessions = row.sessions ?? [];
  const others = sessions.filter((s) => s.attemptId === null || !known.has(s.attemptId));
  return (
    <>
      {row.sessions === null && <p role="status">Sessions unknown: this task's session list has not loaded.</p>}
      <ol className="agents-plain agents-attempts">
        {row.attempts.map((attempt) => (
          <li key={attempt.id}>
            {attempt.number !== null ? `Attempt ${attempt.number}` : "Attempt"} <StatePill state={attempt.state} />
            {sessionList(sessions.filter((s) => s.attemptId === attempt.id))}
          </li>
        ))}
      </ol>
      {others.length > 0 && (
        <>
          <h3>Other sessions</h3>
          {sessionList(others)}
        </>
      )}
    </>
  );
}

/** Local filter over the feed rows already loaded; it never asks the daemon for anything. */
function TraceFilter({ kinds, value, onChange }: { kinds: string[]; value: string; onChange: (kind: string) => void }) {
  const id = useId();
  return (
    <div className="agents-filter">
      <label htmlFor={id}>Filter raw trace</label>
      <select id={id} value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">All event kinds</option>
        {kinds.map((kind) => (
          <option key={kind} value={kind}>
            {kind}
          </option>
        ))}
      </select>
    </div>
  );
}

function PreviewFeed({ entries, kind }: { entries: Array<{ at: string; kind: string; detail: string }>; kind: string }) {
  if (entries.length === 0) return <p>No preview events.</p>;
  const shown = kind ? entries.filter((entry) => entry.kind === kind) : entries;
  return (
    <table aria-label="Execution feed">
      <thead>
        <tr>
          <th scope="col">Time</th>
          <th scope="col">Kind</th>
          <th scope="col">Detail</th>
        </tr>
      </thead>
      <tbody>
        {shown.map((entry, index) => (
          <tr key={index}>
            <td>{entry.at}</td>
            <td>{entry.kind}</td>
            <td>{entry.detail}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Feed({
  timeline,
  sessionsKnown,
  sessionId,
  taskId,
  kind,
}: {
  timeline: Timeline | undefined;
  sessionsKnown: boolean;
  sessionId: string | null;
  taskId: string;
  kind: string;
}) {
  const { data } = useServices();
  if (!sessionsKnown) return <p role="status">Execution feed unknown: this task's session list has not loaded.</p>;
  if (!sessionId) return <p>No session recorded, so there is no execution feed.</p>;
  if (!timeline || timeline.status === "loading") return <p role="status">Loading execution feed…</p>;
  if (timeline.status === "error") {
    return (
      <div role="alert">
        <p>The execution feed could not be loaded: {timeline.error?.message ?? "unknown error"}</p>
        <Button onClick={() => void data.loadTimeline(taskId, sessionId, "reset")}>Retry</Button>
      </div>
    );
  }
  const shown = kind ? timeline.events.filter((event) => event.kind === kind) : timeline.events;
  return (
    <>
      {timeline.events.length === 0 ? (
        <p>No events recorded for this session.</p>
      ) : (
        <table aria-label="Execution feed">
          <thead>
            <tr>
              <th scope="col">Seq</th>
              <th scope="col">Time</th>
              <th scope="col">Kind</th>
              <th scope="col">Payload</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((event) => (
              <tr key={event.sequence}>
                <td>{event.sequence}</td>
                <td>{event.created_at}</td>
                <td>{event.kind}</td>
                <td>
                  <code>{payloadText(event.payload)}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {timeline.error && <p role="alert">The last feed refresh failed: {timeline.error.message}</p>}
      {timeline.more && (
        <Button pending={timeline.loadingMore} onClick={() => void data.loadTimeline(taskId, sessionId, "more")}>
          Load more events
        </Button>
      )}
    </>
  );
}
