/**
 * The approval card's state and copy as plain functions of one proposal (DPES-1105). No React: the card
 * renders these, and the tests read them directly. The daemon's answer is the only state shown; nothing here
 * guesses an outcome.
 */
import { APPROVER_REQUIRED_NOTICE, presentError, type DaemonError } from "../../../daemon/errors";
import type { ActionProposal } from "../../../sources/surfaces/types";
import type { Tone } from "../../theme";
import { formatHoldDay } from "../review/holdDates";
import { agoText, checkLine, mergeMethodText, mergeVerdict, plainRemote, shortSha } from "../review/pullsModel";

export type CardState = "pending" | "approved" | "executed" | "rejected" | "failed" | "superseded" | "expired" | "unknown";

const KNOWN_STATES: readonly string[] = ["pending", "approved", "executed", "rejected", "failed", "superseded", "expired"];

interface KindCopy {
  /** The card heading after "Proposed: ". Lower-cased, it is the verb phrase the announcements use. */
  heading: string;
  /** The pill text once the daemon has executed the change. */
  done: string;
}

const KINDS: Readonly<Record<string, KindCopy>> = {
  "workstream.create": { heading: "Create workstream", done: "Created" },
  "workstream.update": { heading: "Update workstream", done: "Updated" },
  "objective.add": { heading: "Add objective", done: "Created" },
  "objective.revise": { heading: "Revise objective", done: "Revised" },
  "task.propose": { heading: "Create task", done: "Queued" },
  // The daemon only confirms it asked the task to stop (decisions-2 Q11), so the card never says "Cancelled".
  "task.cancel": { heading: "Cancel task", done: "Cancellation requested" },
  "task.retry": { heading: "Retry task", done: "Retry queued" },
  "workstream.link_task": { heading: "Link task", done: "Linked" },
  "task.revise": { heading: "Revise task", done: "Revision started" },
  "decision.hold": { heading: "Hold decision", done: "Held" },
  "decision.ask": { heading: "Ask for your decision", done: "Asked" },
  "task.message": { heading: "Message the waiting worker", done: "Sent" },
  "project.remove": { heading: "Remove project", done: "Removed" },
  "delivery.merge": { heading: "Merge pull request", done: "Merged" },
};

/** The kinds that act on a task. Their failures read in task words. */
const TASK_KINDS: readonly string[] = ["task.propose", "task.cancel", "task.retry", "workstream.link_task"];

const FAILURE_TEXT: Readonly<Record<string, string>> = {
  stale_revision: "A newer change to this workstream exists. Ask the orchestrator to propose again.",
  not_found: "The workstream, objective or gate this names no longer exists. Nothing was changed.",
  conflict: "The workstream is not in a state that allows this change, for example it is archived. Nothing was changed.",
  invalid: "The daemon could not apply these values. Nothing was changed. Ask the orchestrator to propose again.",
  dependency_not_executed: "The workstream this needs was not created, so this did not run. Nothing was changed.",
  forbidden: "The app does not allow this change. Nothing was changed.",
  grant_revoked: "The standing approval was revoked before this ran. Nothing was changed. Ask the orchestrator to propose again, or approve it yourself.",
  grant_expired: "The standing approval had expired before this ran. Nothing was changed. Ask the orchestrator to propose again, or approve it yourself.",
  internal: "The app could not finish this change. Nothing else was changed. Ask the orchestrator to propose it again.",
};

/** Replaces `FAILURE_TEXT` for a task kind, where the workstream wording would mislead. */
const TASK_FAILURE_TEXT: Readonly<Record<string, string>> = {
  not_found: "The project, task or workstream this names was not found. Nothing was changed.",
  conflict:
    "The task, workstream or link is not in a state that allows this (for example the task is already running or finished). Nothing was changed.",
};

