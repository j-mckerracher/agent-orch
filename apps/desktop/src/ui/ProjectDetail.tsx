/** S2: project detail, derived counts and task submission. */
import { useState } from "react";
import type { DaemonError } from "../daemon/errors";
import type { AutonomyPolicy, DeliveryPolicy, TaskKind } from "../daemon/types";
import { Crumbs } from "./ConnectionStrip";
import { TASK_STATES, basename, pendingApprovals } from "./format";
import { buildSubmission } from "./submission";
import { useGate, useServices, useSource, useStore } from "./services";
import { ErrorNotice, LoadableRegion, Notice, SourceSection } from "./widgets";
import { Button } from "./theme";

export function ProjectDetail({ projectId }: { projectId: string }) {
  const { data, router, drafts, sources } = useServices();
  const { projects, tasks, approvals } = useStore(data);
  const draft = useStore(drafts).submit[projectId] ?? drafts.submitDraft(projectId);
  const gate = useGate();
  const [errors, setErrors] = useState<string[]>([]);
  const [failure, setFailure] = useState<DaemonError | null>(null);
  const [busy, setBusy] = useState(false);
  const agents = useSource(() => sources.agents.listAgents(), "agents", sources.agents.provenance);
  const availability = useSource(
    () => sources.execution.availability(),
    "availability",
    sources.execution.provenance,
  );

  const known = projects.data?.some((row) => row.id === projectId) ?? null;
  const own = tasks.data?.filter((task) => task.contract.project_id === projectId) ?? null;
  const ownIds = new Set(own?.map((task) => task.contract.id));
  const pending = pendingApprovals(approvals.data ?? []).filter((approval) => ownIds.has(approval.action.task_id));
  const set = (patch: Parameters<typeof drafts.patchSubmit>[1]) => drafts.patchSubmit(projectId, patch);

  const submit = async () => {
    setFailure(null);
    const built = buildSubmission(projectId, draft);
    if (!built.ok) {
      setErrors(built.errors);
      return;
    }
    setErrors([]);
    setBusy(true);
    const result = await data.submitTask(built.submission);
    setBusy(false);
    if (result.ok) {
      drafts.clearSubmit(projectId);
      router.navigate({ name: "task", projectId, taskId: result.value.contract.id });
    } else {
      setFailure(result.error);
    }
  };

  return (
    <main className="ps-page">
      <Crumbs
        items={[
          { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
          { label: basename(projectId) },
        ]}
      />
      <h1>{basename(projectId)}</h1>
      <p>
        Repository: <code>{projectId}</code>
      </p>
      {known === false && projects.status === "ready" && (
        <Notice tone="bad" role="alert">
          This repository is not in the returned project list; it may have been removed. Refresh to confirm.
        </Notice>
      )}
      <nav aria-label="Project navigation" className="ps-actions">
        <Button onClick={() => router.navigate({ name: "tasks", projectId, filter: "all" })}>Tasks</Button>
        <Button onClick={() => router.navigate({ name: "coordinator", projectId })}>Coordinator</Button>
        <Button onClick={() => router.navigate({ name: "review", projectId, taskId: null })}>
          Pending review ({pending.length})
        </Button>
      </nav>

      <section aria-label="Task counts" className="ps-section">
        <h2>Task counts</h2>
        <LoadableRegion
          what="tasks"
          loadable={tasks}
          connected={gate.connected}
          isEmpty={() => false}
          emptyText=""
          onRetry={() => void data.refreshAll()}
        >
          {() => (
            <div>
              <p data-testid="task-total">{own?.length ?? 0} tasks in this project</p>
              <ul className="ps-counts">
                {TASK_STATES.map((state) => {
                  const count = own?.filter((task) => task.state === state).length ?? 0;
                  return (
                    <li key={state}>
                      <Button
                        disabled={count === 0}
                        onClick={() => router.navigate({ name: "tasks", projectId, filter: `state:${state}` })}
                      >
                        {state}: {count}
                      </Button>{" "}
                      <span className="muted">of {own?.length ?? 0}</span>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
        </LoadableRegion>
      </section>

      <section aria-label="New task" className="ps-section">
        <h2>New task</h2>
        <form
          className="ps-form"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <label className="ps-wide">
            Objective
            <textarea value={draft.objective} onChange={(e) => set({ objective: e.target.value })} />
          </label>
          <label>
            Kind
            <select value={draft.kind} onChange={(e) => set({ kind: e.target.value as TaskKind })}>
              <option>Implement</option>
              <option>Investigate</option>
            </select>
          </label>
          <label>
            Agent
            <input value={draft.agent} onChange={(e) => set({ agent: e.target.value })} />
          </label>
          <label>
            Parent task ID (optional)
            <input value={draft.parentId} onChange={(e) => set({ parentId: e.target.value })} />
          </label>
          <label>
            Dependencies (task IDs, comma or space separated)
            <input value={draft.dependencies} onChange={(e) => set({ dependencies: e.target.value })} />
          </label>
          <label>
            Verification
            <select
              value={draft.verification}
              onChange={(e) => set({ verification: e.target.value as "None" | "Commands" })}
            >
              <option>None</option>
              <option>Commands</option>
            </select>
          </label>
          {draft.verification === "Commands" && (
            <label className="ps-wide">
              Verification commands (one per line; split on spaces, no shell quoting)
              <textarea value={draft.commands} onChange={(e) => set({ commands: e.target.value })} />
            </label>
          )}
          <label>
            Delivery
            <select value={draft.delivery} onChange={(e) => set({ delivery: e.target.value as DeliveryPolicy })}>
              <option>ReportOnly</option>
              <option>LocalReview</option>
              <option>PullRequest</option>
            </select>
          </label>
          <label>
            Autonomy
            <select value={draft.autonomy} onChange={(e) => set({ autonomy: e.target.value as AutonomyPolicy })}>
              <option>ReadOnly</option>
              <option>WorkspaceWrites</option>
              <option>ApprovalRequired</option>
            </select>
          </label>
          <label>
            Max retries
            <input value={draft.maxRetries} onChange={(e) => set({ maxRetries: e.target.value })} />
          </label>
          <label>
            Max repairs
            <input value={draft.maxRepairs} onChange={(e) => set({ maxRepairs: e.target.value })} />
          </label>
          <div className="ps-form-footer">
            <Button type="submit" variant="primary" disabled={!gate.can("tasks") || busy}>
              Submit
            </Button>
            {!gate.connected && (
              <span className="muted"> Submission is unavailable while offline; the draft is kept.</span>
            )}
            <p className="muted">Submitting queues the task. It does not launch an agent.</p>
          </div>
          {errors.length > 0 && (
            <ul role="alert" className="ps-steps ps-wide">
              {errors.map((message) => (
                <li key={message}>{message}</li>
              ))}
            </ul>
          )}
          {failure && <ErrorNotice error={failure} action="Submitting the task" />}
          {failure?.indeterminate && (
            <p role="status" className="ps-wide">
              The outcome is unknown. Submitting the same form again reuses the same request key.
            </p>
          )}
        </form>

        <SourceSection
          title="Agent detection"
          provenance={sources.agents.provenance}
          result={agents}
          emptyText="No agents reported."
        >
          {(list) => (
            <ul className="ps-steps">
              {list.map((agent) => (
                <li key={agent.id}>
                  {agent.name}: {agent.availability}
                  {agent.features.length > 0 && ` (${agent.features.join(", ")})`}
                  {agent.note && <span className="muted"> {agent.note}</span>}
                </li>
              ))}
            </ul>
          )}
        </SourceSection>
        <SourceSection
          title="Execution availability"
          provenance={sources.execution.provenance}
          result={availability}
          emptyText="No availability reported."
        >
          {(info) => (
            <p>
              Launch available: {info.launchAvailable ? "yes" : "no"}. {info.reason}
            </p>
          )}
        </SourceSection>
      </section>
    </main>
  );
}
