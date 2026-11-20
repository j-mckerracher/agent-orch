/** S4: a task's page: state, one next action, then Conversation, Attempts and Timeline; the rest under Technical details. */
import { useEffect, useState } from "react";
import { timelineKey } from "../daemon/appData";
import { APPROVER_REQUIRED_NOTICE, asDaemonError, type DaemonError } from "../daemon/errors";
import type { TaskRecord } from "../daemon/types";
import type { SimulatedReceipt, TaskRef } from "../sources/types";
import { Crumbs } from "./ConnectionStrip";
import { Dialog } from "./Dialog";
import { basename, canArchive, canCancel, pendingApprovals, stateLabel } from "./format";
import { agentDisplayName } from "../daemon/agentDisplayNames";
import { useGate, useServices, useSource, useStore } from "./services";
import { Button, Disclosure, Pill } from "./theme";
import { TaskQueueStatus } from "./TaskQueueStatus";
import { TaskConversation } from "./TaskConversation";
import { TaskPermissionBadge, ToolsNotice } from "./TaskPermission";
import { taskPermissionLevel } from "./views/workspace/permissionLevel";
import { ActionMenu, type MenuItem } from "./views/workstreams/ActionMenu";
import { WsTabs } from "./views/workstreams/WsTabs";
import { ErrorNotice, LoadableRegion, SimulatedReceipts, SourceSection, StateText, TableWrap } from "./widgets";
import "./projectScreens.css";

