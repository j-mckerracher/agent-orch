/**
 * Standing approvals on the Settings page (DPES-1105 Phase 5), as plain functions. No React: the section renders
 * these and the tests read them directly. The checks mirror the daemon's limits (`standing_grant.rs`) so the user
 * sees a message before sending; the daemon stays the authority and its refusal is shown as the daemon's.
 */
import { presentError, type DaemonError } from "../../../daemon/errors";
import type { GrantItem, GrantPolicy, GrantTerms } from "../../../sources/surfaces/types";
import type { Tone } from "../../theme";

export const SECTION_TITLE = "Standing approvals";
export const INTRO_TEXT =
  "A standing approval lets the orchestrator run certain kinds of actions without asking each time, only inside the project or workstream, time, count and token limits you set. You can revoke it at any time. Nothing is on until you create one.";
export const NEVER_GRANTED_TEXT = "These always need your approval: cancel a task, revise a task, remove a project, merge.";
export const TOKEN_NOTE =
  "The most tokens the tasks started under this approval may use. It counts the tokens reported so far, and tasks whose agent reports none count as zero. It blocks only new uses. It does not stop a task that is already running.";
export const UPDATE_NOTE = "Updating a workstream covers its title and summary only.";
export const REVOKE_LOCKED_NOTE = "Only the app that started the service can create or revoke standing approvals.";
export const CREATED_TEXT = "Standing approval created.";
export const FIXTURE_TEXT = "Fixture data: nothing was saved.";
export const INVALID_REASON = "Choose a project or workstream, at least one kind of action, and limits within the ranges shown.";
export const NOT_CONNECTED_REASON = "Reconnect to the daemon to create a standing approval.";

/** Time units the daemon works in. */
const HOUR_SECS = 3600;

/** What the form offers for a kind the daemon lists, in plain words. A kind with no row here is not offered. */
const KIND_LABELS: Readonly<Record<string, string>> = {
  "workstream.create": "Create a workstream",
  "workstream.update": "Update a workstream (title and summary only)",
  "objective.add": "Add an objective",
  "objective.revise": "Revise an objective",
  "task.propose": "Propose a task",
  "task.retry": "Retry a task",
  "workstream.link_task": "Link a task to a workstream",
  "decision.hold": "Hold a decision",
};

/** Mirrors the core's never-grantable list: never a control, whatever the daemon lists. */
const NEVER_GRANTABLE: readonly string[] = ["project.remove", "delivery.merge", "task.cancel", "task.revise"];

export const kindLabel = (kind: string): string => KIND_LABELS[kind] ?? kind;

/** The kinds the form shows as checkboxes: the daemon's grantable list, minus anything never grantable or unknown to this app. */
export function offeredKinds(policy: GrantPolicy): string[] {
  return policy.grantableKinds.filter((kind) => kind in KIND_LABELS && !NEVER_GRANTABLE.includes(kind));
}

/** What the form holds: text as typed, and the kinds ticked. */
export interface GrantDraft {
  scopeKind: "project" | "workstream";
  scopeId: string;
  kinds: string[];
  hours: string;
  uses: string;
  budget: string;
}

export interface GrantErrors {
  scope: string | null;
  kinds: string | null;
  hours: string | null;
  uses: string | null;
  budget: string | null;
}

export function initialDraft(policy: GrantPolicy): GrantDraft {
  return {
    scopeKind: "project",
    scopeId: "",
    kinds: [],
    hours: String(Math.round(policy.defaultExpiresInSecs / HOUR_SECS)),
    uses: String(policy.defaultMaxUses),
    budget: "",
  };
}

/** A whole number written in digits only; null for anything else, so "1e3" and "2.5" are not numbers here. */
export function parseWhole(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d{1,15}$/.test(trimmed)) return null;
  return Number(trimmed);
}

export const maxHours = (policy: GrantPolicy): number => Math.floor(policy.maxExpiresInSecs / HOUR_SECS);
const minHours = (policy: GrantPolicy): number => Math.max(1, Math.ceil(policy.minExpiresInSecs / HOUR_SECS));

export function validateDraft(draft: GrantDraft, policy: GrantPolicy): GrantErrors {
  const hours = parseWhole(draft.hours);
  const uses = parseWhole(draft.uses);
  const budget = parseWhole(draft.budget);
  const offered = offeredKinds(policy);
  return {
    scope: draft.scopeId === "" ? `Choose one ${draft.scopeKind}.` : null,
    kinds:
      draft.kinds.length === 0
        ? "Choose at least one kind of action."
        : draft.kinds.some((kind) => !offered.includes(kind))
          ? "Choose only kinds from the list."
          : null,
    hours:
      hours === null || hours < minHours(policy) || hours > maxHours(policy)
        ? `Enter a whole number of hours from ${minHours(policy)} to ${maxHours(policy)}.`
        : null,
    uses: uses === null || uses < 1 || uses > policy.maxMaxUses ? `Enter a whole number of uses from 1 to ${policy.maxMaxUses}.` : null,
    budget:
      budget === null || budget < 1 || budget > policy.maxTokenBudget
        ? `Enter a whole number of tokens from 1 to ${policy.maxTokenBudget.toLocaleString("en-US")}.`
        : null,
  };
}

