/**
 * Orchestrator notes on the Settings page (DPES-1105 5b), as plain functions. No React: the section renders these and
 * the tests read them directly. The checks mirror the core's limits (`notes.rs`) so the user sees a message before
 * sending. The sensitive-looking check is the daemon's alone: its refusal is shown in plain words.
 */
import { presentError, type DaemonError } from "../../../daemon/errors";

export const SECTION_TITLE = "Notes for the orchestrator";
export const HELPER_TEXT =
  "These are your own reminders. The orchestrator reads them as data, not as instructions. Do not put passwords, tokens or file paths here.";
export const LOCKED_NOTE = "Only the app that started the service can change notes for the orchestrator.";
export const FIXTURE_TEXT = "Fixture data: nothing was saved.";
export const NOT_CONNECTED_REASON = "Reconnect to the daemon to change notes.";
export const SENSITIVE_TEXT = "The daemon refused this note because it looks like a password, a token or a file path. Nothing was changed.";

/** The scope of the notes every workstream shares. A workstream's own scope is `workstream:` plus its id. */
export const GLOBAL_SCOPE = "global";
export const workstreamScope = (workstreamId: string): string => `workstream:${workstreamId}`;

/** What a note is checked against: the daemon's limits for one scope. */
export interface NoteLimits {
  maxNotes: number;
  maxBytes: number;
}

const encoder = new TextEncoder();

/** The core counts bytes, not characters: an accented letter is two bytes and an emoji is four. */
export const noteBytes = (text: string): number => encoder.encode(text).length;

const tooMany = (limits: NoteLimits) =>
  `You can keep at most ${limits.maxNotes} notes in one place. Delete one to add another.`;

/** A plain-words refusal for one note, or null when it passes. Refuses rather than trims, like the core. */
export function checkNote(text: string, limits: NoteLimits): string | null {
  if (text.trim() === "") return "Write a note first. A blank line is not saved.";
  if (noteBytes(text) > limits.maxBytes) {
    return `This note is ${noteBytes(text)} bytes. The most is ${limits.maxBytes} bytes, and an accented letter or an emoji takes more than one.`;
  }
  if (/\p{Cc}/u.test(text)) return "A note is one line. Remove the line break, tab or other control character.";
  return null;
}

/** Why the whole list of a scope may not be sent, or null: its length first, then each note in order. */
export function checkList(notes: readonly string[], limits: NoteLimits): string | null {
  if (notes.length > limits.maxNotes) return tooMany(limits);
  for (const note of notes) {
    const refusal = checkNote(note, limits);
    if (refusal !== null) return refusal;
  }
  return null;
}

/** Why a note cannot be added to a list of this length, or null. */
export const addRefusal = (count: number, limits: NoteLimits): string | null => (count >= limits.maxNotes ? tooMany(limits) : null);

export const bytesText = (text: string, limits: NoteLimits): string => `${noteBytes(text)} of ${limits.maxBytes} bytes`;

export const savedText = (action: "added" | "changed" | "deleted"): string => `Note ${action}.`;

/** The polite sentence after a refused or failed save. `invalid-params` is what the daemon answers for a refused note. */
export function saveFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "forbidden":
      return "Only the app that started the service can change notes. Nothing was changed.";
    case "invalid-params":
      return SENSITIVE_TEXT;
    case "not-found":
      return "This workstream is gone. Nothing was changed.";
    case "timeout":
      return "The daemon did not answer. The note may or may not be saved. Check the list.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}
