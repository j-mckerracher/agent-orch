/**
 * The approval card's state and copy as plain functions of one proposal (DPES-1105). No React: the card
 * renders these, and the tests read them directly. The daemon's answer is the only state shown; nothing here
 * guesses an outcome.
 */
import { presentError, type DaemonError } from "../../../daemon/errors";
import type { ActionProposal } from "../../../sources/surfaces/types";
import type { Tone } from "../../theme";

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
};

const FAILURE_TEXT: Readonly<Record<string, string>> = {
  stale_revision: "A newer change to this workstream exists. Ask the orchestrator to propose again.",
  not_found: "The workstream, objective or gate this names no longer exists. Nothing was changed.",
  conflict: "The workstream is not in a state that allows this change, for example it is archived. Nothing was changed.",
  invalid: "The daemon could not apply these values. Nothing was changed. Ask the orchestrator to propose again.",
  dependency_not_executed: "The workstream this needs was not created, so this did not run. Nothing was changed.",
  internal: "The app could not finish this change. Nothing else was changed. Ask the orchestrator to propose it again.",
};

const NO_NAMES: ReadonlyMap<string, string> = new Map();

/** The workstream titles by daemon id. An id with no title is shown as the id. */
export type WorkstreamNames = ReadonlyMap<string, string>;

export interface Field {
  label: string;
  value: string | string[];
}

export interface ViewContext {
  /** False while the daemon cannot take a decision, for example when disconnected. */
  canDecide: boolean;
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

/** The name of what a proposal changes: a title for a create or an objective, the workstream's title for an update. */
function subjectOf(p: ActionProposal, names: WorkstreamNames): string {
  if (p.kind === "workstream.update") {
    const id = text(p.params.workstream_id);
    return names.get(id) ?? id;
  }
  if (p.kind === "objective.revise") return text(p.params.objective_id);
  return text(p.params.title);
}

/** "Create workstream: Post-MVP work", the label a dependent card gives its parent. */
function parentLabel(parent: ActionProposal): string {
  return `${copyOf(parent.kind)?.heading ?? parent.kind}: ${subjectOf(parent, NO_NAMES)}`;
}

function field(label: string, value: string | string[]): Field {
  return { label, value: value.length === 0 ? "(none)" : value };
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
 * `parent` names the workstream an objective goes into when the objective does not name one yet.
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
    case "objective.add": {
      const id = text(x.workstream_id);
      const workstream =
        id !== "" ? (names.get(id) ?? id) : parent !== null ? `The workstream from “${parentLabel(parent)}”` : "";
      return [
        field("Workstream", workstream),
        field("Title", text(x.title)),
        field("Description", text(x.description)),
        field("Gates", gatesOf(x.gates)),
      ];
    }
    case "objective.revise":
      return [
        field("Objective", text(x.objective_id)),
        field("Based on revision", text(x.expected_revision)),
        field("New title", text(x.title)),
        field("New description", text(x.description)),
        field("Add gates", gatesOf(x.add_gates)),
      ];
    default:
      return Object.keys(x)
        .sort()
        .map((key) => field(key, JSON.stringify(x[key])));
  }
}

/** Plain words for a failed execution, by the daemon's error code. */
export function failureText(p: ActionProposal): string {
  const code = p.errorCode;
  if (code === "stale_revision" && p.kind === "objective.revise") {
    return "A newer change to this objective exists. Ask the orchestrator to propose again.";
  }
  if (code !== null && Object.prototype.hasOwnProperty.call(FAILURE_TEXT, code)) return FAILURE_TEXT[code];
  return code === null ? "The action failed." : `The action failed (${code}).`;
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
    default:
      return null;
  }
}

function executedText(p: ActionProposal, names: WorkstreamNames): string {
  switch (p.kind) {
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

/**
 * A pending card. Approve is off while its parent has not executed, the kind is unknown, or the daemon cannot be
 * reached. A dependent whose parent is not in the list waits too: its parent's state is unknown, so the card cannot
 * say it is ready.
 */
function pendingView(p: ActionProposal, parent: ActionProposal | null, ctx: ViewContext): CardView {
  const waiting = p.dependsOn !== null && parent?.state !== "executed";
  const waitingText =
    parent !== null
      ? `Approve “${parentLabel(parent)}” first. This objective goes into that workstream.`
      : "This objective goes into a workstream that is not created yet. Approve that workstream first.";
  const approveBlock = ctx.deciding
    ? "Sending your decision…"
    : !ctx.canDecide
      ? "Reconnect to the daemon to approve or reject."
      : copyOf(p.kind) === null
        ? "This app version cannot show this action. Update the app to approve it."
        : waiting
          ? waitingText
          : null;
  const variant: "primary" | "danger" = p.kind === "workstream.update" && p.params.state === "archived" ? "danger" : "primary";
  const rejectEnabled = ctx.canDecide && !ctx.deciding;
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
      return closedView(state, { tone: "info", text: "Approved, running" }, { tone: "info", text: "The daemon is running this action." });
    case "executed":
      return closedView(
        state,
        { tone: "ok", text: copyOf(p.kind)?.done ?? "Executed" },
        { tone: "ok", text: executedText(p, ctx.names) },
      );
    case "rejected":
      return closedView(state, { tone: "neutral", text: "Rejected" }, { tone: "neutral", text: "You rejected this. Nothing was changed." });
    case "failed":
      return closedView(state, { tone: "bad", text: "Failed" }, { tone: "bad", text: failureText(p) });
    case "superseded":
      return closedView(
        state,
        { tone: "neutral", text: "Superseded" },
        { tone: "neutral", text: "A newer proposal replaced this one. Nothing was changed." },
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
  switch (stateOf(p)) {
    case "rejected":
      return `Rejected: ${phrase}. Nothing was changed.`;
    case "failed":
      return `Failed: ${phrase}. ${failureText(p)}`;
    default:
      return viewOf(p, parent, { canDecide: true, deciding: false, names }).message.text;
  }
}
