/** S4: real task detail, plus the DUMMY D2/D3/D4 panels shared with the fixture-only view. */
import { useEffect, useState } from "react";
import { timelineKey } from "../daemon/appData";
import { asDaemonError, type DaemonError } from "../daemon/errors";
import type { SimulatedReceipt, TaskRef } from "../sources/types";
import { Crumbs } from "./ConnectionStrip";
import { TERMINAL_STATES, basename, pendingApprovals } from "./format";
import { useGate, useServices, useSource, useStore } from "./services";
import { ErrorNotice, LoadableRegion, SimulatedReceipts, SourceSection } from "./widgets";

export function PlaceholderPanels({ taskRef, previewId }: { taskRef: TaskRef; previewId?: string }) {
  const { sources, data } = useServices();
  const gate = useGate();
  const refKey = taskRef.origin === "real" ? `real:${taskRef.taskId}:${taskRef.state}` : `fixture:${taskRef.id}`;
  const outcome = useSource(() => sources.execution.outcomeFor(taskRef), `outcome:${refKey}`, sources.execution.provenance);
  const recovery = useSource(() => sources.recovery.recoveryFor(taskRef), `recovery:${refKey}`, sources.recovery.provenance);
  const retry = useSource(() => sources.retry.retryFor(taskRef), `retry:${refKey}`, sources.retry.provenance);
  const [receipts, setReceipts] = useState<SimulatedReceipt[]>([]);
  const [retryNote, setRetryNote] = useState<string | null>(null);
  const [retryError, setRetryError] = useState<DaemonError | null>(null);
  const [retrying, setRetrying] = useState(false);
  const note = (receipt: SimulatedReceipt) => setReceipts((list) => [...list, receipt]);
  const ownId = taskRef.origin === "real" ? taskRef.taskId : taskRef.id;
  const realRetry = sources.retry.provenance.kind === "daemon" ? sources.retry.retry : undefined;

  const retryTask = async () => {
    if (!realRetry) return;
    setRetryNote(null);
    setRetryError(null);
    setRetrying(true);
    try {
      const result = await realRetry.call(sources.retry, taskRef);
      setRetryNote(`Retry accepted; the daemon now records the task as ${result.state} with ${result.attempts} attempt(s).`);
      void data.watchTask(ownId);
      void data.refreshAll();
    } catch (error) {
      setRetryError(asDaemonError(error));
    } finally {
      setRetrying(false);
    }
  };

  return (
    <div>
      <SourceSection
        title="Retry"
        provenance={sources.retry.provenance}
        result={retry}
        emptyText="No retry information."
      >
        {(info) => (
          <div>
            <p>
              Retry {info.available ? (realRetry ? "is available" : "would be available") : "is not available"}:{" "}
              {info.reason}
            </p>
            {realRetry ? (
              <div>
                <button
                  type="button"
                  disabled={!info.available || retrying || !gate.can("tasks")}
                  onClick={() => void retryTask()}
                >
                  Retry task
                </button>
                {retryNote && <p role="status">{retryNote}</p>}
                {retryError && <ErrorNotice error={retryError} action="Retrying the task" />}
                {retryError?.indeterminate && (
                  <p role="status">The outcome is unknown. Retrying again reuses the same request key.</p>
                )}
              </div>
            ) : (
              <button
                type="button"
                disabled={!info.available}
                onClick={() =>
                  void sources.retry
                    .simulateRetry(taskRef)
                    .then((r) => note({ ...r, notice: `${r.notice} (new attempt ${r.newAttemptId})` }))
                }
              >
                Preview retry
              </button>
            )}
          </div>
        )}
      </SourceSection>
      <SourceSection
        title="Execution outcome"
        provenance={sources.execution.provenance}
        result={outcome}
        emptyText="No execution outcome."
      >
        {(info) => (
          <div>
            <p>
              State: {info.state}. Launch: {info.launchOutcome}.
              {info.cancelOutcome ? ` Cancel: ${info.cancelOutcome}.` : ""}
            </p>
            {previewId && (
              <p>
                <button type="button" onClick={() => void sources.execution.simulateLaunch(previewId).then(note)}>
                  Simulate launch
                </button>{" "}
                <button type="button" onClick={() => void sources.execution.simulateCancel(previewId).then(note)}>
                  Simulate cancel
                </button>
              </p>
            )}
          </div>
        )}
      </SourceSection>
      <SourceSection
        title="Recovery classification"
        provenance={sources.recovery.provenance}
        result={recovery}
        emptyText="No recovery information."
      >
        {(info) => (
          <div>
            <p>
              {info.classification}: {info.reason} (phase: {info.phase})
            </p>
            <ul>
              <li>Process: {info.checks.process}</li>
              <li>Workspace: {info.checks.workspace}</li>
              <li>Delivery: {info.checks.delivery}</li>
            </ul>
          </div>
        )}
      </SourceSection>
      {receipts.length > 0 && <SimulatedReceipts label={`Simulated receipts for ${ownId}`} receipts={receipts} />}
    </div>
  );
}

