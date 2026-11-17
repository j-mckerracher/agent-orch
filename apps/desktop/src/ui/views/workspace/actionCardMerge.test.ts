import { describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { ActionProposal, PullFacts } from "../../../sources/surfaces/types";
import { actionPhrase, cardTitle, failureText, fieldsOf, openTaskId, viewOf, type ViewContext } from "./actionCardModel";

const SHA = "abc123def4567890abc123def4567890abc12345";
const PASSED: PullFacts = {
  mergeMethod: "squash",
  requiredTotal: 2,
  requiredPassed: 2,
  requiredFailed: 0,
  checks: [
    { name: "build", result: "pass" },
    { name: "lint", result: "pass" },
  ],
  observedAgeSecs: 40,
  refusal: null,
};
const CTX: ViewContext = { canDecide: true, connected: true, approver: true, deciding: false, names: new Map() };

function merge(over: Partial<ActionProposal> = {}, pull: PullFacts | null = PASSED, title = "Retry the sync call"): ActionProposal {
  return {
    id: "proposal-m",
    turnId: "turn-1",
    index: 0,
    kind: "delivery.merge",
    params: { task_id: "task-9", pr_number: 42, head_sha: SHA },
    digest: "digest-m",
    dependsOn: null,
    state: "pending",
    resultRef: null,
    errorCode: null,
    denial: null,
    target: { taskTitle: title, ...(pull === null ? {} : { pull }) },
    updatedAt: "2026-11-17T20:00:00.000Z",
    ...over,
  };
}

const rows = (card: ActionProposal) => Object.fromEntries(fieldsOf(card, new Map(), null).map((f) => [f.label, f.value]));

describe("delivery.merge card", () => {
  it("landing_card_rows_show_title_number_short_commit_method_summary_and_the_warning", () => {
    const r = rows(merge());
    expect(cardTitle("delivery.merge")).toBe("Proposed: Merge pull request");
    expect(r["Task title"]).toBe("Retry the sync call");
    expect(r["Pull request"]).toBe("42");
    expect(r["Commit"]).toBe("abc123def456");
    expect(r["Merge method"]).toBe("Squash");
    expect(r["Checks"]).toEqual(["build: Passed", "lint: Passed"]);
    expect(r["Check summary"]).toBe("All required checks passed");
    expect(r["Seen"]).toBe("40 seconds ago");
    expect(r["What happens"]).toBe("Approving merges exactly this commit on GitHub and cannot be undone from here.");
  });

  it("landing_card_green_is_approvable_and_the_button_names_the_number_and_commit", () => {
    const card = merge();
    const view = viewOf(card, null, CTX);
    expect(view.approve.enabled).toBe(true);
    expect(view.approve.variant).toBe("danger");
    expect(actionPhrase(card, new Map())).toBe("merge pull request 42 at commit abc123def456");
  });

  it.each([
    ["a failed required check", { requiredPassed: 1, requiredFailed: 1 }, "Not ready: a required check failed"],
    ["an unfinished required check", { requiredPassed: 1 }, "Not ready: a required check has not finished"],
    ["no required check", { requiredTotal: 0, requiredPassed: 0 }, "Not ready: no required check was reported, so the app does not merge"],
    ["a refusal the daemon reports", { refusal: "not_mergeable" }, "Not ready: GitHub says it cannot be merged now"],
  ])("landing_card_not_ready_for_%s_disables_approve_and_says_why_in_words", (_name, change, words) => {
    const card = merge({}, { ...PASSED, ...change });
    expect(rows(card)["Check summary"]).toBe(words);
    const view = viewOf(card, null, CTX);
    expect(view.approve).toMatchObject({ enabled: false, reason: words });
    expect(view.message.text).toBe(words);
    // Reject stays available: the person can still say no.
    expect(view.reject.enabled).toBe(true);
  });

  it("landing_card_without_facts_does_not_claim_checks_passed", () => {
    const card = merge({}, null);
    expect(rows(card)["Check summary"]).toBe("(not available)");
    expect(rows(card)["Merge method"]).toBe("(not available)");
    expect(viewOf(card, null, CTX).approve.enabled).toBe(true);
  });

  it("landing_card_denied_at_creation_is_failed_with_the_reason_and_no_decision", () => {
    const card = merge({ state: "failed", errorCode: "action_denied", denial: "checks_red" });
    const view = viewOf(card, null, CTX);
    expect(view.pill).toEqual({ tone: "warn", text: "Denied" });
    expect(view.message.text).toBe("A required check failed. Nothing was merged.");
    expect(view.approve.enabled).toBe(false);
    expect(view.reject.enabled).toBe(false);
    expect(rows(card)["Check summary"]).toBe("Not ready: a required check failed");
  });

  it.each([
    ["head_moved", "The pull request changed after you were shown it. Nothing was merged. Ask the orchestrator to propose again."],
    ["checks_pending", "A required check has not finished. Nothing was merged."],
    ["checks_missing", "No required check was reported, so the app does not merge. Nothing was merged."],
    ["not_mergeable", "GitHub says this cannot be merged now (for example a conflict or a required review). Nothing was merged."],
    ["stale", "The app last looked at this pull request too long ago. Nothing was merged."],
    ["project_mode", "This project's mode is local-only. Nothing was merged."],
  ])("landing_card_refusal_%s_reads_as_plain_words", (code, words) => {
    expect(failureText(merge({ state: "failed", errorCode: "action_denied", denial: code }))).toBe(words);
    expect(failureText(merge({ state: "failed", errorCode: code }))).toBe(words);
  });

  it("landing_card_failure_texts_say_whether_the_host_was_asked", () => {
    expect(failureText(merge({ state: "failed", errorCode: "merge_unconfirmed" }))).toBe(
      "The app asked GitHub to merge but could not confirm it. Check the pull request on GitHub before asking again.",
    );
    expect(failureText(merge({ state: "failed", errorCode: "not_signed_in" }))).toBe(
      "The GitHub command line is not signed in on this machine. Nothing was merged.",
    );
    expect(failureText(merge({ state: "failed", errorCode: "action_denied", denial: "odd" }))).toBe("The app refused this merge. Nothing was merged.");
  });

  it("landing_card_after_approval_says_the_merge_is_being_checked_and_has_no_buttons", () => {
    const view = viewOf(merge({ state: "approved" }), null, CTX);
    expect(view.message.text).toMatch(/exact commit/);
    expect(view.approve.enabled).toBe(false);
  });

  it("landing_card_superseded_says_the_pull_request_changed", () => {
    expect(viewOf(merge({ state: "superseded" }), null, CTX).message.text).toBe(
      "Superseded: the pull request changed. Ask the orchestrator to propose again. Nothing was merged.",
    );
  });

  it("landing_card_executed_says_merged_with_the_number_and_commit", () => {
    const view = viewOf(merge({ state: "executed" }), null, CTX);
    expect(view.pill.text).toBe("Merged");
    expect(view.message.text).toBe("Merged pull request 42 at commit abc123def456.");
    expect(openTaskId(merge({ state: "executed" }))).toBe("task-9");
  });

  it("landing_card_is_locked_for_a_non_approver_with_the_notice", () => {
    const view = viewOf(merge(), null, { ...CTX, approver: false });
    expect(view.approve).toMatchObject({ enabled: false, reason: APPROVER_REQUIRED_NOTICE });
    expect(view.reject.enabled).toBe(false);
  });

  it("landing_card_hostile_title_and_check_names_stay_plain_capped_text", () => {
    const hostile = "<img src=x onerror=alert(1)>\u0000\u001b[31m" + "t".repeat(400);
    const pull: PullFacts = { ...PASSED, checks: [{ name: "<script>boom</script>\nline two", result: "pass" }] };
    const r = rows(merge({}, pull, hostile));
    expect(r["Task title"]).not.toMatch(/[\u0000-\u001f]/);
    expect(Array.from(r["Task title"] as string).length).toBeLessThanOrEqual(121);
    // Text stays text: the model only copies characters, and React renders strings as text nodes.
    expect(r["Task title"]).toContain("<img src=x onerror=alert(1)>");
    expect(r["Checks"]).toEqual(["<script>boom</script> line two: Passed"]);
    expect(actionPhrase(merge({}, pull, hostile), new Map())).not.toContain("<img");
  });
});
