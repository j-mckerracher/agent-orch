/** S2: project detail, derived counts and task submission. */
import { useEffect, useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError, type DaemonError } from "../daemon/errors";
import type { AutonomyPolicy, DeliveryPolicy, TaskKind } from "../daemon/types";
import {
  FIXTURE_SAVE_TEXT,
  MAX_CHECKS,
  MERGE_OPTIONS,
  MODE_OPTIONS,
  defaultsOf,
  draftOf,
  hasErrors,
  saveFailureText,
  validateDraft,
  type DefaultsDraft,
} from "../sources/surfaces/projectDefaults";
import type { ProjectDefaults } from "../sources/surfaces/types";
import { Crumbs } from "./ConnectionStrip";
import { TASK_STATES, basename, pendingApprovals, stateLabel } from "./format";
import { buildSubmission } from "./submission";
import { useGate, useServices, useSource, useStore, useSurface } from "./services";
import { NOT_CONNECTED_REASON } from "./views/settings/settingsModel";
import { SaveButton, useSaveGuard } from "./views/settings/SaveBar";
import { ErrorNotice, LoadableRegion, Notice, SourceSection } from "./widgets";
import { Button } from "./theme";

const SAVED_TEXT = "Saved. These defaults apply to this project from now on.";
const INVALID_REASON = "Fix the fields marked as invalid before saving.";
const PREFIX_HINT =
  "Task branches start with this. Letters, digits, dot, dash, underscore and slash, ending with a slash (for example mcao/). Not orchestrator/. Leave empty to use the machine-wide prefix.";
const CHECKS_HINT = `Each command runs without a shell: type the program and its arguments separated by spaces. Quotes and variables are not interpreted. Up to ${MAX_CHECKS} commands.`;

const describedBy = (...ids: Array<string | false | null | undefined>) => ids.filter(Boolean).join(" ") || undefined;

