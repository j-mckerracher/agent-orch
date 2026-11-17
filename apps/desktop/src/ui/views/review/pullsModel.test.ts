import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import {
  ageText,
  checkLine,
  checkResultText,
  checksSummary,
  mergeStateText,
  mergeVerdict,
  plainRemote,
  prStateText,
  pullsErrorText,
  refusalWords,
  shortSha,
} from "./pullsModel";

const FACTS = {
  mergeMethod: "squash",
  requiredTotal: 3,
  requiredPassed: 3,
  requiredFailed: 0,
  checks: [],
  observedAgeSecs: 40,
  refusal: null,
};

describe("pull request words", () => {
  it("landing_model_states_the_pull_request_state_in_words", () => {
    expect(prStateText("open")).toBe("Open");
    expect(prStateText("merged")).toBe("Merged");
    expect(prStateText("closed")).toBe("Closed, not merged");
    expect(prStateText("draft")).toBe("State: draft");
  });

  it("landing_model_check_results_are_words_and_an_unknown_one_says_so", () => {
    expect(checkResultText("pass")).toBe("Passed");
    expect(checkResultText("fail")).toBe("Failed");
    expect(checkResultText("pending")).toBe("Not finished");
    expect(checkResultText("skipping")).toBe("Result not known");
    expect(checkLine({ name: "build", result: "fail" })).toBe("build: Failed");
  });

  it("landing_model_check_summary_counts_passed_failed_and_unfinished", () => {
    expect(checksSummary(FACTS)).toBe("Required checks 3 of 3 passed");
    expect(checksSummary({ requiredTotal: 3, requiredPassed: 1, requiredFailed: 1 })).toBe(
      "Required checks 1 of 3 passed, 1 failed, 1 not finished",
    );
    expect(checksSummary({ requiredTotal: 0, requiredPassed: 0, requiredFailed: 0 })).toBe("No required checks were reported");
  });

  it("landing_model_age_reads_in_seconds_minutes_and_hours", () => {
    expect(ageText(2)).toBe("Seen just now");
    expect(ageText(40)).toBe("Seen 40 seconds ago");
    expect(ageText(60)).toBe("Seen 1 minute ago");
    expect(ageText(7300)).toBe("Seen 2 hours ago");
    expect(ageText(-1)).toBe("Seen at an unknown time");
  });

  it("landing_model_remote_text_loses_control_characters_and_is_capped", () => {
    expect(plainRemote("a\u0000b\u001b[31m\nc\u2028d")).toBe("a b [31m c d");
    expect(Array.from(plainRemote("x".repeat(500))).length).toBe(121);
    expect(shortSha("abc123def4567890abc123def4567890abc12345")).toBe("abc123def456");
  });

  it("landing_model_merge_request_states_name_the_wait_and_the_refusal", () => {
    expect(mergeStateText(null)).toBeNull();
    expect(mergeStateText({ state: "issued", refusal: null })).toBe("Checking whether it merged");
    expect(mergeStateText({ state: "refused", refusal: "checks_red" })).toBe("Merge refused: a required check failed");
    expect(mergeStateText({ state: "merged", refusal: null })).toBeNull();
  });

  it("landing_model_unknown_refusal_code_is_said_not_hidden", () => {
    expect(refusalWords("head_moved")).toBe("the pull request changed after you were shown it");
    expect(refusalWords("odd_code")).toBe("the app refused it (odd_code)");
  });

  it("landing_model_refresh_errors_are_plain_and_a_rate_limit_asks_to_wait", () => {
    expect(pullsErrorText(new DaemonError("conflict", "rate limited"))).toMatch(/Wait about 10 seconds/);
    expect(pullsErrorText(new DaemonError("internal", "rate limited by host"))).toMatch(/Wait about 10 seconds/);
    expect(pullsErrorText(new DaemonError("not-found", "none"))).toMatch(/Nothing was changed/);
    expect(pullsErrorText(new DaemonError("timeout", "slow"))).toMatch(/did not answer/);
  });
});

describe("merge verdict", () => {
  it("landing_verdict_all_passed_is_ready", () => {
    expect(mergeVerdict(FACTS, null)).toEqual({ ready: true, text: "All required checks passed" });
  });

  it("landing_verdict_failed_pending_and_missing_counts_say_why", () => {
    expect(mergeVerdict({ ...FACTS, requiredPassed: 2, requiredFailed: 1 }, null)?.text).toBe("Not ready: a required check failed");
    expect(mergeVerdict({ ...FACTS, requiredPassed: 2 }, null)?.text).toBe("Not ready: a required check has not finished");
    expect(mergeVerdict({ ...FACTS, requiredTotal: 0, requiredPassed: 0 }, null)?.text).toBe(
      "Not ready: no required check was reported, so the app does not merge",
    );
  });

  it("landing_verdict_the_daemons_refusal_wins_over_the_counts", () => {
    expect(mergeVerdict({ ...FACTS, refusal: "not_mergeable" }, null)).toEqual({
      ready: false,
      text: "Not ready: GitHub says it cannot be merged now",
    });
    expect(mergeVerdict(FACTS, "head_moved")?.ready).toBe(false);
  });

  it("landing_verdict_with_no_facts_and_no_denial_says_nothing", () => {
    expect(mergeVerdict(undefined, null)).toBeNull();
    expect(mergeVerdict(undefined, "stale")?.text).toBe("Not ready: the app last looked at it too long ago");
  });
});
