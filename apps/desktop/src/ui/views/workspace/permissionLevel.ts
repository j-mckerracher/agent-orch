/**
 * The permission level for chats (DPES-1105) and tasks (DPES-1126): its labels, the level a request carries,
 * why a level cannot be chosen, the Full confirmation's wording, and the saved preferences. Pure, so the rules
 * are tested without rendering.
 */
import type { TaskRecord } from "../../../daemon/types";
import type { PermissionLevel } from "../../../sources/surfaces/types";

export const PERMISSION_LEVELS: readonly PermissionLevel[] = ["restricted", "extended", "auto", "full"];

/** Auto is the default for chats (design D1). */
export const DEFAULT_PERMISSION_LEVEL: PermissionLevel = "auto";

/** The one value that survives a reload. Only Restricted is saved; Auto, Extended and Full reset to Auto (design O4). */
export const RESTRICTED_PREFERENCE_KEY = "agent-orch.chat.permission-level";

/** The task form's own saved Restricted choice, so a chat choice never reaches a task (design O4). */
export const TASK_PREFERENCE_KEY = "agent-orch.task.permission-level";

const LABELS: Record<PermissionLevel, string> = {
  restricted: "Restricted",
  extended: "Extended",
  auto: "Auto",
  full: "Full",
};

export const permissionLabel = (level: PermissionLevel): string => LABELS[level];

/**
 * The level a request carries. Absent when the daemon does not advertise `permission_levels`: the request
 * then goes exactly as an older desktop sent it, and the daemon runs it as Restricted.
 */
export function permissionForSend(level: PermissionLevel, daemonAdvertises: boolean): PermissionLevel | undefined {
  return daemonAdvertises ? level : undefined;
}

/** Where a level is picked. Each surface keeps its own saved Restricted choice. */
export type PermissionSurface = "chat" | "task";

const EXTENDED_BLOCK: Record<PermissionSurface, string> = {
  chat: "not available for chat yet",
  task: "read-only git only, so it can't build, test or commit",
};

/** Why a level cannot be chosen on a surface, or null. Extended is refused on every agent for now (PR A). */
export function choiceBlock(level: PermissionLevel, daemonAdvertises: boolean, surface: PermissionSurface): string | null {
  if (level === "extended") return EXTENDED_BLOCK[surface];
  if (!daemonAdvertises && level !== "restricted") return "needs an updated daemon";
  return null;
}

export const OLDER_DAEMON_NOTICE = "This daemon predates permission levels. Update the daemon, or choose Restricted.";

export const FULL_SUMMARY =
  "Full lets the agent run any command and edit any file your account can. MCAO adds no sandbox and no approval checks.";

/** Full has no review step at all, so the review's limits are stated beside it. */
export const FULL_REVIEW_TEXT =
  "Full has no automatic review step. Automatic review at Auto is not a guarantee either.";

export const FULL_SAVE_TEXT = "Full lasts until you choose another level. It is not kept after a restart.";

export type AgentId = "codex" | "claude-code" | "github-copilot";
const AGENT_IDS: readonly AgentId[] = ["codex", "claude-code", "github-copilot"];

export const FULL_AGENT_TEXT: Record<AgentId, string> = {
  codex:
    "Codex runs with no sandbox. MCAO passes --sandbox danger-full-access, and your own ~/.codex settings still decide the approvals.",
  "claude-code":
    "Claude Code skips its permission checks and uses your own Claude settings and hooks. It can run any tool your account allows.",
  "github-copilot":
    "Copilot can run any shell command, edit any file your account can, reach any URL, and use your own Copilot settings and hooks. No OS sandbox is applied. Copilot cannot ask you anything during the run.",
};

/**
 * The agent texts that apply to a message. A message with a named agent gets that agent's text. Without one,
 * the daemon routes it, so every agent that could answer is named.
 */
export function agentTexts(texts: Record<AgentId, string>, agent: string | undefined): { agent: AgentId; text: string }[] {
  const named = AGENT_IDS.filter((id) => id === agent);
  return (named.length > 0 ? named : AGENT_IDS).map((id) => ({ agent: id, text: texts[id] }));
}

export type PreferenceStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/** The app's browser storage, or null when the browser blocks it. Shared by the saved chat preferences. */
export function browserStorage(): PreferenceStorage | null {
  try {
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

/** The level to start with. Only a saved Restricted counts; missing or unreadable storage means the default. */
export function readSavedLevel(storage: PreferenceStorage | null = browserStorage(), key = RESTRICTED_PREFERENCE_KEY): PermissionLevel {
  try {
    return storage?.getItem(key) === "restricted" ? "restricted" : DEFAULT_PERMISSION_LEVEL;
  } catch {
    return DEFAULT_PERMISSION_LEVEL;
  }
}

/**
 * Saves Restricted and removes the saved value for any other level, so the only value ever written is the
 * Restricted preference. A failed write means the choice is simply not kept after a reload.
 */
export function saveLevel(level: PermissionLevel, storage: PreferenceStorage | null = browserStorage(), key = RESTRICTED_PREFERENCE_KEY): void {
  try {
    if (level === "restricted") storage?.setItem(key, "restricted");
    else storage?.removeItem(key);
  } catch {
    // Not kept after a reload; the choice still applies to this session.
  }
}

/**
 * The level a task's agent ran with: its explicit `permission_level`, else the contract's. The contract omits
 * Restricted, so a missing level there means Restricted, which is also what an older task ran at.
 */
export function taskPermissionLevel(task: TaskRecord): PermissionLevel {
  return PERMISSION_LEVELS.find((level) => level === task.permission_level) ?? task.contract.permission ?? "restricted";
}
