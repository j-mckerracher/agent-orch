import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import type { SettingsSnapshot } from "../../../sources/surfaces/types";
import {
  LIMIT_ERROR,
  LOAD_METERS,
  gateAnnouncement,
  gateStatus,
  hintText,
  limitError,
  limitPatch,
  parseLimit,
  readingText,
  saveFailureText,
} from "./settingsModel";

const SAVED = { cpu: 80, memory: 85, disk: 90 };

const snapshot = (gate: SettingsSnapshot["gate"], feed: SettingsSnapshot["feed"] = "sampler"): SettingsSnapshot => ({ limits: SAVED, gate, feed });
const readings = {
  cpu: { state: "mean", meanPercent: 91 },
  memory: { state: "mean", meanPercent: 62 },
  disk: { state: "unknown", reason: "warming_up" },
} as const;

describe("settingsModel", () => {
  it.each([
    ["an empty field", ""],
    ["a blank field", "   "],
    ["a number under 50", "49"],
    ["a number over 100", "101"],
    ["a number with a fraction", "85.5"],
    ["letters", "abc"],
    ["a negative number", "-60"],
    ["an exponent", "1e2"],
  ])("settings limit text that is %s gets the one plain message", (_label, text) => {
    expect(limitError(text)).toBe("Enter a whole number from 50 to 100.");
    expect(limitError(text)).toBe(LIMIT_ERROR);
    expect(parseLimit(text)).toBeNull();
  });

  it("limit_text_from_50_to_100_is_a_valid_whole_number", () => {
    for (const text of ["50", "85", "100", " 70 "]) expect(limitError(text), text).toBeNull();
    expect([parseLimit("50"), parseLimit("100"), parseLimit(" 70 ")]).toEqual([50, 100, 70]);
  });

  it("a_limit_patch_holds_only_the_valid_fields_that_differ_from_the_saved_limits_in_a_fixed_order", () => {
    expect(limitPatch(SAVED, {})).toEqual({});
    expect(limitPatch(SAVED, { cpu: "80", memory: "85", disk: "90" })).toEqual({});
    expect(limitPatch(SAVED, { cpu: "85", memory: "85", disk: "abc" })).toEqual({ cpu: 85 });
    expect(limitPatch(SAVED, { disk: "100" })).toEqual({ disk: 100 });
    expect(Object.keys(limitPatch(SAVED, { disk: "60", cpu: "70" }))).toEqual(["cpu", "disk"]);
    expect(LOAD_METERS).toEqual(["cpu", "memory", "disk"]);
  });

  it("the_status_strip_and_the_field_hint_word_each_meter_state", () => {
    expect(readingText("cpu", { state: "mean", meanPercent: 42 })).toBe("CPU 42 percent");
    expect(readingText("memory", { state: "mean", meanPercent: 61 })).toBe("memory 61 percent");
    expect(readingText("cpu", { state: "unknown", reason: "unavailable" })).toBe("Load unknown: CPU");
    expect(readingText("disk", { state: "off" })).toBe("disk check off");
    expect(hintText("cpu", { state: "mean", meanPercent: 42 })).toBe("Now: 42 percent (60 second average)");
    expect(hintText("cpu", { state: "unknown", reason: "warming_up" })).toBe("Load unknown: CPU (still measuring)");
    expect(hintText("disk", { state: "unknown", reason: "unavailable" })).toBe("Load unknown: disk (not available on this machine)");
    expect(hintText("memory", { state: "off" })).toBe("Check is off");
  });

  it("the_gate_status_names_the_holding_meter_and_says_load_checks_are_off_for_a_test_daemon", () => {
    expect(gateStatus(snapshot({ state: "open", hold: null, readings }))).toEqual({ tone: "ok", text: "Machine load: OK, new tasks can start" });
    expect(
      gateStatus(snapshot({ state: "closed", hold: { meter: "cpu", meanPercent: 91, limitPercent: 80, reopenPercent: 70 }, readings })),
    ).toEqual({ tone: "warn", text: "Machine load: busy, new tasks are waiting (CPU 91 percent, limit 80)" });
    expect(gateStatus(snapshot({ state: "closed", hold: null, readings })).text).toBe("Machine load: busy, new tasks are waiting");
    expect(gateStatus(snapshot({ state: "open", hold: null, readings }, "disabled_by_environment")).text).toBe(
      "Load checks are off for this daemon.",
    );
  });

  it("the_gate_announcement_differs_for_a_busy_and_an_ok_machine", () => {
    expect(gateAnnouncement(true)).toBe("Machine load is busy. New tasks are waiting.");
    expect(gateAnnouncement(false)).toBe("Machine load is OK. New tasks can start.");
  });

  it("a_failed_save_gets_plain_words_by_error_kind_and_the_shared_wording_otherwise", () => {
    expect(saveFailureText(new DaemonError("invalid-params", "x", -32602))).toBe("The daemon refused these values.");
    expect(saveFailureText(new DaemonError("forbidden", "x", -32012))).toBe("Only the app can change these limits.");
    expect(saveFailureText(new DaemonError("timeout", "x"))).toBe("The daemon did not answer. Nothing was changed.");
    expect(saveFailureText(new DaemonError("disconnected", "x"))).toBe(
      "The connection to the daemon was lost. Actions are paused until the client reconnects.",
    );
  });
});
