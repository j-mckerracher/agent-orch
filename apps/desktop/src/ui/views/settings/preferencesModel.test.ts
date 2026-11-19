import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import type { MemorySnapshot } from "../../../sources/surfaces/types";
import {
  GLOBAL_SCOPE,
  REFUSED_TEXT,
  changeFailureText,
  checkKey,
  checkText,
  entryBytes,
  overBudget,
  overBudgetText,
  projectScope,
  projectedBytes,
  workstreamScope,
} from "./preferencesModel";

const LIMITS = { maxKeyBytes: 40, maxTextBytes: 280 };

const snapshot = (bytes: number, budget: number): MemorySnapshot => ({
  kind: "preference",
  scope: "global",
  entries: [{ key: "small-prs", text: "Prefer small pull requests.", writtenBy: "user", createdAtMs: 0, confirmedAtMs: 0, stale: false }],
  bytes,
  budget,
  archived: [],
  ...LIMITS,
  staleAfterDays: null,
});

describe("preferences model", () => {
  it("keys_are_lowercase_letters_digits_and_hyphens_up_to_the_limit", () => {
    expect(checkKey("small-prs", LIMITS)).toBeNull();
    expect(checkKey("1st", LIMITS)).toBeNull();
    expect(checkKey("", LIMITS)).toMatch(/Write a key first/);
    expect(checkKey("-lead", LIMITS)).toMatch(/starts with a letter or a digit/);
    expect(checkKey("Upper", LIMITS)).toMatch(/lowercase letters, digits and hyphens/);
    expect(checkKey("with space", LIMITS)).toMatch(/lowercase/);
    expect(checkKey("a".repeat(40), LIMITS)).toBeNull();
    expect(checkKey("a".repeat(41), LIMITS)).toMatch(/41 bytes\. The most is 40 bytes/);
  });

  it("text_counts_bytes_refuses_blank_and_control_characters_and_never_trims", () => {
    expect(checkText("a".repeat(280), LIMITS)).toBeNull();
    expect(checkText("é".repeat(141), LIMITS)).toMatch(/282 bytes/);
    expect(checkText("   ", LIMITS)).toMatch(/Write the text first/);
    expect(checkText("one\ntwo", LIMITS)).toMatch(/one line/);
    expect(checkText("one\ttwo", LIMITS)).toMatch(/one line/);
    expect(checkText("  keeps its spaces  ", LIMITS)).toBeNull();
    // Looks like a token to the daemon; the app does not guess.
    expect(checkText("ghp_" + "a".repeat(30), LIMITS)).toBeNull();
  });

  it("an_entry_costs_its_key_and_text_bytes_and_a_same_key_write_replaces_the_old_entry", () => {
    expect(entryBytes("ab", "é")).toBe(4);
    const doc = snapshot(entryBytes("small-prs", "Prefer small pull requests."), 2048);
    expect(projectedBytes(doc, "new-key", "abc")).toBe(doc.bytes + 10);
    expect(projectedBytes(doc, "small-prs", "Short.")).toBe(entryBytes("small-prs", "Short."));
  });

  it("over_budget_mirrors_the_core_and_allows_a_write_that_does_not_grow_the_document", () => {
    expect(overBudget(snapshot(100, 120), 121)).toBe(true);
    expect(overBudget(snapshot(100, 120), 120)).toBe(false);
    // Already over the budget: shrinking or keeping the size is allowed.
    expect(overBudget(snapshot(200, 120), 200)).toBe(false);
    expect(overBudget(snapshot(200, 120), 201)).toBe(true);
    expect(overBudgetText(2100, 2048)).toBe("Over the budget: 2100 of 2048 bytes. Shorten or remove an entry.");
  });

  it("scopes_are_global_workstream_or_project_plus_the_raw_id", () => {
    expect(GLOBAL_SCOPE).toBe("global");
    expect(workstreamScope("ws-1")).toBe("workstream:ws-1");
    expect(projectScope("/Users/me/repo")).toBe("project:/Users/me/repo");
  });

  it("failure_text_is_plain_words_for_each_failure", () => {
    expect(changeFailureText(new DaemonError("invalid-params", "invalid params", -32602))).toBe(REFUSED_TEXT);
    expect(REFUSED_TEXT).toMatch(/password, a token or a file path/);
    expect(changeFailureText(new DaemonError("forbidden", "forbidden", -32012))).toMatch(/Only the app that started the service/);
    expect(changeFailureText(new DaemonError("not-found", "gone", -32010))).toMatch(/project or workstream is gone/);
    expect(changeFailureText(new DaemonError("conflict", "in use", -32011))).toMatch(/key is in use again, or it was already restored/);
    expect(changeFailureText(new DaemonError("timeout", "timed out"))).toMatch(/may or may not be saved/);
  });
});
