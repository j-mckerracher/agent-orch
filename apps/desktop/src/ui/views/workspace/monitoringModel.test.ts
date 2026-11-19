import { describe, expect, it } from "vitest";
import type { ConversationEntry, MonitoringState, WakeRecord } from "../../../sources/surfaces/types";
import {
  MODE_OPTIONS,
  badgeLabel,
  chipLabel,
  isQuiet,
  isTaskEvent,
  notifyingCount,
  pausedFor,
  pausedText,
  placeWake,
  quietLine,
  quietTurnIds,
  skipText,
  targetNameWithBadge,
  unseenFor,
} from "./monitoringModel";
import { formatAt } from "./time";

const state = (mode: string): MonitoringState => ({ mode, paused: [], unseen: [] });
const entry = (id: string, sequence?: number): ConversationEntry => ({
  id,
  kind: "message",
  role: "user",
  text: id,
  at: "2026-11-16T10:00:00Z",
  ...(sequence === undefined ? {} : { sequence }),
});
const record = (id: string, anchorSequence: number, extra: Partial<WakeRecord> = {}): WakeRecord => ({
  id,
  event: "task_failed",
  subject: "t-1",
  notice: "Task t-1 failed.",
  state: "noticed",
  outcome: null,
  skipReason: null,
  turnId: null,
  anchorSequence,
  notify: true,
  seen: false,
  createdAt: "2026-11-16T10:00:00Z",
  ...extra,
});