/** Replaces both tables above for a kind whose failures need their own words. */
const KIND_FAILURE_TEXT: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "task.revise": {
    not_found: "The task is gone. Nothing was changed.",
    conflict: "The task has already finished, so it cannot be revised. Ask the orchestrator to create a new task instead. Nothing was changed.",
    invalid:
      "The app could not revise this task: the request is outside this conversation, or the task text would be too long. Nothing was changed.",
  },
  "decision.hold": {
    not_found: "The task is gone, or it has no decision waiting. Nothing was changed.",
    conflict: "The task has already finished, so there is nothing to hold. Nothing was changed.",
    invalid:
      "The date is not allowed (it must be from today to 60 days from today), or the request is outside this conversation. Nothing was changed.",
  },
  // Each merge failure says whether the host was asked: only `merge_unconfirmed` leaves that open.
  "delivery.merge": {
    head_moved: "The pull request changed after you were shown it. Nothing was merged. Ask the orchestrator to propose again.",
    checks_red: "A required check failed. Nothing was merged.",
    checks_pending: "A required check has not finished. Nothing was merged.",
    checks_missing: "No required check was reported, so the app does not merge. Nothing was merged.",
    not_open: "The pull request is no longer open. Nothing was merged.",
    not_mergeable: "GitHub says this cannot be merged now (for example a conflict or a required review). Nothing was merged.",
    stale: "The app last looked at this pull request too long ago. Nothing was merged.",
    merge_unconfirmed:
      "The app asked GitHub to merge but could not confirm it. Check the pull request on GitHub before asking again.",
    not_signed_in: "The GitHub command line is not signed in on this machine. Nothing was merged.",
    project_mode: "This project's mode is local-only. Nothing was merged.",
    delivery_not_authorized: "This task does not deliver by pull request, or it has not finished. Nothing was merged.",
    not_found: "The task or its pull request is no longer known. Nothing was merged.",
    conflict: "The task is not in a state that allows a merge. Nothing was merged.",
    invalid: "The app could not read this merge request. Nothing was merged. Ask the orchestrator to propose again.",
  },
  "project.remove": {
    not_found: "The project is gone. Nothing was changed.",
    conflict: "The project still has running work or decisions waiting, so it was not removed. Nothing was changed.",
    invalid: "The request is outside this conversation. Nothing was changed.",
  },
};

/** Why the app refused a task before it reached the user, by the daemon's `denial` key. */
const DENIAL_TEXT: Readonly<Record<string, string>> = {
  delivery_not_authorized:
    "This task asks for a delivery the app does not allow from chat. Nothing was created. Ask the orchestrator for local review or a report only.",
  autonomy_not_authorized: "This task asks for more autonomy than chat may request. Nothing was created.",
  verification_not_authorized: "This task asks for verification commands, which chat may not request. Nothing was created.",
  contract_invalid: "The task text is not valid. Nothing was created. Ask the orchestrator to propose again.",
};

const DENIAL_FALLBACK = "The app refused this task. Nothing was created.";
const MERGE_DENIAL_FALLBACK = "The app refused this merge. Nothing was merged.";

const REVISE_EFFECT =
  "The current attempt stops. Its work is kept. A new task starts with the old objective plus this instruction.";
const HOLD_EFFECT = "It stays in Review and Decisions, marked Held, and comes back on that day.";
const MERGE_WARNING = "Approving merges exactly this commit on GitHub and cannot be undone from here.";
const REMOVE_EFFECT =
  "The project leaves the project list. Its folder, worktrees and branches are not touched. Register the folder again to bring it back.";

/** The delivery a project mode asks for, in card words. An unknown mode is named as the daemon sent it. */
function modeDeliveryWords(mode: string | undefined): string {
  if (mode === "direct-pr" || mode === "no-mistakes") return "pull request";
  return mode === "local-only" ? "local review" : (mode ?? "not available");
}

function deliveryWords(delivery: string): string {
  if (delivery === "LocalReview") return "local review";
  if (delivery === "ReportOnly") return "report only";
  return delivery === "PullRequest" ? "pull request" : delivery;
}

/** One sentence for the "Below this project's default" row: text, so the card never leans on colour. */
function belowDefaultText(mode: string | undefined, delivery: string): string {
  return `Project default: ${modeDeliveryWords(mode)}. This task: ${deliveryWords(delivery)}.`;
}

/** The longest subject shown in a heading phrase, a button name or an announcement. */
const SUBJECT_CHARS = 60;

const NO_NAMES: ReadonlyMap<string, string> = new Map();

/** The workstream titles by daemon id. An id with no title is shown as the id. */
export type WorkstreamNames = ReadonlyMap<string, string>;

export interface Field {
  label: string;
  value: string | string[];
}

export interface ViewContext {
  /** False while the daemon cannot take a decision, for example when disconnected or not an approver. */
  canDecide: boolean;
  /** False while the daemon session is not connected. */
  connected: boolean;
  /** False when this app did not start the running daemon, so it cannot approve or reject for it (DPES-1105). */
  approver: boolean;
  /** True while this card's decision is in flight. */
  deciding: boolean;
  names: WorkstreamNames;
}

