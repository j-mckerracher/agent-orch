/**
 * The chat model choice the user last made (DPES-1105): the provider, model and effort each message is sent with
 * until the user changes it. Saved in the same browser storage as the permission level. Pure, so the rules are
 * tested without rendering.
 */
import type { ModelChoice } from "../../../sources/surfaces/types";
import { browserStorage, type PreferenceStorage } from "./permissionLevel";

export const CHOICE_PREFERENCE_KEY = "agent-orch.chat.model-choice";

const isText = (value: unknown): value is string => typeof value === "string" && value !== "";

/**
 * The saved choice, or null when none is saved. A saved value that is not a whole choice counts as none, so a part is
 * never dropped and sent as a different choice. Unreadable storage means none.
 */
export function readSavedChoice(storage: PreferenceStorage | null = browserStorage()): ModelChoice | null {
  try {
    const raw = storage?.getItem(CHOICE_PREFERENCE_KEY);
    if (!raw) return null;
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null) return null;
    const { agent, model, effort } = value as Record<string, unknown>;
    if (!isText(agent)) return null;
    if (model !== undefined && !isText(model)) return null;
    if (effort !== undefined && !isText(effort)) return null;
    return { agent, ...(model === undefined ? {} : { model }), ...(effort === undefined ? {} : { effort }) };
  } catch {
    return null;
  }
}

/** Saves the choice. A failed write means it is not kept after a reload; the choice still applies to this session. */
export function saveChoice(choice: ModelChoice, storage: PreferenceStorage | null = browserStorage()): void {
  try {
    storage?.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify(choice));
  } catch {
    // Not kept after a reload; the choice still applies to this session.
  }
}
