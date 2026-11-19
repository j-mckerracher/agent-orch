/**
 * Preferences and learnings on the Settings page (DPES-1108 phase 6), as plain functions. No React: the section renders
 * these and the tests read them directly. The checks mirror the core's (`memory.rs`) so the user sees a message before
 * sending. The sensitive-looking check is the daemon's alone: its refusal is shown in plain words.
 */
import { presentError, type DaemonError } from "../../../daemon/errors";
import type { MemoryAuthor, MemoryKind, MemorySnapshot } from "../../../sources/surfaces/types";

export const SECTION_TITLE = "Preferences and learnings";
export const FIXTURE_TEXT = "Fixture data: nothing was saved.";
export const NOT_CONNECTED_REASON = "Reconnect to the daemon to change preferences and learnings.";
export const REFUSED_TEXT =
  "The daemon refused this entry. A key is lowercase letters, digits and hyphens. The text is one line and must not look like a password, a token or a file path. Nothing was changed.";

/** One of the three lists on the page. */
export interface ListCopy {
  kind: MemoryKind;
  title: string;
  caption: string;
  /** The singular noun in button and field names, so the three lists never share a name. */
  noun: string;
}

export const SHARED_LIST: ListCopy = {
  kind: "preference",
  title: "Shared preferences",
  caption:
    "Your standing defaults. Every conversation follows them unless a message says otherwise. Project coordinators and workstream conversations can read them but cannot change them.",
  noun: "shared preference",
};

export const LOCAL_LIST: ListCopy = {
  kind: "preference",
  title: "Project and workstream preferences",
  caption: "Preferences for one project or one workstream. Its conversation follows them together with the shared preferences.",
  noun: "project or workstream preference",
};

export const LEARNINGS_LIST: ListCopy = {
  kind: "learning",
  title: "Learnings",
  caption: "Operational facts the orchestrator recorded.",
  noun: "learning",
};

export const staleCaption = (days: number): string =>
  `An entry nobody confirmed for ${days} days is marked stale. Keep it if it is still true, or remove it.`;

/** The document every conversation shares. A workstream's is `workstream:` plus its id; a project's `project:` plus its id. */
export const GLOBAL_SCOPE = "global";
export const workstreamScope = (workstreamId: string): string => `workstream:${workstreamId}`;
/** The raw project id from `projects.list` (the repository root), as the daemon keys its stores. */
export const projectScope = (projectId: string): string => `project:${projectId}`;

const encoder = new TextEncoder();

/** The core counts bytes, not characters: an accented letter is two bytes and an emoji is four. */
export const textBytes = (text: string): number => encoder.encode(text).length;

/** What one entry takes from its document's budget: its key bytes plus its text bytes. */
export const entryBytes = (key: string, text: string): number => textBytes(key) + textBytes(text);

type Limits = Pick<MemorySnapshot, "maxKeyBytes" | "maxTextBytes">;

/** A plain-words refusal for a key, or null when it passes. */
export function checkKey(key: string, limits: Limits): string | null {
  if (key === "") return "Write a key first.";
  if (!/^[a-z0-9][a-z0-9-]*$/.test(key)) {
    return "A key uses lowercase letters, digits and hyphens only, and starts with a letter or a digit.";
  }
  if (textBytes(key) > limits.maxKeyBytes) return `This key is ${textBytes(key)} bytes. The most is ${limits.maxKeyBytes} bytes.`;
  return null;
}

/** A plain-words refusal for an entry's text, or null when it passes. Refuses rather than trims, like the core. */
export function checkText(text: string, limits: Limits): string | null {
  if (text.trim() === "") return "Write the text first. A blank line is not saved.";
  if (textBytes(text) > limits.maxTextBytes) {
    return `This text is ${textBytes(text)} bytes. The most is ${limits.maxTextBytes} bytes, and an accented letter or an emoji takes more than one.`;
  }
  if (/\p{Cc}/u.test(text)) return "An entry is one line. Remove the line break, tab or other control character.";
  return null;
}

/**
 * The document's size after writing `text` under `key`: an existing key's old entry is replaced.
 * Mirrors the core's `check_budget`: a write that does not grow the document is always allowed.
 */
export function projectedBytes(snapshot: MemorySnapshot, key: string, text: string): number {
  const old = snapshot.entries.find((entry) => entry.key === key);
  return snapshot.bytes - (old === undefined ? 0 : entryBytes(old.key, old.text)) + entryBytes(key, text);
}

export const overBudget = (snapshot: MemorySnapshot, after: number): boolean => after > snapshot.budget && after > snapshot.bytes;

export const bytesText = (bytes: number, budget: number): string => `${bytes} of ${budget} bytes`;

export const overBudgetText = (bytes: number, budget: number): string =>
  `Over the budget: ${bytes} of ${budget} bytes. Shorten or remove an entry.`;

export const authorText = (author: MemoryAuthor): string => (author === "user" ? "the user" : "the orchestrator");

const dayFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
export const dayText = (ms: number): string => dayFormat.format(new Date(ms));

/** The sentence after a `memory.set` the daemon answered. Null for `over_budget`, which is a refusal. */
export function setOutcomeText(outcome: "created" | "updated" | "confirmed", key: string, keeping: boolean): string {
  switch (outcome) {
    case "created":
      return `Added ${key}.`;
    case "updated":
      return `Changed ${key}. The old text is under Removed entries.`;
    case "confirmed":
      return keeping ? `Kept ${key}. It is confirmed as of today.` : `${key} already says this. It is confirmed as of today.`;
  }
}

/** The polite sentence after a refused or failed change. `invalid-params` is what the daemon answers for a refused entry. */
export function changeFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "forbidden":
      return "Only the app that started the service can change preferences and learnings. Nothing was changed.";
    case "invalid-params":
      return REFUSED_TEXT;
    case "not-found":
      return "This project or workstream is gone. Nothing was changed.";
    case "conflict":
      return "This entry cannot be restored: its key is in use again, or it was already restored. Nothing was changed.";
    case "timeout":
      return "The daemon did not answer. The change may or may not be saved. Check the list.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}
