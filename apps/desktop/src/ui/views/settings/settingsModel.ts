/** Pure parts of the Settings page: validation, wording and the patch Save sends (DPES-1105). */
import { presentError, type DaemonError } from "../../../daemon/errors";
import type { LoadMeterName } from "../../../daemon/types";
import type { LoadLimits, LoadReading, SettingsSnapshot } from "../../../sources/surfaces/types";
import type { Tone } from "../../theme";

/** The order the page lists the meters in; it is also the order the daemon names a holding meter in. */
export const LOAD_METERS: readonly LoadMeterName[] = ["cpu", "memory", "disk"];

export const MIN_LIMIT = 50;
export const MAX_LIMIT = 100;

const NAME: Record<LoadMeterName, string> = { cpu: "CPU", memory: "memory", disk: "disk" };

export const LIMIT_LABEL: Record<LoadMeterName, string> = {
  cpu: "CPU limit (percent)",
  memory: "Memory limit (percent)",
  disk: "Disk limit (percent)",
};

export const INTRO_TEXT =
  "New tasks wait while the machine is busy. Each limit is compared with the average of the last 60 seconds. Enter 100 to turn a check off. Running tasks are never stopped.";
export const LIMIT_ERROR = "Enter a whole number from 50 to 100.";
export const OFF_NOTE = "Off: tasks start whatever the load.";
export const DISK_NOTE = "Measured on the disk that holds the app's data";
export const FEED_OFF_TEXT = "Load checks are off for this daemon.";
export const SAVED_TEXT = "Saved. The new limits apply from the next measurement.";
export const FIXTURE_SAVE_TEXT = "Fixture data: nothing was saved.";
export const UNSUPPORTED_TITLE = "Settings are not available on this daemon.";
export const UNSUPPORTED_DETAIL = "It does not offer the settings capability, so there are no machine load limits to change here.";
export const REFRESH_FAILED_TEXT = "The last refresh failed. The load figures on this page may be out of date.";

export const INVALID_REASON = "Enter whole numbers from 50 to 100 before saving.";
export const NOT_CONNECTED_REASON = "The app is not connected to the orchestrator service, so nothing can be saved.";

/** The whole number in `text`, or null when it is empty, not a whole number, or outside 50 to 100. */
export function parseLimit(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= MIN_LIMIT && value <= MAX_LIMIT ? value : null;
}

export function limitError(text: string): string | null {
  return parseLimit(text) === null ? LIMIT_ERROR : null;
}

/** What Save sends: every meter, from its typed text or else the saved limit, so a save with no edit still writes. A meter whose text is invalid is left out; Save is blocked before then. */
export function limitPatch(saved: LoadLimits, typed: Partial<Record<LoadMeterName, string>>): Partial<LoadLimits> {
  const patch: Partial<LoadLimits> = {};
  for (const meter of LOAD_METERS) {
    const value = parseLimit(typed[meter] ?? String(saved[meter]));
    if (value !== null) patch[meter] = value;
  }
  return patch;
}

/** The status strip's line for one meter. */
export function readingText(meter: LoadMeterName, reading: LoadReading): string {
  switch (reading.state) {
    case "mean":
      return `${NAME[meter]} ${reading.meanPercent} percent`;
    case "unknown":
      return `Load unknown: ${NAME[meter]}`;
    case "off":
      return `${NAME[meter]} check off`;
  }
}

/** The hint under a field: what the gate reads now, or why it cannot. */
export function hintText(meter: LoadMeterName, reading: LoadReading): string {
  switch (reading.state) {
    case "mean":
      return `Now: ${reading.meanPercent} percent (60 second average)`;
    case "unknown":
      return `Load unknown: ${NAME[meter]} (${reading.reason === "warming_up" ? "still measuring" : "not available on this machine"})`;
    case "off":
      return "Check is off";
  }
}

/** The gate's state in words. Tone and glyph repeat it; the words alone carry the meaning. */
export function gateStatus(snapshot: SettingsSnapshot): { tone: Tone; text: string } {
  if (snapshot.feed === "disabled_by_environment") return { tone: "warn", text: FEED_OFF_TEXT };
  if (snapshot.gate.state === "open") return { tone: "ok", text: "Machine load: OK, new tasks can start" };
  const { hold } = snapshot.gate;
  const waiting = "Machine load: busy, new tasks are waiting";
  return {
    tone: "warn",
    text: hold ? `${waiting} (${NAME[hold.meter]} ${hold.meanPercent} percent, limit ${hold.limitPercent})` : waiting,
  };
}

/** Read aloud once when the gate changes state; a refresh that changes only a number says nothing. */
export function gateAnnouncement(closed: boolean): string {
  return closed ? "Machine load is busy. New tasks are waiting." : "Machine load is OK. New tasks can start.";
}

/** Plain words for a failed save; every other failure keeps the app's shared wording. */
export function saveFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "invalid-params":
      return "The daemon refused these values.";
    case "forbidden":
      return "Only the app can change these limits.";
    case "timeout":
      return "The daemon did not answer. Nothing was changed.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}