export function TaskDetail({ projectId, taskId }: { projectId: string; taskId: string }) {
  const { data, router } = useServices();
  const { taskDetails, taskSessions, sessionDetails, timelines, tasks, approvals } = useStore(data);
  const gate = useGate();
  const [chosen, setChosen] = useState<string | null>(null);
  const [cancelError, setCancelError] = useState<DaemonError | null>(null);
  const [cancelNote, setCancelNote] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);

  useEffect(() => {
    void data.watchTask(taskId);
  }, [data, taskId]);

  const detail = taskDetails[taskId] ?? { status: "idle" as const, data: null, error: null };
  const task = detail.data;
  const sessionEntries = taskSessions[taskId]?.data ?? null;
  const sessions = sessionEntries?.map((entry) => entry.session) ?? task?.sessions ?? [];
  const sessionId = chosen && sessions.some((s) => s.id === chosen) ? chosen : (sessions[sessions.length - 1]?.id ?? null);

  useEffect(() => {
    if (sessionId === null) return;
    void data.watchSession(taskId, sessionId);
    void data.loadTimeline(taskId, sessionId, "reset");
  }, [data, taskId, sessionId]);

  const timeline = sessionId ? timelines[timelineKey(taskId, sessionId)] : undefined;
  const sessionDetail = sessionId ? sessionDetails[timelineKey(taskId, sessionId)]?.data : null;
  const candidateApprovals = pendingApprovals(approvals.data ?? []).filter((a) => a.action.task_id === taskId);
  const dependencyKnown = (id: string) => tasks.data?.some((t) => t.contract.id === id) ?? false;

  const cancel = async () => {
    setCancelError(null);
    setCancelNote(null);
    setCancelling(true);
    const result = await data.cancelTask(taskId);
    setCancelling(false);
    if (result.ok) {
      setCancelNote(
        `Cancel request acknowledged; the daemon now records the task as ${result.value.state}. This is not proof that any process has stopped.`,
      );
      void data.watchTask(taskId);
    } else {
      setCancelError(result.error);
    }
  };

  return (
    <main>
      <Crumbs
        items={[
          { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
          { label: "Tasks", onClick: () => router.navigate({ name: "tasks", projectId, filter: "all" }) },
          { label: taskId },
        ]}
      />
      <h1>Task</h1>
      <LoadableRegion
        what="task"
        loadable={detail}
        connected={gate.connected}
        isEmpty={() => false}
        emptyText=""
        onRetry={() => void data.watchTask(taskId)}
      >
        {(record) => (
          <div>
            <p>
              <button type="button" onClick={() => router.navigate({ name: "tasks", projectId, filter: "all" })}>
                Back to tasks
              </button>{" "}
              {candidateApprovals.length > 0 ? (
                <button type="button" onClick={() => router.navigate({ name: "review", projectId, taskId })}>
                  Review candidate
                </button>
              ) : (
                <span className="muted">No pending approval for this task.</span>
              )}
            </p>
            <section aria-label="Task summary">
              <p>
                <strong>{record.contract.objective}</strong>
              </p>
              <dl>
                <dt>ID</dt>
                <dd><code>{record.contract.id}</code></dd>
                <dt>State</dt>
                <dd data-testid="task-state">{record.state}</dd>
                <dt>Version</dt>
                <dd>{record.version}</dd>
                <dt>Kind / agent</dt>
                <dd>{record.contract.kind} / {record.contract.agent}</dd>
                <dt>Delivery / autonomy</dt>
                <dd>{record.contract.delivery} / {record.contract.autonomy}</dd>
                <dt>Limits</dt>
                <dd>retries {record.contract.max_retries}, repairs {record.contract.max_repairs}</dd>
                <dt>Verification</dt>
                <dd><code>{JSON.stringify(record.contract.verification)}</code></dd>
                <dt>Dependencies</dt>
                <dd>
                  {record.contract.dependencies.length === 0 && "none"}
                  {record.contract.dependencies.map((dep) =>
                    dependencyKnown(dep) ? (
                      <button key={dep} type="button" onClick={() => router.navigate({ name: "task", projectId, taskId: dep })}>
                        {dep}
                      </button>
                    ) : (
                      <code key={dep}>{dep} (not in the returned list)</code>
                    ),
                  )}
                </dd>
              </dl>
              {record.state === "Queued" && record.sessions.length === 0 && (
                <p role="status">
                  Queued: the daemon has admitted this task but no agent has been launched and no session is recorded.
                </p>
              )}
            </section>
            <section aria-label="Attempts">
              <h2>Attempts</h2>
              {record.attempts.length === 0 ? (
                <p>No attempts recorded.</p>
              ) : (
                <ul>
                  {record.attempts.map((attempt) => (
                    <li key={attempt.id}>
                      #{attempt.attempt_number} <code>{attempt.id}</code>: {attempt.state}
                    </li>
                  ))}
                </ul>
              )}
            </section>
            <section aria-label="Audit transitions">
              <h2>Task audit transitions</h2>
              {record.events.length === 0 ? (
                <p>None recorded.</p>
              ) : (
                <ol>
                  {record.events.map((event, index) => (
                    <li key={index}>{event}</li>
                  ))}
                </ol>
              )}
            </section>
            <section aria-label="Sessions">
              <h2>Sessions</h2>
              {sessions.length === 0 ? (
                <p>No recorded session.</p>
              ) : (
                <div>
                  <label>
                    Recorded session
                    <select value={sessionId ?? ""} onChange={(event) => setChosen(event.target.value)}>
                      {sessions.map((session) => (
                        <option key={session.id} value={session.id}>
                          {session.id} ({session.state})
                        </option>
                      ))}
                    </select>
                  </label>
                  {sessionDetail && (
                    <p data-testid="session-detail">
                      Adapter {sessionDetail.session.adapter}, state {sessionDetail.session.state}, attempt{" "}
                      <code>{sessionDetail.session.attempt_id}</code>
                    </p>
                  )}
                </div>
              )}
            </section>
            <section aria-label="Event timeline">
              <h2>Timeline</h2>
              {sessionId === null ? (
                <p>No session events: no session is recorded for this task.</p>
              ) : !timeline || timeline.status === "loading" ? (
                <p role="status">Loading session events…</p>
              ) : (
                <div>
                  {timeline.error && <ErrorNotice error={timeline.error} action="Loading session events" />}
                  {timeline.events.length === 0 ? (
                    <p>No session events recorded.</p>
                  ) : (
                    <table aria-label="Session events">
                      <thead>
                        <tr>
                          <th>Sequence / time</th>
                          <th>Reported kind</th>
                          <th>Recorded payload</th>
                        </tr>
                      </thead>
                      <tbody>
                        {timeline.events.map((event) => (
                          <tr key={event.sequence}>
                            <td>
                              {event.sequence} <span className="muted">{event.created_at}</span>
                            </td>
                            <td>{event.kind}</td>
                            <td>
                              <pre>{JSON.stringify(event.payload, null, 2)}</pre>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                  {timeline.more && (
                    <button
                      type="button"
                      disabled={timeline.loadingMore || !gate.connected}
                      onClick={() => void data.loadTimeline(taskId, sessionId, "more")}
                    >
                      Load more session events
                    </button>
                  )}
                </div>
              )}
            </section>
            <section aria-label="Cancellation">
              <button
                type="button"
                disabled={!gate.can("tasks") || cancelling || TERMINAL_STATES.has(record.state)}
                onClick={() => void cancel()}
              >
                Request cancel
              </button>
              {!gate.connected && <span className="muted"> Unavailable while offline.</span>}
              {cancelNote && <p role="status">{cancelNote}</p>}
              {cancelError && <ErrorNotice error={cancelError} action="Cancelling the task" />}
              {cancelError?.indeterminate && (
                <p role="status">The outcome is unknown. Requesting again reuses the same request key.</p>
              )}
            </section>
            <PlaceholderPanels taskRef={{ origin: "real", taskId, state: record.state }} />
          </div>
        )}
      </LoadableRegion>
    </main>
  );
}

/** Fixture-only S4: never reads or writes daemon records. */
export function FixtureTaskDetail({ projectId, id }: { projectId: string; id: string }) {
  const { router, sources } = useServices();
  const preview = useSource(() => sources.execution.getPreviewTask(id), `preview:${id}`, sources.execution.provenance);
  return (
    <main>
      <Crumbs
        items={[
          { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
          { label: "Tasks", onClick: () => router.navigate({ name: "tasks", projectId, filter: "all" }) },
          { label: `${id} (preview)` },
        ]}
      />
      <h1>Task preview</h1>
      <SourceSection
        title="Fixture task"
        provenance={sources.execution.provenance}
        result={preview}
        emptyText="This preview task does not exist."
      >
        {(task) => (
          <div>
            <p>
              <strong>{task.objective}</strong> — {task.state}, agent {task.agent}
            </p>
            <ul aria-label="Preview attempts">
              {task.attempts.map((a) => (
                <li key={a.id}>
                  <code>{a.id}</code>: {a.state}
                </li>
              ))}
            </ul>
            <ul aria-label="Preview sessions">
              {task.sessions.map((s) => (
                <li key={s.id}>
                  <code>{s.id}</code>: {s.state}
                </li>
              ))}
            </ul>
            <table aria-label="Preview timeline">
              <tbody>
                {task.timeline.map((row) => (
                  <tr key={`${row.at}-${row.kind}`}>
                    <td>{row.at}</td>
                    <td>{row.kind}</td>
                    <td>{row.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <PlaceholderPanels taskRef={{ origin: "fixture", id }} previewId={id} />
          </div>
        )}
      </SourceSection>
    </main>
  );
}
