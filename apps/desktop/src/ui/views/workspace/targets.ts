/** Conversation targets of the Orchestrator Workspace and the keys that store their drafts. */
import type { ConversationTarget } from "../../../sources/surfaces/types";
import { basename } from "../../format";

export const GLOBAL_TARGET: ConversationTarget = { kind: "global" };

export interface TargetOption {
  key: string;
  target: ConversationTarget;
  /** Name shown in the picker and used in "Talking to: …". */
  label: string;
  /** One line under the header; never an invented figure. */
  scope: string;
  /** True for fixture-only options (Global in fixture mode): they carry the D10 PLACEHOLDER badge. */
  placeholder: boolean;
}

/** Stable key for a target; also the draft key. The `project:` and `workstream:` prefixes keep them apart from `global`. */
export function targetKey(target: ConversationTarget): string {
  if (target.kind === "global") return "global";
  return target.kind === "project" ? `project:${target.projectId}` : `workstream:${target.workstreamId}`;
}

export function parseTargetKey(key: string | null): ConversationTarget | null {
  if (key === "global") return GLOBAL_TARGET;
  if (key?.startsWith("project:") && key.length > "project:".length) {
    return { kind: "project", projectId: key.slice("project:".length) };
  }
  if (key?.startsWith("workstream:") && key.length > "workstream:".length) {
    return { kind: "workstream", workstreamId: key.slice("workstream:".length) };
  }
  return null;
}

export function projectLabel(projectId: string): string {
  return `${basename(projectId)} coordinator`;
}

/** The id that names the target in the daemon; Global has none. */
export function targetId(target: ConversationTarget): string | null {
  if (target.kind === "global") return null;
  return target.kind === "project" ? target.projectId : target.workstreamId;
}

/** Case-insensitive substring match over the option's label and scope. */
export function filterTargets(options: readonly TargetOption[], query: string): TargetOption[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...options];
  return options.filter((option) => `${option.label} ${option.scope}`.toLowerCase().includes(needle));
}
