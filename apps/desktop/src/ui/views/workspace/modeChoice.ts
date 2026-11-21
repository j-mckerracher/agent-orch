/**
 * The chat mode each agent was last set to (DPES-1105): one option per group, saved per agent. Pure, so the rules are
 * tested without rendering.
 */
import type { Measured, ModeGroup, ModeOption, ModeSelection } from "../../../sources/surfaces/types";
import { browserStorage, type PreferenceStorage } from "./permissionLevel";

const KEY_PREFIX = "agent-orch.chat.mode.";

/** Why the mode control cannot be picked: the trigger shows the label, and the panel the reason. */
export interface ModeDisabled {
  label: string;
  reason: string;
}

/**
 * What the mode control offers for a chat's agent, and why nothing can be picked. A routed message names no agent, so
 * the daemon routes it and runs each group at its default. An older daemon has no modes at all.
 */
export function modeOffer(
  agent: string | undefined,
  daemonAdvertisesModes: boolean,
  modes: Measured<ModeGroup[]> | undefined,
): { groups: ModeGroup[]; disabled: ModeDisabled | null } {
  if (agent === undefined) return { groups: [], disabled: { label: "Default mode", reason: "Choose an agent to pick its mode." } };
  if (!daemonAdvertisesModes) {
    return { groups: [], disabled: { label: "Needs an updated daemon", reason: "This daemon predates chat modes. Update the daemon to choose a mode." } };
  }
  if (modes === undefined) return { groups: [], disabled: { label: "Modes not confirmed", reason: "The daemon's catalog does not list this agent yet." } };
  if (modes.status !== "ok") {
    return { groups: [], disabled: { label: modes.status === "unknown" ? "Modes not confirmed" : "Default mode", reason: modes.reason } };
  }
  if (modes.value.length === 0) return { groups: [], disabled: { label: "Default mode", reason: "This agent offers no modes." } };
  return { groups: modes.value, disabled: null };
}

/**
 * Keeps only the options the catalog still offers as available and that need no confirmation. An option that needs
 * confirmation (Full access) is never saved, so a reload starts from the default (design O4).
 */
function keepable(groups: ModeGroup[], selection: unknown): ModeSelection {
  const kept: ModeSelection = {};
  if (typeof selection !== "object" || selection === null) return kept;
  for (const [groupId, optionId] of Object.entries(selection)) {
    const option = groups.find((group) => group.id === groupId)?.options.find((item) => item.id === optionId);
    if (option?.available && !option.needsConfirmation) kept[groupId] = option.id;
  }
  return kept;
}

/** The saved selection for an agent. Missing or unreadable storage means none, so each group takes its default. */
export function readSavedMode(agent: string, groups: ModeGroup[], storage: PreferenceStorage | null = browserStorage()): ModeSelection {
  try {
    const raw = storage?.getItem(KEY_PREFIX + agent);
    return raw ? keepable(groups, JSON.parse(raw)) : {};
  } catch {
    return {};
  }
}

/** Saves the selection for an agent. A failed write means it is not kept after a reload; the choice still applies here. */
export function saveMode(agent: string, selection: ModeSelection, groups: ModeGroup[], storage: PreferenceStorage | null = browserStorage()): void {
  try {
    storage?.setItem(KEY_PREFIX + agent, JSON.stringify(keepable(groups, selection)));
  } catch {
    // Not kept after a reload; the choice still applies to this session.
  }
}

/**
 * The option each group runs with: the chosen one while the daemon still offers it, else the group's default. A group
 * with no default is left out, so the daemon's own default applies to it.
 */
export function effectiveMode(groups: ModeGroup[], selection: ModeSelection): ModeSelection {
  const effective: ModeSelection = {};
  for (const group of groups) {
    const chosen = group.options.find((option) => option.id === selection[group.id] && option.available);
    const option = chosen ?? group.options.find((item) => item.isDefault);
    if (option) effective[group.id] = option.id;
  }
  return effective;
}

/** The options a selection names, in group order. */
export function selectedOptions(groups: ModeGroup[], selection: ModeSelection): ModeOption[] {
  return groups.flatMap((group) => group.options.filter((option) => option.id === selection[group.id]));
}
