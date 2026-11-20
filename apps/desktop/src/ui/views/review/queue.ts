// Pure model of the Review queue: every item that needs the user, in one list. An item is one of four types and
// sits in one of three tabs. Nothing here reads the daemon; the view passes in what each source returned.
import type { Approval } from "../../../daemon/types";
import type { HeldSets } from "../../../sources/surfaces/holds";
import type { HoldItem, ProposalItem, PullItem, QuestionItem } from "../../../sources/surfaces/types";

export type ItemType = "approval" | "question" | "candidate" | "pr";
export type QueueTab = "needs" | "held" | "resolved";

export const TYPE_LABEL: Record<ItemType, string> = {
  approval: "Approval",
  question: "Question",
  candidate: "Candidate",
  pr: "PR",
};

export const TABS: ReadonlyArray<{ id: QueueTab; label: string }> = [
  { id: "needs", label: "Needs you" },
  { id: "held", label: "Held" },
  { id: "resolved", label: "Resolved" },
];

interface Base {
  /** Stable across reads; the React key and the focus anchor. */
  key: string;
  tab: QueueTab;
  /** The project the item belongs to, when a source says; an item with none shows under "All projects" only. */
  projectId: string | null;
  /** The day a hold ends, for an item in the Held tab. */
  heldUntil: string | null;
  /** The hold that keeps the item in the Held tab, so it can be released. */
  hold: HoldItem | null;
}

export type QueueItem =
  /** A proposed action from the orchestrator: decided inline. */
  | (Base & { type: "approval"; source: "proposal"; proposal: ProposalItem })
  /** A pending approval of a kind this app cannot decide; it is listed so none is silently dropped. */
  | (Base & { type: "approval"; source: "daemon"; approval: Approval })
  | (Base & { type: "question"; question: QuestionItem })
  | (Base & { type: "candidate"; approval: Approval })
  | (Base & { type: "pr"; pull: PullItem });

export interface QueueInput {
  /** Pending and decided proposals, newest first as given. A pending one is Needs you, a decided one Resolved. */
  proposals: readonly ProposalItem[];
  questions: readonly QuestionItem[];
  /** Pending approvals of a delivery kind, and the decisions this session already made. */
  candidates: readonly Approval[];
  resolved: readonly Approval[];
  /** Pending approvals of any other kind. */
  other: readonly Approval[];
  pulls: readonly PullItem[];
  holds: readonly HoldItem[];
  held: HeldSets;
  projectOfTask: (taskId: string) => string | null;
}

function holdFor(holds: readonly HoldItem[], kind: string, subjectId: string): HoldItem | null {
  return holds.find((hold) => hold.state === "active" && hold.subjectKind === kind && hold.subjectId === subjectId) ?? null;
}

function projectOfProposal(item: ProposalItem): string | null {
  return item.scope.kind === "project" ? item.scope.projectId : null;
}

/**
 * Items by type in a fixed order (approvals, questions, candidates, other approvals, pull requests), each type newest
 * first: the daemon lists oldest first, and only proposed actions carry a time. A hold moves an item to Held.
 */
export function buildQueue(input: QueueInput): QueueItem[] {
  const { holds, held, projectOfTask } = input;
  const items: QueueItem[] = [];
  const newestFirst = <T,>(list: readonly T[]) => [...list].reverse();

  for (const proposal of input.proposals) {
    const pending = proposal.proposal.state === "pending";
    const hold = pending ? holdFor(holds, "proposal", proposal.proposal.id) : null;
    items.push({
      type: "approval",
      source: "proposal",
      key: `proposal:${proposal.proposal.id}`,
      tab: !pending ? "resolved" : hold ? "held" : "needs",
      projectId: projectOfProposal(proposal),
      heldUntil: hold?.untilDate ?? null,
      hold,
      proposal,
    });
  }
  for (const question of newestFirst(input.questions)) {
    items.push({
      type: "question",
      key: `question:${question.id}`,
      tab: question.deferredUntil !== null ? "held" : "needs",
      projectId: projectOfTask(question.taskId),
      heldUntil: question.deferredUntil,
      hold: null,
      question,
    });
  }
  const pendingApproval = (approval: Approval) => {
    const hold = held.tasks.has(approval.action.task_id) ? holdFor(holds, "approval", approval.action.task_id) : null;
    return { tab: (hold ? "held" : "needs") as QueueTab, hold, heldUntil: hold?.untilDate ?? null };
  };
  for (const approval of newestFirst(input.candidates)) {
    items.push({
      type: "candidate",
      key: `approval:${approval.id}`,
      projectId: projectOfTask(approval.action.task_id),
      approval,
      ...pendingApproval(approval),
    });
  }
  for (const approval of newestFirst(input.resolved)) {
    if (approval.state.state === "requested") continue;
    items.push({
      type: "candidate",
      key: `approval:${approval.id}`,
      tab: "resolved",
      projectId: projectOfTask(approval.action.task_id),
      heldUntil: null,
      hold: null,
      approval,
    });
  }
  for (const approval of newestFirst(input.other)) {
    items.push({
      type: "approval",
      source: "daemon",
      key: `approval:${approval.id}`,
      projectId: projectOfTask(approval.action.task_id),
      approval,
      ...pendingApproval(approval),
    });
  }
  for (const pull of input.pulls) {
    items.push({
      type: "pr",
      key: `pr:${pull.taskId}`,
      tab: pull.state === "open" ? "needs" : "resolved",
      projectId: projectOfTask(pull.taskId),
      heldUntil: null,
      hold: null,
      pull,
    });
  }
  return items;
}

export interface QueueFilter {
  tab: QueueTab;
  /** `null` is "All projects". */
  project: string | null;
  /** `null` is every type. */
  type: ItemType | null;
}

export function filterQueue(items: readonly QueueItem[], filter: QueueFilter): QueueItem[] {
  return items.filter(
    (item) =>
      item.tab === filter.tab &&
      (filter.project === null || item.projectId === filter.project) &&
      (filter.type === null || item.type === filter.type),
  );
}

/** The projects that have an item, for the project filter. */
export function projectsOf(items: readonly QueueItem[]): string[] {
  return [...new Set(items.flatMap((item) => (item.projectId === null ? [] : [item.projectId])))].sort();
}

export function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

export const KIND_LABEL: Record<string, string> = {
  LocalReviewMerge: "Local review merge",
  PullRequestCreate: "Pull request creation",
  ReportPublish: "Report publication",
  policy_change: "Policy change",
};

export const STATE_LABEL: Record<Approval["state"]["state"], string> = {
  requested: "Waiting for you",
  approved: "Accepted",
  rejected: "Rejected",
  superseded: "Superseded",
};