export const hasErrors = (errors: GrantErrors): boolean => Object.values(errors).some((message) => message !== null);

/** What Create sends: exactly the ticked kinds, in the daemon's order. Call only for a draft with no errors. */
export function termsOf(draft: GrantDraft, policy: GrantPolicy): GrantTerms {
  return {
    scopeKind: draft.scopeKind,
    scopeId: draft.scopeId,
    kinds: offeredKinds(policy).filter((kind) => draft.kinds.includes(kind)),
    expiresInSecs: (parseWhole(draft.hours) ?? 0) * HOUR_SECS,
    maxUses: parseWhole(draft.uses) ?? 0,
    tokenBudget: parseWhole(draft.budget) ?? 0,
  };
}

export interface StatusChip {
  text: string;
  tone: Tone;
}

/** The chip for a grant. The daemon's status is read when the list is, so an active grant past its end shows as expired at once. */
export function statusChip(grant: GrantItem, nowMs: number): StatusChip {
  switch (grant.status) {
    case "active":
      return nowMs >= grant.expiresAtMs ? { text: "Expired", tone: "neutral" } : { text: "Active", tone: "ok" };
    case "expired":
      return { text: "Expired", tone: "neutral" };
    case "revoked":
      return { text: "Revoked", tone: "neutral" };
    case "exhausted":
      return { text: "Used up", tone: "warn" };
    case "budget_exhausted":
      return { text: "Out of budget", tone: "warn" };
    default:
      return { text: `Unknown status: ${grant.status}`, tone: "neutral" };
  }
}

export const isActive = (grant: GrantItem, nowMs: number): boolean => statusChip(grant, nowMs).text === "Active";

export const usesLeftText = (grant: GrantItem): string => {
  const left = Math.max(0, grant.maxUses - grant.usesUsed);
  return `${left} of ${grant.maxUses} ${grant.maxUses === 1 ? "use" : "uses"} left`;
};

export function timeLeftText(grant: GrantItem, nowMs: number): string {
  if (!isActive(grant, nowMs)) return "Ended";
  const minutes = Math.max(1, Math.ceil((grant.expiresAtMs - nowMs) / 60_000));
  if (minutes < 60) return `${minutes} ${minutes === 1 ? "minute" : "minutes"} left`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  const hoursText = `${hours} ${hours === 1 ? "hour" : "hours"}`;
  return rest === 0 ? `${hoursText} left` : `${hoursText} ${rest} ${rest === 1 ? "minute" : "minutes"} left`;
}

export const tokensText = (grant: GrantItem): string =>
  `${grant.tokensSpent.toLocaleString("en-US")} of ${grant.tokenBudget.toLocaleString("en-US")} tokens reported so far`;

/** A project's folder name or a workstream's title; with no label, only the kind. Never an id. */
export const scopeText = (grant: GrantItem): string =>
  `${grant.scopeKind === "workstream" ? "Workstream" : "Project"}${grant.scopeLabel === null ? "" : ` ${grant.scopeLabel}`}`;

/** A short id for the card and the list: the first ten characters, such as `grant-9f2c`. */
export const shortGrantId = (id: string): string => id.slice(0, 10);

/** Plain words for a failed create. The daemon is the authority, so its refusal is explained, not second-guessed. */
export function createFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "forbidden":
      return "Only the app that started the service can create standing approvals. Nothing was changed.";
    case "grant-invalid":
    case "invalid-params":
      return "The daemon refused these terms. Check the kinds and the limits. Nothing was changed.";
    case "grant-not-found":
      return "The project or workstream is gone. Nothing was changed.";
    case "timeout":
      return "The daemon did not answer. Nothing was changed.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}

export function revokeFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "forbidden":
      return "Only the app that started the service can revoke standing approvals. Nothing was changed.";
    case "grant-not-found":
      return "This standing approval no longer exists. Refresh the list.";
    case "timeout":
      return "The daemon did not answer. The standing approval may still be on. Check the list.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}

export function revokedText(stopped: number): string {
  return stopped === 0
    ? "Standing approval revoked."
    : `Standing approval revoked. ${stopped} pending ${stopped === 1 ? "action" : "actions"} stopped.`;
}

/** The sentence the polite region says once a grant is created. */
export function createdText(grant: GrantItem): string {
  const until = new Date(grant.expiresAtMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return `${CREATED_TEXT} It covers ${scopeText(grant)} until ${until}.`;
}
