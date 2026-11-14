/** Selected task and session detail: attempt history, sessions, controls and the execution feed. */
import { useEffect, useId, useState } from "react";
import { timelineKey, type Timeline } from "../../../daemon/appData";
import { useServices, useStore } from "../../services";
import { Button, Disclosure, EmptyState } from "../../theme";
import { DisabledAction, PlaceholderMark, StatePill } from "./common";
import { rowFromTask, type WorkRow } from "./model";
import { SessionControls } from "./SessionControls";
import { TaskControls } from "./TaskControls";

const PAYLOAD_LIMIT = 200;

function payloadText(payload: unknown): string {
  const text = typeof payload === "string" ? payload : (JSON.stringify(payload) ?? "");
  return text.length > PAYLOAD_LIMIT ? `${text.slice(0, PAYLOAD_LIMIT)}…` : text;
}

export function SessionDetail({ row, live }: { row: WorkRow | null; live: boolean }) {
  return (
    <aside className="agents-card" aria-labelledby="agents-detail-title">
      <header className="agents-card-head">
        <h2 id="agents-detail-title">Selected task / session detail</h2>
        {!live && row && <PlaceholderMark id="D2" />}
      </header>
      {row ? (
        <Selected key={row.id} initial={row} live={live} />
      ) : (
        <EmptyState
          title="No task selected"
          description="Select a task to see its attempts, sessions and execution feed."
        />
      )}
    </aside>
  );
}

function uniqueKinds(entries: ReadonlyArray<{ kind: string }>): string[] {
  return [...new Set(entries.map((entry) => entry.kind))];
}

function Selected({ initial, live }: { initial: WorkRow; live: boolean }) {
  const { data } = useServices();
  const snapshot = useStore(data);
  const [chosen, setChosen] = useState<string | null>(null);
  const [kind, setKind] = useState("");
  const real = live && initial.origin === "real";

  useEffect(() => {
    if (real) void data.watchTask(initial.id);
  }, [real, initial.id, data]);

  const record = real ? snapshot.taskDetails[initial.id]?.data : null;
  const row = record ? rowFromTask(record, snapshot.taskSessions[initial.id]?.data ?? null, initial.blocked) : initial;
  const sessions = row.sessions ?? [];
  const session = sessions.find((s) => s.id === chosen) ?? sessions[sessions.length - 1] ?? null;

  useEffect(() => {
    if (!real || !session) return;
    void data.watchSession(row.id, session.id);
    void data.loadTimeline(row.id, session.id, "reset");
  }, [real, row.id, session?.id, data]);

  const timeline = real && session ? snapshot.timelines[timelineKey(row.id, session.id)] : undefined;
  const kinds = uniqueKinds(real ? (timeline?.events ?? []) : row.timeline);
  const activeKind = kinds.includes(kind) ? kind : "";

  return (
    <div className="agents-detail">
      <p>
        <code>{row.id}</code> <StatePill state={row.state} />
      </p>
      <p>{row.objective}</p>
      <p className="agents-sub">Agent {row.agent}</p>

      <section aria-label="Attempt history">
        <h3>Attempt history</h3>
        {row.attempts.length === 0 ? (
          <p>No attempts recorded.</p>
        ) : (
          <ol className="agents-plain">
            {row.attempts.map((attempt) => (
              <li key={attempt.id}>
                {attempt.number !== null ? `Attempt ${attempt.number}` : "Attempt"} <code>{attempt.id}</code>{" "}
                <StatePill state={attempt.state} />
              </li>
            ))}
          </ol>
        )}
      </section>

      <section aria-label="Sessions">
        <h3>Sessions</h3>
        {row.sessions === null ? (
          <p role="status">Sessions unknown: this task's session list has not loaded.</p>
        ) : row.sessions.length === 0 ? (
          <p>No session recorded.</p>
        ) : (
          <ul className="agents-plain">
            {row.sessions.map((s) => (
              <li key={s.id}>
                <Button variant="quiet" aria-pressed={session?.id === s.id} onClick={() => setChosen(s.id)}>
                  <code>{s.id}</code>
                </Button>{" "}
                <StatePill state={s.state} />
                {s.adapter && <span className="agents-sub"> adapter {s.adapter}</span>}
              </li>
            ))}
          </ul>
        )}
      </section>

      <TaskControls key={row.id} row={row} live={live} />
      <SessionControls row={row} session={session} live={live} />

      <Disclosure summary="Execution feed, side effects and supplied context">
        <p className="agents-sub">
          Side effects and supplied context are not recorded by the daemon, so none are listed here.
        </p>
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
        <section aria-label="Feed actions" className="agents-controls">
          <DisabledAction
            label="Request diagnostic"
            reason="Not available: the daemon has no method to request a diagnostic for a session."
          />
          <DisabledAction
            label="Branch from retained evidence"
            reason="Not available: the daemon does not expose retained evidence to branch from."
          />
        </section>
      </Disclosure>
    </div>
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
      {timeline.error && <p role="status">The last feed refresh failed: {timeline.error.message}</p>}
      {timeline.more && (
        <Button disabled={timeline.loadingMore} onClick={() => void data.loadTimeline(taskId, sessionId, "more")}>
          {timeline.loadingMore ? "Loading…" : "Load more events"}
        </Button>
      )}
    </>
  );
}