/** Outcome and recovery, plus (fixture preview only) a simulated retry. A real task's retry is the page's own action. */
export function PlaceholderPanels({ taskRef, previewId, withRetry = false }: { taskRef: TaskRef; previewId?: string; withRetry?: boolean }) {
  const { sources } = useServices();
  const refKey = taskRef.origin === "real" ? `real:${taskRef.taskId}:${taskRef.state}` : `fixture:${taskRef.id}`;
  const outcome = useSource(() => sources.execution.outcomeFor(taskRef), `outcome:${refKey}`, sources.execution.provenance);
  const recovery = useSource(() => sources.recovery.recoveryFor(taskRef), `recovery:${refKey}`, sources.recovery.provenance);
  const retry = useSource(() => sources.retry.retryFor(taskRef), `retry:${refKey}`, sources.retry.provenance);
  const [receipts, setReceipts] = useState<SimulatedReceipt[]>([]);
  const note = (receipt: SimulatedReceipt) => setReceipts((list) => [...list, receipt]);
  const ownId = taskRef.origin === "real" ? taskRef.taskId : taskRef.id;

  return (
    <div>
      {withRetry && (
        <SourceSection
          title="Retry"
          provenance={sources.retry.provenance}
          result={retry}
          emptyText="No retry information."
        >
          {(info) => (
            <div>
              <p>
                Retry {info.available ? "would be available" : "is not available"}: {info.reason}
              </p>
              {info.available && (
                <Button
                  onClick={() =>
                    void sources.retry
                      .simulateRetry(taskRef)
                      .then((r) => note({ ...r, notice: `${r.notice} (new attempt ${r.newAttemptId})` }))
                  }
                >
                  Preview retry
                </Button>
              )}
            </div>
          )}
        </SourceSection>
      )}
      <SourceSection
        title="Execution outcome"
        provenance={sources.execution.provenance}
        result={outcome}
        emptyText="No execution outcome."
      >
        {(info) => (
          <div>
            <p>
              State: {stateLabel(info.state)}. Launch: {info.launchOutcome}.
              {info.cancelOutcome ? ` Cancel: ${info.cancelOutcome}.` : ""}
            </p>
            {previewId && (
              <div className="ps-actions">
                <Button onClick={() => void sources.execution.simulateLaunch(previewId).then(note)}>
                  Simulate launch
                </Button>
                <Button onClick={() => void sources.execution.simulateCancel(previewId).then(note)}>
                  Simulate cancel
                </Button>
              </div>
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
            <ul className="ps-steps">
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


export const UNDELIVERED_TEXT = "This task has changes that were not delivered. Deliver them, or discard them first.";
export const DISCARD_CONFIRM_TEXT =
  "Discard this task's changes? The branch stays in the repository, but the app will no longer keep the task's workspace for it.";

const DELIVERY_PHRASE = {
  ReportOnly: "reports only",
  LocalReview: "ends in a local review",
  PullRequest: "ends in a pull request",
} as const;

export function TaskDetail({ projectId, taskId }: { projectId: string; taskId: string }) {
  const { data, router } = useServices();
  const { taskDetails } = useStore(data);
  const gate = useGate();

  useEffect(() => {
    void data.watchTask(taskId);
  }, [data, taskId]);

  const detail = taskDetails[taskId] ?? { status: "idle" as const, data: null, error: null };
  return (
    <main className="ps-page">
      <Crumbs
        items={[
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
          { label: "Task" },
        ]}
      />
      {detail.data === null && <h1>Task</h1>}
      <LoadableRegion
        what="task"
        loadable={detail}
        connected={gate.connected}
        isEmpty={() => false}
        emptyText=""
        onRetry={() => void data.watchTask(taskId)}
      >
        {(record) => <TaskView projectId={projectId} taskId={taskId} record={record} />}
      </LoadableRegion>
    </main>
  );
}

function TaskView({ projectId, taskId, record }: { projectId: string; taskId: string; record: TaskRecord }) {
  const { data, router, sources } = useServices();
  const { taskSessions, sessionDetails, timelines, tasks, approvals, archiveUnsupported } = useStore(data);
  const gate = useGate();
  const [chosen, setChosen] = useState<string | null>(null);
  const [cancelError, setCancelError] = useState<DaemonError | null>(null);
  const [cancelNote, setCancelNote] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [retryNote, setRetryNote] = useState<string | null>(null);
  const [retryError, setRetryError] = useState<DaemonError | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [archiveError, setArchiveError] = useState<DaemonError | null>(null);
  const [archived, setArchived] = useState(false);
  const [archiving, setArchiving] = useState(false);
  const [discardAsked, setDiscardAsked] = useState(false);
  const [discarding, setDiscarding] = useState(false);
  const [discarded, setDiscarded] = useState(false);
  const [discardError, setDiscardError] = useState<DaemonError | null>(null);

  const taskRef: TaskRef = { origin: "real", taskId, state: record.state };
  const retry = useSource(
    () => sources.retry.retryFor(taskRef),
    `retry:real:${taskId}:${record.state}`,
    sources.retry.provenance,
  );
  const realRetry = sources.retry.provenance.kind === "daemon" ? sources.retry.retry : undefined;
  const canRetry = !!realRetry && retry.state === "ready" && retry.data.available && gate.can("tasks");

  const sessionEntries = taskSessions[taskId]?.data ?? null;
  const sessions = sessionEntries?.map((entry) => entry.session) ?? record.sessions;
  const sessionId = chosen && sessions.some((s) => s.id === chosen) ? chosen : (sessions[sessions.length - 1]?.id ?? null);
  const session = sessions.find((s) => s.id === sessionId);
  const running = session?.state === "Spawning" || session?.state === "Running";

  useEffect(() => {
    if (sessionId === null) return;
    void data.watchSession(taskId, sessionId);
    void data.loadTimeline(taskId, sessionId, "reset");
  }, [data, taskId, sessionId]);

  const timeline = sessionId ? timelines[timelineKey(taskId, sessionId)] : undefined;
  const sessionDetail = sessionId ? sessionDetails[timelineKey(taskId, sessionId)]?.data : null;
  const awaitingReview = pendingApprovals(approvals.data ?? []).some((a) => a.action.task_id === taskId);
  const dependencyKnown = (id: string) => tasks.data?.some((t) => t.contract.id === id) ?? false;

  const retryTask = async () => {
    if (!realRetry) return;
    setRetryNote(null);
    setRetryError(null);
    setRetrying(true);
    try {
      await realRetry.call(sources.retry, taskRef);
      setRetryNote("The retry request was accepted by the daemon.");
      void data.watchTask(taskId);
      void data.refreshAll();
    } catch (error) {
      setRetryError(asDaemonError(error));
    } finally {
      setRetrying(false);
    }
  };

  const archive = async () => {
    setArchiveError(null);
    setArchiving(true);
    const result = await data.archiveTasks([taskId]);
    setArchiving(false);
    if (result.ok) setArchived(true);
    else setArchiveError(result.error);
  };

  // `tasks.discard` is user-only, like the settings.
  const discardLocked = !(gate.connected && gate.connection.approver);
  const discard = async () => {
    setDiscardError(null);
    setDiscarding(true);
    const result = await data.discardTask(taskId);
    setDiscarding(false);
    if (result.ok) {
      setDiscarded(true);
      setDiscardAsked(false);
      setArchiveError(null);
    } else {
      setDiscardError(result.error);
    }
  };

  const cancel = async () => {
    setCancelError(null);
    setCancelNote(null);
    setCancelling(true);
    const result = await data.cancelTask(taskId);
    setCancelling(false);
    if (result.ok) {
      setCancelNote(
        `Cancel request acknowledged; the daemon now records the task as ${stateLabel(result.value.state)}. This is not proof that any process has stopped.`,
      );
      void data.watchTask(taskId);
    } else {
      setCancelError(result.error);
    }
  };

  // One primary action by state; an action the task or the daemon cannot take is not shown.
  const failed = record.state === "Failed" || record.state === "VerificationFailed";
  const primary = awaitingReview ? (
    <Button variant="primary" onClick={() => router.navigate({ name: "global", view: "review", taskId })}>
      Review changes
    </Button>
  ) : failed && canRetry ? (
    <Button variant="primary" disabled={retrying} onClick={() => void retryTask()}>
      Retry
    </Button>
  ) : canCancel(record.state) && gate.can("tasks") ? (
    <Button variant="danger" disabled={cancelling} onClick={() => void cancel()}>
      Cancel
    </Button>
  ) : null;
  const finished = canArchive(record.state);
  const items: MenuItem[] = [
    ...(canRetry && !(failed && !awaitingReview) ? [{ label: "Retry", onSelect: () => void retryTask() }] : []),
    ...(finished && !discardLocked && !discarded ? [{ label: "Discard changes", onSelect: () => setDiscardAsked(true) }] : []),
    ...(!archiveUnsupported && finished && gate.can("tasks") && !archived && !archiving
      ? [{ label: "Archive", onSelect: () => void archive() }]
      : []),
  ];
  const conflict = archiveError?.kind === "conflict" && finished;

  return (
    <div>
      <header className="pt-head">
        <div className="pt-title-row">
          <h1>{record.contract.objective}</h1>
          <span data-testid="task-state">
            {awaitingReview ? <Pill tone="warn">Awaiting review</Pill> : <StateText state={record.state} />}
          </span>
          <ToolsNotice notice={record.tools_notice} />
          <TaskPermissionBadge level={taskPermissionLevel(record)} />
          <span className="pt-spacer" />
          {items.length > 0 && <ActionMenu label="Task actions" trigger={<span aria-hidden="true">⋯</span>} items={items} />}
          {primary}
        </div>
        <p className="muted">
          {agentDisplayName(record.contract.agent)} · {DELIVERY_PHRASE[record.contract.delivery]}
        </p>
      </header>
      <div aria-live="polite">
        {retryNote && <p role="status">{retryNote}</p>}
        {cancelNote && <p role="status">{cancelNote}</p>}
        {archived && <p role="status">Archived. The task leaves the task lists; its history is kept.</p>}
        {discarded && !archived && <p role="status">Changes discarded. You can archive the task now.</p>}
      </div>
      {retryError && <ErrorNotice error={retryError} action="Retrying the task" />}
      {retryError?.indeterminate && <p role="status">The outcome is unknown. Retrying again reuses the same request key.</p>}
      {cancelError && <ErrorNotice error={cancelError} action="Canceling the task" />}
      {cancelError?.indeterminate && <p role="status">The outcome is unknown. Requesting again reuses the same request key.</p>}
      {conflict ? (
        <div>
          <p role="alert">{UNDELIVERED_TEXT}</p>
          {discardLocked ? (
            <p className="muted">{APPROVER_REQUIRED_NOTICE}</p>
          ) : (
            <Button variant="danger" onClick={() => setDiscardAsked(true)}>
              Discard changes
            </Button>
          )}
        </div>
      ) : (
        archiveError && <ErrorNotice error={archiveError} action="Archiving the task" />
      )}
      {discardError && <ErrorNotice error={discardError} action="Discarding the changes" />}
      {discardAsked && (
        <Dialog
          title="Discard changes?"
          onClose={() => setDiscardAsked(false)}
          footer={
            <>
              <Button disabled={discarding} onClick={() => setDiscardAsked(false)}>
                Keep changes
              </Button>
              <Button variant="danger" disabled={discarding} onClick={() => void discard()}>
                Confirm discard
              </Button>
            </>
          }
        >
          <p>{DISCARD_CONFIRM_TEXT}</p>
        </Dialog>
      )}

      <WsTabs
        label="Task views"
        tabs={[
          {
            id: "conversation",
            label: "Conversation",
            content: (
              <>
                {record.state === "Queued" && <TaskQueueStatus key={taskId} taskId={taskId} agent={record.contract.agent} />}
                {record.state === "Queued" && record.sessions.length === 0 && (
                  <p role="status">
                    Queued: the daemon has admitted this task but no agent has been launched and no session is recorded.
                  </p>
                )}
                <TaskConversation
                  taskId={taskId}
                  state={record.state}
                  sessionId={sessionId}
                  running={running}
                  agent={agentDisplayName(record.contract.agent)}
                />
              </>
            ),
          },
          {
            id: "attempts",
            label: `Attempts ${record.attempts.length}`,
            content: (
              <section aria-label="Attempts" className="ps-section">
                {record.attempts.length === 0 ? (
                  <p>No attempts recorded.</p>
                ) : (
                  <ul className="ps-steps">
                    {record.attempts.map((attempt) => (
                      <li key={attempt.id}>
                        #{attempt.attempt_number} <code>{attempt.id}</code>: {stateLabel(attempt.state)}
                        {attempt.state === "Failed" && (
                          <p data-testid={`failure-reason-${attempt.id}`}>
                            Failure reason: {attempt.failure_reason?.trim() || "unavailable."}
                          </p>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            ),
          },
          {
            id: "timeline",
            label: "Timeline",
            content: (
              <section aria-label="Event timeline" className="ps-section">
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
                      <TableWrap>
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
                      </TableWrap>
                    )}
                    {timeline.more && (
                      <Button
                        disabled={timeline.loadingMore || !gate.connected}
                        onClick={() => void data.loadTimeline(taskId, sessionId, "more")}
                      >
                        Load more session events
                      </Button>
                    )}
                  </div>
                )}
              </section>
            ),
          },
        ]}
      />

      <Disclosure summary="Technical details" count="outcome, recovery, sessions, audit transitions">
        <section aria-label="Task summary" className="ps-section">
          <dl className="ps-facts">
            <dt>ID</dt>
            <dd><code>{record.contract.id}</code></dd>
            <dt>State</dt>
            <dd>{stateLabel(record.state)}</dd>
            <dt>Version</dt>
            <dd>{record.version}</dd>
            <dt>Kind / agent</dt>
            <dd>{record.contract.kind} / {agentDisplayName(record.contract.agent)}</dd>
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
                  <Button key={dep} onClick={() => router.navigate({ name: "task", projectId, taskId: dep })}>
                    {dep}
                  </Button>
                ) : (
                  <code key={dep}>{dep} (not in the returned list)</code>
                ),
              )}
            </dd>
          </dl>
        </section>
        <section aria-label="Sessions" className="ps-section">
          <h3>Sessions</h3>
          {sessions.length === 0 ? (
            <p>No recorded session.</p>
          ) : (
            <div>
              <label className="ps-field">
                Recorded session
                <select value={sessionId ?? ""} onChange={(event) => setChosen(event.target.value)}>
                  {sessions.map((session) => (
                    <option key={session.id} value={session.id}>
                      {session.id} ({stateLabel(session.state)})
                    </option>
                  ))}
                </select>
              </label>
              {sessionDetail && (
                <p data-testid="session-detail">
                  Adapter {sessionDetail.session.adapter}, state {stateLabel(sessionDetail.session.state)}, attempt{" "}
                  <code>{sessionDetail.session.attempt_id}</code>
                </p>
              )}
            </div>
          )}
        </section>
        <section aria-label="Audit transitions" className="ps-section">
          <h3>Task audit transitions</h3>
          {record.events.length === 0 ? (
            <p>None recorded.</p>
          ) : (
            <ol className="ps-steps">
              {record.events.map((event, index) => (
                <li key={index}>{event}</li>
              ))}
            </ol>
          )}
        </section>
        <PlaceholderPanels taskRef={taskRef} />
      </Disclosure>
    </div>
  );
}

/** Fixture-only S4: never reads or writes daemon records. */
export function FixtureTaskDetail({ projectId, id }: { projectId: string; id: string }) {
  const { router, sources } = useServices();
  const preview = useSource(() => sources.execution.getPreviewTask(id), `preview:${id}`, sources.execution.provenance);
  return (
    <main className="ps-page">
      <Crumbs
        items={[
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
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
              <strong>{task.objective}</strong> — {stateLabel(task.state)}, agent {agentDisplayName(task.agent)}
            </p>
            <ul aria-label="Preview attempts" className="ps-steps">
              {task.attempts.map((a) => (
                <li key={a.id}>
                  <code>{a.id}</code>: {stateLabel(a.state)}
                </li>
              ))}
            </ul>
            <ul aria-label="Preview sessions" className="ps-steps">
              {task.sessions.map((s) => (
                <li key={s.id}>
                  <code>{s.id}</code>: {stateLabel(s.state)}
                </li>
              ))}
            </ul>
            <TableWrap>
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
            </TableWrap>
            <PlaceholderPanels taskRef={{ origin: "fixture", id }} previewId={id} withRetry />
          </div>
        )}
      </SourceSection>
    </main>
  );
}