export interface Control {
  enabled: boolean;
  /** The visible reason a control is off; null when it is on or the card shows no control. */
  reason: string | null;
}

export interface CardView {
  state: CardState;
  pill: { tone: Tone; text: string };
  message: { tone: Tone; text: string };
  approve: Control & { variant: "primary" | "danger" };
  reject: Control;
}

function copyOf(kind: string): KindCopy | null {
  return Object.prototype.hasOwnProperty.call(KINDS, kind) ? KINDS[kind] : null;
}

function text(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return "";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** A gate is a string in the design's example, and `{ description }` once the daemon stores it. */
function gateText(gate: unknown): string {
  if (isRecord(gate) && typeof gate.description === "string") return gate.description;
  return typeof gate === "string" ? gate : JSON.stringify(gate);
}

function gatesOf(value: unknown): string[] {
  return Array.isArray(value) ? value.map(gateText) : [];
}

function lookup(table: Readonly<Record<string, string>>, key: string | null): string | null {
  return key !== null && Object.prototype.hasOwnProperty.call(table, key) ? table[key] : null;
}

/** One line of at most `SUBJECT_CHARS` characters, cut on a character boundary. */
function shorten(value: string): string {
  const line = value.replace(/\s+/g, " ").trim();
  const chars = Array.from(line);
  return chars.length > SUBJECT_CHARS ? `${chars.slice(0, SUBJECT_CHARS).join("").trimEnd()}…` : line;
}

/** A task the app refused when the reply arrived. It is never pending, so it has no decision. */
function isDenied(p: ActionProposal): boolean {
  return p.state === "failed" && p.errorCode === "action_denied";
}

function denialText(p: ActionProposal): string {
  if (p.kind === "delivery.merge") return lookup(KIND_FAILURE_TEXT["delivery.merge"], p.denial) ?? MERGE_DENIAL_FALLBACK;
  return lookup(DENIAL_TEXT, p.denial) ?? DENIAL_FALLBACK;
}

/**
 * The name of what a proposal changes: a title for a create or an objective, the workstream's title for an update,
 * the start of the objective for a new task, and the task's title (else its id) for a cancel, retry or link.
 */
function subjectOf(p: ActionProposal, names: WorkstreamNames): string {
  if (p.kind === "workstream.update") {
    const id = text(p.params.workstream_id);
    return names.get(id) ?? id;
  }
  if (p.kind === "objective.revise") return text(p.params.objective_id);
  if (p.kind === "task.propose") return shorten(text(p.params.objective));
  if (
    p.kind === "task.cancel" ||
    p.kind === "task.retry" ||
    p.kind === "workstream.link_task" ||
    p.kind === "task.revise" ||
    p.kind === "decision.hold" ||
    p.kind === "decision.ask" ||
    p.kind === "task.message"
  ) {
    return shorten(p.target?.taskTitle || text(p.params.task_id));
  }
  if (p.kind === "project.remove") return shorten(p.target?.projectName || text(p.params.project));
  // The number and the commit name the merge in a button, so the title (remote text) stays out of it.
  if (p.kind === "delivery.merge") return `${text(p.params.pr_number)} at commit ${shortSha(text(p.params.head_sha))}`;
  return text(p.params.title);
}

/** "Create workstream: Post-MVP work", the label a dependent card gives its parent. */
function parentLabel(parent: ActionProposal): string {
  return `${copyOf(parent.kind)?.heading ?? parent.kind}: ${subjectOf(parent, NO_NAMES)}`;
}

function field(label: string, value: string | string[]): Field {
  return { label, value: value.length === 0 ? "(none)" : value };
}

/** The text one entry per non-blank line, so a long task is shown whole and stays readable. */
function linesOf(value: string): string[] {
  return value.split(/\r\n|\r|\n/).filter((line) => line.trim() !== "");
}

/** The workstream a new objective or task goes into: named by id, or by the proposal that creates it. */
function workstreamOf(id: string, names: WorkstreamNames, parent: ActionProposal | null): string {
  if (id !== "") return names.get(id) ?? id;
  return parent !== null ? `The workstream from “${parentLabel(parent)}”` : "";
}

/** The state the daemon reported, or `unknown` for a value this app version does not know. */
export function stateOf(p: ActionProposal): CardState {
  return KNOWN_STATES.includes(p.state) ? (p.state as CardState) : "unknown";
}

/** The later of the transcript's record and the daemon's answer to a decision, by `updatedAt`. */
export function newerRecord(listed: ActionProposal, confirmed: ActionProposal | undefined): ActionProposal {
  if (confirmed === undefined) return listed;
  return Date.parse(confirmed.updatedAt) >= Date.parse(listed.updatedAt) ? confirmed : listed;
}

/** "Proposed: Create workstream". The heading never changes with the state, so the card's name stays stable. */
export function cardTitle(kind: string): string {
  return `Proposed: ${copyOf(kind)?.heading ?? "Unknown action"}`;
}

/** The lower-case action phrase used in button names and announcements, for example "create workstream Post-MVP work". */
export function actionPhrase(p: ActionProposal, names: WorkstreamNames): string {
  const copy = copyOf(p.kind);
  if (copy === null) return `action ${p.kind}`;
  return `${copy.heading.toLowerCase()} ${subjectOf(p, names)}`.trim();
}

/**
 * The exact fields of a proposal, read from its canonical parameters. Free text is shown as the daemon stored it.
 * `parent` names the workstream an objective or a task goes into when it does not name one yet.
 */
export function fieldsOf(p: ActionProposal, names: WorkstreamNames, parent: ActionProposal | null): Field[] {
  const x = p.params;
  switch (p.kind) {
    case "workstream.create":
      return [field("Title", text(x.title)), field("Summary", text(x.summary))];
    case "workstream.update": {
      const id = text(x.workstream_id);
      return [
        field("Workstream", names.get(id) ?? id),
        field("Based on revision", text(x.expected_revision)),
        field("New title", text(x.title)),
        field("New summary", text(x.summary)),
        field("New state", text(x.state)),
      ];
    }
    case "objective.add":
      return [
        field("Workstream", workstreamOf(text(x.workstream_id), names, parent)),
        field("Title", text(x.title)),
        field("Description", text(x.description)),
        field("Gates", gatesOf(x.gates)),
      ];
    case "objective.revise":
      return [
        field("Objective", text(x.objective_id)),
        field("Based on revision", text(x.expected_revision)),
        field("New title", text(x.title)),
        field("New description", text(x.description)),
        field("Add gates", gatesOf(x.add_gates)),
      ];
    case "task.propose": {
      const project = text(x.project);
      const name = p.target?.projectName;
      const workstream = workstreamOf(text(x.workstream_id), names, parent);
      const objective = text(x.objective_id);
      return [
        field("Project", name ? `${name} (${project})` : project),
        field("Task", linesOf(text(x.objective))),
        field("Agent", text(x.agent)),
        field("Task kind", text(x.task_kind)),
        field("Delivery", text(x.delivery)),
        ...(p.target?.belowProjectDefault ? [field("Below this project's default", belowDefaultText(p.target.projectMode, text(x.delivery)))] : []),
        // The envelope fixes both: chat cannot raise them, and the card says so.
        field("Autonomy", "Approval required, fixed by the app"),
        field("Verification", "None, fixed by the app"),
        ...(workstream !== "" ? [field("Workstream", workstream)] : []),
        ...(objective !== "" ? [field("Objective", objective)] : []),
      ];
    }
    case "task.cancel":
    case "task.retry":
      return [
        field("Task title", p.target?.taskTitle ?? "(not available)"),
        field("Task state now", p.target?.taskState ?? "(not available)"),
        field("Task id", text(x.task_id)),
      ];
    case "workstream.link_task": {
      const id = text(x.workstream_id);
      return [
        field("Workstream", names.get(id) ?? id),
        field("Task title", p.target?.taskTitle ?? "(not available)"),
        field("Task id", text(x.task_id)),
        field("Objective", text(x.objective_id)),
      ];
    }
    case "task.revise":
      return [
        field("Task title", p.target?.taskTitle ?? "(not available)"),
        field("Task state now", p.target?.taskState ?? "(not available)"),
        field("Added instruction", linesOf(text(x.added_intent))),
        field("What happens", REVISE_EFFECT),
        field("Task id", text(x.task_id)),
      ];
    case "decision.hold":
      return [
        field("Task title", p.target?.taskTitle ?? "(not available)"),
        field("Task state now", p.target?.taskState ?? "(not available)"),
        field("Until", formatHoldDay(text(x.until), true)),
        field("What happens", HOLD_EFFECT),
        field("Task id", text(x.task_id)),
      ];
    case "decision.ask":
      return [
        field("Task title", p.target?.taskTitle ?? "(not available)"),
        field("Task state now", p.target?.taskState ?? "(not available)"),
        field("Question", linesOf(text(x.question))),
        field("Options", Array.isArray(x.options) ? x.options.map(text) : []),
        field("Task id", text(x.task_id)),
      ];
    case "task.message":
      return [
        field("Task title", p.target?.taskTitle ?? "(not available)"),
        field("Task state now", p.target?.taskState ?? "(not available)"),
        field("Message", linesOf(text(x.text))),
        field("Task id", text(x.task_id)),
      ];
    case "delivery.merge": {
      const pull = p.target?.pull;
      const verdict = mergeVerdict(pull, p.denial);
      return [
        field("Task title", plainRemote(p.target?.taskTitle ?? "(not available)")),
        field("Pull request", text(x.pr_number)),
        field("Commit", shortSha(text(x.head_sha))),
        field("Merge method", pull === undefined ? "(not available)" : mergeMethodText(pull.mergeMethod)),
        field("Checks", pull === undefined ? [] : pull.checks.map(checkLine)),
        field("Check summary", verdict?.text ?? "(not available)"),
        ...(pull === undefined ? [] : [field("Seen", agoText(pull.observedAgeSecs))]),
        field("What happens", MERGE_WARNING),
        field("Task id", text(x.task_id)),
      ];
    }
    case "project.remove": {
      const project = text(x.project);
      const name = p.target?.projectName;
      const count = p.target?.taskCount;
      const running = p.target?.runningTasks;
      return [
        field("Project", name ? `${name} (${project})` : project),
        field(
          "Tasks",
          count === undefined || running === undefined
            ? "(not available)"
            : `${count} ${count === 1 ? "task is" : "tasks are"} kept in history. Running tasks: ${running}`,
        ),
        field("What happens", REMOVE_EFFECT),
      ];
    }
    default:
      return Object.keys(x)
        .sort()
        .map((key) => field(key, JSON.stringify(x[key])));
  }
}

/** Plain words for a failed execution, by the daemon's error code. */
export function failureText(p: ActionProposal): string {
  const code = p.errorCode;
  if (code === "action_denied") return denialText(p);
  const own = Object.prototype.hasOwnProperty.call(KIND_FAILURE_TEXT, p.kind) ? lookup(KIND_FAILURE_TEXT[p.kind], code) : null;
  if (own !== null) return own;
  if (code === "stale_revision" && p.kind === "objective.revise") {
    return "A newer change to this objective exists. Ask the orchestrator to propose again.";
  }
  const known = (TASK_KINDS.includes(p.kind) ? lookup(TASK_FAILURE_TEXT, code) : null) ?? lookup(FAILURE_TEXT, code);
  if (known !== null) return known;
  return code === null ? "The action failed." : `The action failed (${code}).`;
}

/** A short grant id is enough to tell two apart; the full id is never shown. */
const GRANT_LABEL_CHARS = 10;

/**
 * The line under a card whose action a standing approval approved, with the grant's short label. Null for a card a
 * person approved, one nobody approved yet, or an older daemon that does not say. Read from the proposal event's actor
 * (`grant:<id>`); the id is cut short and never shown as a path or a token.
 */
export function approvedByText(p: ActionProposal): string | null {
  if (p.approvedBy?.kind !== "grant") return null;
  return `Approved by a standing approval (${p.approvedBy.grantId.slice(0, GRANT_LABEL_CHARS)}).`;
}

/** The inline text for a request that the daemon refused or never answered. */
export function requestErrorText(error: DaemonError, parent: ActionProposal | null): string {
  switch (error.kind) {
    case "proposal-not-found":
      return "This proposal no longer exists. The conversation history may have been cleared.";
    case "proposal-not-pending":
      return "This proposal was already decided. Showing the daemon's current state.";
    case "proposal-invalid":
      return parent !== null ? `Approve “${parentLabel(parent)}” first.` : "The daemon cannot approve this now.";
    case "idempotency-in-progress":
      return "Still finishing. Refresh to see the result, then try again if it is still pending.";
    case "timeout":
    case "disconnected":
      return "The daemon did not answer. Your decision may have been recorded. Trying again reuses the same request.";
    default:
      return presentError(error).reason;
  }
}

/** The workstream "Open workstream" goes to once the change has executed; null when the card cannot name one. */
export function openWorkstreamId(p: ActionProposal, parent: ActionProposal | null, targetWorkstream: string | null): string | null {
  switch (p.kind) {
    case "workstream.create":
      return p.resultRef;
    case "workstream.update":
      return text(p.params.workstream_id) || null;
    case "objective.add":
      return text(p.params.workstream_id) || parent?.resultRef || null;
    case "objective.revise":
      return targetWorkstream;
    case "task.propose":
      return text(p.params.workstream_id) || parent?.resultRef || null;
    case "workstream.link_task":
      return text(p.params.workstream_id) || null;
    default:
      return null;
  }
}

/** The task "Open task" goes to once the change has executed; null when the card names none. */
export function openTaskId(p: ActionProposal): string | null {
  switch (p.kind) {
    case "task.propose":
      return p.resultRef;
    case "task.cancel":
    case "task.retry":
    case "decision.hold":
    case "decision.ask":
    case "task.message":
    case "delivery.merge":
      return text(p.params.task_id) || null;
    case "task.revise":
      // The new task: the daemon's result is its id.
      return p.resultRef;
    default:
      return null;
  }
}

function executedText(p: ActionProposal, names: WorkstreamNames): string {
  switch (p.kind) {
    case "task.propose":
      return `Created and queued task “${subjectOf(p, names)}”. Open the task to follow it.`;
    case "task.cancel":
      return `Cancellation requested for “${subjectOf(p, names)}”. The task stops when its agent confirms.`;
    case "task.retry":
      return `Retry queued for “${subjectOf(p, names)}”.`;
    case "workstream.link_task":
      return `Linked “${subjectOf(p, names)}” to the workstream.`;
    case "task.revise":
      return `Revision started for “${subjectOf(p, names)}”. The old attempt is stopping and a new task is queued.`;
    case "decision.hold":
      return `Held the decision for “${subjectOf(p, names)}” until ${formatHoldDay(text(p.params.until), true)}.`;
    case "decision.ask":
      return `Asked you about “${subjectOf(p, names)}”. Answer it under Needs your decision in Review & Decisions.`;
    case "task.message":
      return `Sent the message to the waiting worker of “${subjectOf(p, names)}”. It continues with it.`;
    case "project.remove":
      return `Removed project “${subjectOf(p, names)}”. Its tasks stay in history.`;
    case "delivery.merge":
      return `Merged pull request ${text(p.params.pr_number)} at commit ${shortSha(text(p.params.head_sha))}.`;
    case "workstream.create":
      return `Created workstream “${text(p.params.title)}”.`;
    case "objective.add":
      return `Added objective “${text(p.params.title)}”.`;
    case "workstream.update":
      return `Updated workstream “${subjectOf(p, names)}”.`;
    case "objective.revise":
      return `Revised objective “${text(p.params.objective_id)}”.`;
    default:
      return "The daemon ran this action.";
  }
}

const NO_PULL_FACTS = "No pull request facts yet. Refresh the list.";

/** Why a pending merge card cannot be approved: the daemon says a merge would be refused now. Null for any other card. */
function mergeNotReady(p: ActionProposal): string | null {
  if (p.kind !== "delivery.merge") return null;
  const verdict = mergeVerdict(p.target?.pull, p.denial);
  if (verdict === null) return NO_PULL_FACTS;
  return verdict.ready ? null : verdict.text;
}

/**
 * A pending card. Approve is off while its parent has not executed, the kind is unknown, or the daemon cannot be
 * reached. A dependent whose parent is not in the list waits too: its parent's state is unknown, so the card cannot
 * say it is ready.
 */
function pendingView(p: ActionProposal, parent: ActionProposal | null, ctx: ViewContext): CardView {
  const waiting = p.dependsOn !== null && parent?.state !== "executed";
  const noun = p.kind === "task.propose" ? "task" : "objective";
  const waitingText =
    parent !== null
      ? `Approve “${parentLabel(parent)}” first. This ${noun} goes into that workstream.`
      : `This ${noun} goes into a workstream that is not created yet. Approve that workstream first.`;
  const approveBlock = ctx.deciding
    ? "Sending your decision…"
    : ctx.connected && !ctx.approver
      ? APPROVER_REQUIRED_NOTICE
      : !ctx.canDecide
        ? "Reconnect to the daemon to approve or reject."
        : copyOf(p.kind) === null
          ? "This app version cannot show this action. Update the app to approve it."
          : waiting
            ? waitingText
            : mergeNotReady(p);
  // Archiving, stopping a task and removing a project are hard to undo.
  const variant: "primary" | "danger" =
    (p.kind === "workstream.update" && p.params.state === "archived") ||
    p.kind === "task.cancel" ||
    p.kind === "task.revise" ||
    p.kind === "project.remove" ||
    p.kind === "delivery.merge"
      ? "danger"
      : "primary";
  const rejectEnabled = ctx.canDecide && ctx.approver && !ctx.deciding;
  return {
    state: "pending",
    pill: { tone: "info", text: "Pending approval" },
    message: { tone: approveBlock === null ? "info" : "warn", text: approveBlock ?? "Nothing runs until you approve." },
    approve: { enabled: approveBlock === null, reason: approveBlock, variant },
    reject: { enabled: rejectEnabled, reason: rejectEnabled ? null : approveBlock },
  };
}

function closedView(state: CardState, pill: CardView["pill"], message: CardView["message"]): CardView {
  const off: Control = { enabled: false, reason: null };
  return { state, pill, message, approve: { ...off, variant: "primary" }, reject: off };
}

/** What the card shows for a proposal: the pill, the message, and whether each decision is on. */
export function viewOf(p: ActionProposal, parent: ActionProposal | null, ctx: ViewContext): CardView {
  const state = stateOf(p);
  switch (state) {
    case "pending":
      return pendingView(p, parent, ctx);
    case "approved":
      return closedView(
        state,
        { tone: "info", text: "Approved, running" },
        {
          tone: "info",
          text:
            p.kind === "delivery.merge"
              ? "Approved. The app reads the pull request once more, then asks GitHub to merge this exact commit only if it is unchanged. Checking whether it merged can take a moment."
              : "The daemon is running this action.",
        },
      );
    case "executed":
      return closedView(
        state,
        { tone: "ok", text: copyOf(p.kind)?.done ?? "Executed" },
        { tone: "ok", text: executedText(p, ctx.names) },
      );
    case "rejected":
      return closedView(state, { tone: "neutral", text: "Rejected" }, { tone: "neutral", text: "You rejected this. Nothing was changed." });
    case "failed":
      // A denial is the app's refusal, not a fault: a warning, and nothing for the user to retry or approve.
      return isDenied(p)
        ? closedView(state, { tone: "warn", text: "Denied" }, { tone: "warn", text: failureText(p) })
        : closedView(state, { tone: "bad", text: "Failed" }, { tone: "bad", text: failureText(p) });
    case "superseded":
      return closedView(
        state,
        { tone: "neutral", text: "Superseded" },
        {
          tone: "neutral",
          text:
            p.kind === "delivery.merge"
              ? "Superseded: the pull request changed. Ask the orchestrator to propose again. Nothing was merged."
              : "A newer proposal replaced this one. Nothing was changed.",
        },
      );
    case "expired":
      return closedView(
        state,
        { tone: "warn", text: "Expired" },
        { tone: "warn", text: "This proposal expired. Nothing was changed. Ask the orchestrator to propose again." },
      );
    case "unknown":
      return closedView(
        state,
        { tone: "neutral", text: `Unknown state: ${p.state}` },
        { tone: "neutral", text: "This app version does not know this state." },
      );
  }
}

/** What the polite region says once the daemon has answered a decision. */
export function outcomeAnnouncement(p: ActionProposal, parent: ActionProposal | null, names: WorkstreamNames): string {
  const phrase = actionPhrase(p, names);
  // A task's words can already end in a full stop or an ellipsis.
  const sentence = /[.…]$/.test(phrase) ? phrase : `${phrase}.`;
  switch (stateOf(p)) {
    case "rejected":
      return `Rejected: ${sentence} Nothing was changed.`;
    case "failed":
      return `${isDenied(p) ? "Denied" : "Failed"}: ${sentence} ${failureText(p)}`;
    default:
      return viewOf(p, parent, { canDecide: true, connected: true, approver: true, deciding: false, names }).message.text;
  }
}