describe("monitoring model", () => {
  it("monitoring_model_names_each_mode_in_the_chip_text_and_an_unknown_mode_as_unknown", () => {
    expect(["on", "quiet", "off"].map((mode) => chipLabel(state(mode)))).toEqual([
      "Monitoring: on",
      "Monitoring: quiet",
      "Monitoring: off",
    ]);
    expect(chipLabel(state("away"))).toBe("Monitoring: away");
    expect(chipLabel(state("later"))).toBe("Monitoring: unknown");
    expect(MODE_OPTIONS.map((option) => option.text)).toEqual([
      "On: tell me about updates",
      "Quiet: keep updates, do not notify me",
      "Off: notices only, no automatic replies",
      "Away: replies stay quiet, standing approvals keep working, and you get a summary when you return. It adds no permissions.",
    ]);
  });

  it("monitoring_model_paused_text_adds_the_resume_time_only_when_it_is_known_and_ahead", () => {
    const now = Date.parse("2026-11-16T10:00:00Z");
    const until = "2026-11-16T10:20:00Z";
    expect(pausedText({ until: null }, now)).toBe("Monitoring paused: limit reached");
    expect(pausedText({ until }, now)).toBe(`Monitoring paused: limit reached, resumes about ${formatAt(until)}`);
    expect(pausedText({ until }, Date.parse("2026-11-16T11:00:00Z"))).toBe("Monitoring paused: limit reached");
  });

  it("monitoring_model_finds_the_pause_and_the_unseen_count_of_one_scope", () => {
    const value: MonitoringState = {
      mode: "on",
      paused: [{ target: { kind: "workstream", workstreamId: "ws-1" }, reason: "hourly", until: null }],
      unseen: [{ target: { kind: "global" }, count: 2 }],
    };
    expect(pausedFor(value, { kind: "global" })).toBeNull();
    expect(pausedFor(value, { kind: "workstream", workstreamId: "ws-1" })?.reason).toBe("hourly");
    expect(unseenFor(value, { kind: "global" })).toBe(2);
    expect(unseenFor(null, { kind: "global" })).toBe(0);
  });

  it("monitoring_model_badge_label_is_singular_for_one_and_plural_otherwise", () => {
    expect(badgeLabel(1)).toBe("1 update");
    expect(badgeLabel(2)).toBe("2 updates");
    expect(targetNameWithBadge("Global orchestrator", 2)).toBe("Global orchestrator, 2 new updates");
    expect(targetNameWithBadge("Global orchestrator", 0)).toBe("Global orchestrator");
  });

  it("monitoring_model_place_wake_puts_a_notice_after_its_anchor_and_at_the_end_when_the_anchor_is_beyond", () => {
    const entries = [entry("a", 1), entry("b", 5), entry("sim"), entry("c", 9)];
    const placed = placeWake(entries, [record("r1", 5), record("r2", 7), record("r3", 99), record("r0", 0)]);
    expect(placed.before.map((item) => item.id)).toEqual(["r0"]);
    expect(placed.after.get("b")?.map((item) => item.id)).toEqual(["r1", "r2"]);
    expect(placed.after.get("c")?.map((item) => item.id)).toEqual(["r3"]);
    expect(placed.after.has("sim")).toBe(false);
  });

  it("monitoring_model_quiet_line_text_and_quiet_records_hide_their_turn", () => {
    expect(quietLine(3)).toBe("Checked 3 updates. Nothing needs you.");
    expect(quietLine(1)).toBe("Checked 1 update. Nothing needs you.");
    const records = [
      record("q", 1, { event: "work_check", state: "handled", outcome: "quiet", turnId: "turn-q", notify: false }),
      record("r", 1, { state: "handled", outcome: "replied", turnId: "turn-r" }),
    ];
    expect([...quietTurnIds(records)]).toEqual(["turn-q"]);
    expect(notifyingCount(records)).toBe(1);
  });

  it("monitoring_model_an_outcome_kind_is_never_quiet_even_when_its_reply_is_quiet", () => {
    const quiet = { state: "handled", outcome: "quiet", turnId: "turn-x", notify: false };
    expect(isQuiet(record("f", 1, quiet))).toBe(false);
    expect(isQuiet(record("s", 1, { ...quiet, event: "scout_report_ready" }))).toBe(false);
    expect(isQuiet(record("c", 1, { ...quiet, event: "candidate_ready" }))).toBe(false);
    expect(isQuiet(record("n", 1, { ...quiet, event: "worker_needs_decision" }))).toBe(false);
    expect(isQuiet(record("u", 1, { event: "work_check", state: "handled", outcome: "unanswered" }))).toBe(false);
    expect(isQuiet(record("w", 1, { ...quiet, event: "work_check" }))).toBe(true);
    expect(isQuiet(record("d", 1, { ...quiet, event: "daemon_start" }))).toBe(true);
    expect([...quietTurnIds([record("f", 1, quiet)])]).toEqual([]);
  });

  it("monitoring_model_a_task_event_is_linked_to_its_task_and_a_heartbeat_is_not", () => {
    expect(isTaskEvent(record("f", 1, { event: "task_failed" }))).toBe(true);
    expect(isTaskEvent(record("c", 1, { event: "candidate_ready" }))).toBe(true);
    expect(isTaskEvent(record("s", 1, { event: "scout_report_ready" }))).toBe(true);
    expect(isTaskEvent(record("d", 1, { event: "delivery_waiting" }))).toBe(true);
    expect(isTaskEvent(record("n", 1, { event: "worker_needs_decision" }))).toBe(true);
    expect(isTaskEvent(record("b", 1, { event: "worker_blocked" }))).toBe(true);
    expect(isTaskEvent(record("w", 1, { event: "work_check" }))).toBe(false);
    expect(isTaskEvent(record("p", 1, { event: "proposal_waiting" }))).toBe(false);
  });

  it("monitoring_model_skip_text_names_the_reason_in_words", () => {
    expect(skipText("mode_off")).toBe("Monitoring was off");
    expect(skipText("limit_daily")).toBe("Monitoring paused: limit reached");
    expect(skipText("new_word")).toMatch(/does not know/);
  });

  it("monitoring_model_agent_failing_skip_says_updates_are_paused_for_the_agent", () => {
    expect(skipText("agent_failing")).toBe("Automatic updates are paused while the agent is failing");
    expect(skipText("agent_failing")).not.toMatch(/does not know/);
  });
});
