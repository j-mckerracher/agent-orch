// The Review queue (#/review): one list of everything that needs the user, newest first. A chip marks each item
// Approval, Question, Candidate or PR. Three tabs split it into Needs you, Held and Resolved; a project filter and a
// type filter narrow it. Simple items are decided in their row; a candidate opens its own page.
import { useEffect, useMemo, useRef, useState } from "react";
import type { ReviewClient } from "../../../review/client";
import type { ReviewSession } from "../../../review/reviewSession";
import { heldSets } from "../../../sources/surfaces/holds";
import type { ProposalItem } from "../../../sources/surfaces/types";
import { ActionStatus, useAction } from "../../action";
import { basename, decisionCountText, pendingDecisionCount } from "../../format";
import { useGate, useServices, useStore, useSurface } from "../../services";
import { Button, EmptyState } from "../../theme";
import { asDaemonError, presentError } from "../../../daemon/errors";
import { useActionDecisions } from "../workspace/useActionDecisions";
import { useRetained } from "../workspace/useRetained";
import { WsTabs } from "../workstreams/WsTabs";
import { HOLD_REASON_ID } from "./HoldControl";
import { QueueRow, type RowEnv } from "./QueueRows";
import { TABS, TYPE_LABEL, buildQueue, filterQueue, projectsOf, type ItemType, type QueueItem, type QueueTab } from "./queue";
import { useApprovalQueue } from "./useApprovalQueue";
import { useHolds } from "./useHolds";
import { useQuestions } from "./useQuestions";
import { usePulls } from "./usePulls";

const NO_PROPOSALS: ProposalItem[] = [];
const NO_NAMES = new Map<string, string>();
const TYPES = Object.keys(TYPE_LABEL) as ItemType[];

/**
 * The same number the shell shows as "need you": pending approvals plus pending proposed actions, held ones left out.
 * Null until both lists have loaded: it is unknown then, never 0.
 */
export function useWaitingCount(): string | null {
  const { data } = useServices();
  const { approvals, proposals, holds } = useStore(data);
  if (approvals.data === null || proposals.data === null) return null;
  return decisionCountText(pendingDecisionCount(approvals.data, proposals.data, heldSets(holds.data ?? [])), proposals.more);
}

/**
 * A decision's own result is shown by its row (ProposalRow): a decided row moves to Resolved, so nothing is written to a
 * shared region for it.
 */
const noAnnounce = () => undefined;

const EMPTY: Record<QueueTab, { title: string; description: string }> = {
  needs: { title: "Nothing needs you", description: "Approvals, questions, candidates and pull requests that wait for you appear here." },
  held: { title: "Nothing is held", description: "Hold a decision and it waits here until its day." },
  resolved: { title: "Nothing resolved yet", description: "Decisions you make in this session appear here." },
};

