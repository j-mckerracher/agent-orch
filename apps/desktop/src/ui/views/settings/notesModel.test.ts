import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import {
  GLOBAL_SCOPE,
  SENSITIVE_TEXT,
  addRefusal,
  checkList,
  checkNote,
  noteBytes,
  saveFailureText,
  workstreamScope,
} from "./notesModel";

const LIMITS = { maxNotes: 10, maxBytes: 200 };

describe("notes model", () => {
  it("notes_validation_counts_bytes_not_characters", () => {
    expect(noteBytes("é")).toBe(2);
    expect(noteBytes("😀")).toBe(4);
    expect(checkNote("a".repeat(200), LIMITS)).toBeNull();
    expect(checkNote("a".repeat(201), LIMITS)).toMatch(/201 bytes\. The most is 200 bytes/);
    // 101 characters, 202 bytes: over the limit although the character count is under it.
    expect(checkNote("é".repeat(100), LIMITS)).toBeNull();
    expect(checkNote("é".repeat(101), LIMITS)).toMatch(/202 bytes/);
    expect(checkNote("😀".repeat(51), LIMITS)).toMatch(/204 bytes/);
  });

  it("notes_validation_refuses_a_blank_line_and_a_control_character_and_never_trims", () => {
    expect(checkNote("", LIMITS)).toMatch(/Write a note first/);
    expect(checkNote("   ", LIMITS)).toMatch(/Write a note first/);
    expect(checkNote("one\ntwo", LIMITS)).toMatch(/one line/);
    expect(checkNote("one\ttwo", LIMITS)).toMatch(/one line/);
    expect(checkNote("  keeps its spaces  ", LIMITS)).toBeNull();
  });

  it("notes_validation_limits_the_list_to_the_daemons_count_and_leaves_the_sensitive_check_to_the_daemon", () => {
    const ten = Array.from({ length: 10 }, (_, index) => `note ${index}`);
    expect(checkList(ten, LIMITS)).toBeNull();
    expect(checkList([...ten, "one more"], LIMITS)).toMatch(/at most 10 notes/);
    expect(checkList(["fine", ""], LIMITS)).toMatch(/Write a note first/);
    expect(addRefusal(9, LIMITS)).toBeNull();
    expect(addRefusal(10, LIMITS)).toMatch(/Delete one to add another/);
    // Looks like a token to the daemon; the app does not guess.
    expect(checkNote("ghp_" + "a".repeat(30), LIMITS)).toBeNull();
    // The limits are the daemon's: a smaller list limit from a newer daemon is respected.
    expect(addRefusal(3, { maxNotes: 3, maxBytes: 200 })).toMatch(/at most 3 notes/);
  });

  it("notes_scopes_are_global_or_workstream_plus_id", () => {
    expect(GLOBAL_SCOPE).toBe("global");
    expect(workstreamScope("ws-1")).toBe("workstream:ws-1");
  });

  it("notes_refusal_text_is_plain_words_for_each_failure", () => {
    expect(saveFailureText(new DaemonError("invalid-params", "invalid params", -32602))).toBe(SENSITIVE_TEXT);
    expect(SENSITIVE_TEXT).toMatch(/password, a token or a file path/);
    expect(saveFailureText(new DaemonError("forbidden", "forbidden", -32012))).toMatch(/Only the app that started the service/);
    expect(saveFailureText(new DaemonError("not-found", "gone", -32010))).toMatch(/workstream is gone/);
    expect(saveFailureText(new DaemonError("timeout", "timed out"))).toMatch(/may or may not be saved/);
  });
});