/** Delivery defaults for one project (DPES-1105). Hidden quietly while loading or when the daemon cannot offer them. */
function ProjectDefaultsSection({ projectId }: { projectId: string }) {
  const { surfaces } = useServices();
  const source = surfaces.projectDefaults;
  const gate = useGate();
  const loaded = useSurface(() => source.get(projectId), `project-defaults:${projectId}`, source.provenance);
  // What the daemon answered a save with, until a newer read replaces it; and what the user has typed (null: untouched).
  const [fresh, setFresh] = useState<ProjectDefaults | null>(null);
  const [typed, setTyped] = useState<DefaultsDraft | null>(null);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<{ kind: "saved" | "fixture" } | { kind: "failed"; message: string } | null>(null);
  const section = useRef<HTMLElement>(null);
  const busy = useRef(false);
  const ids = { hint: useId(), prefix: useId(), base: useId(), checks: useId(), reason: useId(), mode: useId() };

  const { tried, guard } = useSaveGuard();
  useEffect(() => setFresh(null), [loaded]);

  if (loaded.status === "loading" || loaded.status === "unsupported" || (loaded.status !== "ok" && gate.settling)) return null;
  if (loaded.status !== "ok") {
    return (
      <section aria-label="Defaults for this project" className="ps-section">
        <h2>Defaults for this project</h2>
        <Notice tone="warn" role="status">
          The defaults for this project could not be read. {loaded.reason}
        </Notice>
      </section>
    );
  }

  const saved = fresh ?? loaded.data;
  const draft = typed ?? draftOf(saved);
  const errors = validateDraft(draft);
  const live = source.provenance.kind === "daemon";
  const locked = live && !(gate.connected && gate.connection.approver);
  const reason = live && !gate.connected
    ? NOT_CONNECTED_REASON
    : locked
      ? APPROVER_REQUIRED_NOTICE
      : hasErrors(errors)
        ? INVALID_REASON
        : null;
  const update = (patch: Partial<DefaultsDraft>) => setTyped({ ...draft, ...patch });
  const setCheck = (index: number, line: string) => update({ checks: draft.checks.map((item, i) => (i === index ? line : item)) });
  const known = (options: ReadonlyArray<{ value: string }>, value: string) => options.some((option) => option.value === value);

  const save = async () => {
    if (busy.current) return;
    busy.current = true;
    setSaving(true);
    setResult(null);
    try {
      const receipt = await source.set(projectId, defaultsOf(draft));
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        setFresh(receipt.defaults);
        setTyped(null);
        setResult({ kind: "saved" });
      }
    } catch (error) {
      setResult({ kind: "failed", message: saveFailureText(asDaemonError(error)) });
    } finally {
      busy.current = false;
      setSaving(false);
      // The Save button may be disabled by now; focus stays on the section so the result is read in place.
      section.current?.focus();
    }
  };

  return (
    <section ref={section} tabIndex={-1} aria-label="Defaults for this project" className="ps-section">
      <h2>Defaults for this project</h2>
      <p className="muted">
        Delivery mode, branches and checks for tasks in this project. Chat can ask for a pull request only when Settings allows it.
      </p>
      <form noValidate className="ps-form" onSubmit={guard(reason, () => void save())}>
        <fieldset className="ps-wide" aria-describedby={describedBy(locked && ids.reason)}>
          <legend>Delivery mode</legend>
          {MODE_OPTIONS.map((option) => (
            <div key={option.value}>
              <label>
                <input
                  type="radio"
                  name={ids.mode}
                  value={option.value}
                  checked={draft.mode === option.value}
                  disabled={locked}
                  aria-describedby={describedBy(`${ids.mode}-${option.value}`, locked && ids.reason)}
                  onChange={() => update({ mode: option.value })}
                />{" "}
                {option.label}
              </label>
              <p id={`${ids.mode}-${option.value}`} className="muted">
                {option.consequence}
              </p>
            </div>
          ))}
          {!known(MODE_OPTIONS, draft.mode) && <p className="muted">The daemon reports a mode this app does not know: {draft.mode}.</p>}
        </fieldset>

        <label>
          Base branch
          <input
            value={draft.baseBranch}
            disabled={locked}
            aria-invalid={errors.baseBranch !== null}
            aria-describedby={describedBy(ids.base, errors.baseBranch !== null && `${ids.base}-error`, locked && ids.reason)}
            onChange={(e) => update({ baseBranch: e.target.value })}
          />
        </label>
        <p id={ids.base} className="muted ps-wide">
          Leave empty to branch from the current checkout
        </p>
        {errors.baseBranch !== null && (
          <Notice tone="bad" id={`${ids.base}-error`} role="alert" className="ps-wide">
            {errors.baseBranch}
          </Notice>
        )}

        <label>
          Branch prefix
          <input
            value={draft.branchPrefix}
            disabled={locked}
            aria-invalid={errors.branchPrefix !== null}
            aria-describedby={describedBy(ids.prefix, errors.branchPrefix !== null && `${ids.prefix}-error`, locked && ids.reason)}
            onChange={(e) => update({ branchPrefix: e.target.value })}
          />
        </label>
        <p id={ids.prefix} className="muted ps-wide">
          {PREFIX_HINT}
        </p>
        {errors.branchPrefix !== null && (
          <Notice tone="bad" id={`${ids.prefix}-error`} role="alert" className="ps-wide">
            {errors.branchPrefix}
          </Notice>
        )}

        <fieldset className="ps-wide" aria-describedby={describedBy(ids.checks, errors.checks !== null && `${ids.checks}-error`, locked && ids.reason)}>
          <legend>Checks</legend>
          <p id={ids.checks} className="muted">
            {CHECKS_HINT}
          </p>
          {draft.checks.length === 0 && <p className="muted">No checks. Full verification needs at least one.</p>}
          <ul className="ps-steps">
            {draft.checks.map((line, index) => (
              // Rows have no identity beyond their place, so the index is the key.
              <li key={index}>
                <label>
                  Check command {index + 1}
                  <input
                    value={line}
                    disabled={locked}
                    aria-invalid={errors.checks !== null}
                    aria-describedby={describedBy(ids.checks, locked && ids.reason)}
                    onChange={(e) => setCheck(index, e.target.value)}
                  />
                </label>{" "}
                <Button
                  disabled={locked}
                  aria-describedby={describedBy(locked && ids.reason)}
                  onClick={() => update({ checks: draft.checks.filter((_, i) => i !== index) })}
                >
                  Remove check command {index + 1}
                </Button>
              </li>
            ))}
          </ul>
          <Button
            disabled={locked || draft.checks.length >= MAX_CHECKS}
            aria-describedby={describedBy(locked ? ids.reason : draft.checks.length >= MAX_CHECKS && ids.checks)}
            onClick={() => update({ checks: [...draft.checks, ""] })}
          >
            Add check command
          </Button>
          {errors.checks !== null && (
            <Notice tone="bad" id={`${ids.checks}-error`} role="alert">
              {errors.checks}
            </Notice>
          )}
        </fieldset>

        <label>
          Merge method
          <select
            value={draft.mergeMethod}
            disabled={locked}
            aria-describedby={describedBy(locked && ids.reason)}
            onChange={(e) => update({ mergeMethod: e.target.value })}
          >
            {!known(MERGE_OPTIONS, draft.mergeMethod) && <option value={draft.mergeMethod}>{draft.mergeMethod}</option>}
            {MERGE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <div className="ps-form-footer">
          <SaveButton label="Save project defaults" reason={reason} saving={saving} tried={tried} reasonId={ids.reason} />
          <div role="status" aria-live="polite" aria-label="Project defaults save result">
            {result?.kind === "saved" && <p>{SAVED_TEXT}</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
            {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
          </div>
        </div>
      </form>
    </section>
  );
}

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
  // Pending review needs both lists: without the task list no approval can be matched to this project.
  const pending =
    tasks.data && approvals.data
      ? pendingApprovals(approvals.data).filter((approval) => ownIds.has(approval.action.task_id))
      : null;
  // Unread is never zero: "…" while a daemon answer can still arrive, "unknown" once it cannot.
  const answerPending = tasks.status !== "error" && approvals.status !== "error" && (gate.connected || gate.settling);
  const pendingText = pending ? String(pending.length) : answerPending ? "…" : "unknown";
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
          Pending review ({pendingText})
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
                        {stateLabel(state)}: {count}
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

      <ProjectDefaultsSection key={projectId} projectId={projectId} />

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
          <p className="muted ps-wide">Use the launcher ID: codex or claude-code. Display capitalization is accepted.</p>
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
            <p className="muted">
              Submitting queues the task. Eligible queued work may start automatically when the scheduler can run it.
            </p>
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