export function ReviewQueue({ client, session, projectId }: { client: ReviewClient; session: ReviewSession; projectId?: string }) {
  const { surfaces, data, router } = useServices();
  const gate = useGate();
  const [project, setProject] = useState<string | null>(projectId ?? null);
  const [type, setType] = useState<ItemType | null>(null);
  const holds = useHolds();
  const questions = useQuestions();
  const pulls = usePulls();
  const waiting = useWaitingCount();
  const refreshList = useAction();

  // The shell's lists refresh on their own. This view reads again when the pending ids in them change, and keeps
  // the last answer on screen meanwhile, so the list and the count do not stay apart.
  const { approvals, proposals, tasks } = useStore(data);
  const approvalIds = approvals.data?.map((item) => item.id).join(",") ?? "";
  const pendingIds = proposals.data?.map((item) => item.id).join(",") ?? "";
  const { queue, resolved, refresh } = useApprovalQueue(client, session, approvalIds);
  const decisions = useRetained(
    useSurface(() => surfaces.decisions.queue(), `review-decisions:${pendingIds}`, surfaces.decisions.provenance),
    "review-decisions",
  );

  const live = surfaces.conversation.provenance.kind === "daemon";
  const decided = useActionDecisions({
    scope: "review",
    resolve: surfaces.conversation.resolve,
    announce: noAnnounce,
    reload: () => undefined,
  });

  // A proposal this view decided leaves the daemon's pending list, but stays here, as Resolved, for the session.
  const seen = useRef(new Map<string, ProposalItem>());
  const listed = decisions.status === "ok" ? decisions.data.proposals : NO_PROPOSALS;
  for (const item of listed) seen.current.set(item.proposal.id, item);
  const proposalItems = useMemo(() => {
    const open = listed.filter((item) => item.proposal.state === "pending" && !decided.confirmed.has(item.proposal.id));
    const done = [...decided.confirmed.values()].flatMap((proposal) => {
      const was = seen.current.get(proposal.id);
      return was ? [{ ...was, proposal }] : [];
    });
    return [...open, ...done];
  }, [listed, decided.confirmed]);

  const taskById = useMemo(() => new Map((tasks.data ?? []).map((task) => [task.contract.id, task])), [tasks.data]);
  const items = useMemo(
    () =>
      buildQueue({
        proposals: proposalItems,
        questions: questions.items,
        candidates: queue.status === "ready" ? queue.candidates : [],
        resolved,
        other: queue.status === "ready" ? queue.other : [],
        pulls: pulls.rows,
        holds: holds.items,
        held: holds.held,
        projectOfTask: (taskId) => taskById.get(taskId)?.contract.project_id ?? null,
      }),
    [proposalItems, questions.items, queue, resolved, pulls.rows, holds.items, holds.held, taskById],
  );

  // The control a person used leaves the page once the daemon confirms. Focus then goes to the row if it is still
  // listed (a refusal), else to the list.
  const panelRef = useRef<HTMLDivElement>(null);
  const startedFrom = useRef<string | null>(null);
  const wasBusy = useRef(false);
  useEffect(() => {
    if (holds.busy !== null) {
      wasBusy.current = true;
      return;
    }
    if (!wasBusy.current) return;
    wasBusy.current = false;
    const root = panelRef.current;
    const row = Array.from(root?.querySelectorAll<HTMLElement>("[data-row-key]") ?? []).find((el) => el.dataset.rowKey === startedFrom.current);
    (row ?? root)?.focus();
  }, [holds.busy]);

  const env: RowEnv = {
    fixture: client.provenance === "fixture",
    names: NO_NAMES,
    canDecide: !live || (gate.can("orchestrator_actions") && gate.connection.approver),
    connected: !live || gate.connected,
    approver: !live || gate.connection.approver,
    decisions: decided,
    questions,
    holds,
    pulls,
    start: (key) => {
      startedFrom.current = key;
    },
    open: (taskId) => router.navigate({ name: "global", view: "review", taskId }),
    taskTitle: (taskId) => taskById.get(taskId)?.contract.objective ?? null,
  };

  const tabs = TABS.filter((tab) => tab.id !== "held" || holds.supported);
  const heldCount = holds.loading || holds.unavailable !== null ? null : items.filter((item) => item.tab === "held").length;
  const countOf = (tab: QueueTab) => (tab === "needs" ? waiting : tab === "held" ? heldCount : null);
  const projects = projectsOf(items);
  const loaded = queue.status === "ready";

  const notices = [
    queue.status === "error" ? `Pending decisions are unavailable (${queue.kind}): ${queue.message}. No data is shown instead; retry with Refresh.` : null,
    holds.unavailable !== null ? `Held decisions are not available: ${holds.unavailable}` : null,
    questions.unavailable !== null ? `Questions are not available: ${questions.unavailable}` : null,
    pulls.unavailable !== null ? `Pull requests are not available: ${pulls.unavailable}` : null,
    decisions.status !== "ok" && decisions.status !== "loading" ? `Proposed actions are not available: ${decisions.reason}` : null,
  ].filter((text): text is string => text !== null);

  /** Refresh reads the pending decisions again; a refusal says so beside the button, and names the next step. */
  const refreshPending = () =>
    void refreshList.run("Refreshing…", async () => {
      try {
        await refresh();
      } catch (error) {
        const { reason, nextStep } = presentError(asDaemonError(error));
        throw new Error(`Pending decisions were not refreshed. ${reason} ${nextStep}`);
      }
      return { kind: "saved", text: "Pending decisions refreshed." };
    });

  return (
    <section className="rq" aria-label="Review" data-testid="review-queue">
      <header className="rq-head">
        <h1>Review</h1>
        {waiting !== null && <span className="rq-waiting">{waiting} waiting</span>}
        <span className="rq-spacer" />
        <Button variant="quiet" pending={refreshList.pending} onClick={refreshPending}>
          Refresh
        </Button>
        <ActionStatus action={refreshList} />
      </header>
      {notices.map((text) => (
        <p key={text} role="alert">
          {text}
        </p>
      ))}
      {holds.blocked !== null && holds.supported && (
        <p id={HOLD_REASON_ID} className="rq-note">
          {holds.blocked}
        </p>
      )}
      <div ref={panelRef} tabIndex={-1} className="rq-panel">
        {/* Outside the tabs, so a result stays on screen when the row it came from moves to another tab. */}
        <div className="rq-outcomes" data-testid="queue-status">
          <ActionStatus action={holds.action} />
          <ActionStatus action={questions.action} />
          <ActionStatus action={pulls.action} />
        </div>
        <WsTabs
          label="Review queue"
          actions={
            <div className="rq-filters">
              <select aria-label="Project" value={project ?? ""} onChange={(event) => setProject(event.target.value || null)}>
                <option value="">All projects</option>
                {projects.map((id) => (
                  <option key={id} value={id}>
                    {basename(id)}
                  </option>
                ))}
                {project !== null && !projects.includes(project) && <option value={project}>{basename(project)}</option>}
              </select>
              <select aria-label="Type" value={type ?? ""} onChange={(event) => setType((event.target.value || null) as ItemType | null)}>
                <option value="">All types</option>
                {TYPES.map((id) => (
                  <option key={id} value={id}>
                    {TYPE_LABEL[id]}
                  </option>
                ))}
              </select>
            </div>
          }
          tabs={tabs.map(({ id, label }) => {
            const count = countOf(id);
            const shown = filterQueue(items, { tab: id, project, type });
            return {
              id,
              label: count === null ? label : `${label} ${count}`,
              content:
                shown.length === 0 ? (
                  loaded && <EmptyState title={EMPTY[id].title} description={EMPTY[id].description} />
                ) : (
                  <ul className="rq-rows" aria-label={`${label} items`}>
                    {shown.map((item: QueueItem) => (
                      <QueueRow key={item.key} item={item} env={env} />
                    ))}
                  </ul>
                ),
            };
          })}
        />
      </div>
    </section>
  );
}
