/** Selected task and session detail: attempt history, sessions, controls and the execution feed. */
import { useEffect, useState } from "react";
import { timelineKey, type Timeline } from "../../../daemon/appData";
import { useServices, useStore } from "../../services";
import { Button, Disclosure, EmptyState } from "../../theme";
import { DisabledAction, PlaceholderMark, StatePill } from "./common";
import { rowFromTask, type WorkRow } from "./model";
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

function Selected({ initial, live }: { initial: WorkRow; live: boolean }) {
  const { data } = useServices();
  const snapshot = useStore(data);
  const [chosen, setChosen] = useState<string | null>(null);
  const real = live && initial.origin === "real";

  useEffect(() => {
    if (real) void data.watchTask(initial.id);
  }, [real, initial.id, data]);

  const record = real ? snapshot.taskDetails[initial.id]?.data : null;
  const row = record ? rowFromTask(record, snapshot.taskSessions[initial.id]?.data ?? null, initial.blocked) : initial;
  const session = row.sessions.find((s) => s.id === chosen) ?? row.sessions[row.sessions.length - 1] ?? null;

  useEffect(() => {
    if (!real || !session) return;
    void data.watchSession(row.id, session.id);
    void data.loadTimeline(row.id, session.id, "reset");
  }, [real, row.id, session?.id, data]);

  const timeline = real && session ? snapshot.timelines[timelineKey(row.id, session.id)] : undefined;

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
        {row.sessions.length === 0 ? (
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

      <section aria-label="Session controls" className="agents-controls">
        <h4>Session controls</h4>
        <DisabledAction
          label="Resume session"
          reason="Not available: the daemon has no method to resume a session, whatever an adapter reports for session resumption."
        />
        <DisabledAction
          label="Inspect retained checkpoint"
          reason="Not available: the daemon does not expose retained checkpoints."
        />
      </section>

      <Disclosure summary="Execution feed, side effects and supplied context">
        <p className="agents-sub">
          Side effects and supplied context are not recorded by the daemon, so none are listed here.
        </p>
        {live ? (
          <Feed timeline={timeline} sessionId={session?.id ?? null} taskId={row.id} />
        ) : (
          <PreviewFeed entries={row.timeline} />
        )}
      </Disclosure>
    </div>
  );
}

function PreviewFeed({ entries }: { entries: Array<{ at: string; kind: string; detail: string }> }) {
  if (entries.length === 0) return <p>No preview events.</p>;
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
        {entries.map((entry, index) => (
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
  sessionId,
  taskId,
}: {
  timeline: Timeline | undefined;
  sessionId: string | null;
  taskId: string;
}) {
  const { data } = useServices();
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
            {timeline.events.map((event) => (
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
