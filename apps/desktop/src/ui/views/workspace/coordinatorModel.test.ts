import { describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { CoordinatorReport, CoordinatorState, RoutedRequest } from "../../../sources/surfaces/types";
import {
  COORDINATOR_MAX_ROWS,
  authorText,
  countsText,
  noteText,
  openRequestsOf,
  reportLabel,
  reportsOf,
  requestLabel,
  scopeError,
  scopeLockReason,
  scopeToSend,
  stateFrom,
  verbText,
} from "./coordinatorModel";

const request = (id: string, extra: Partial<RoutedRequest> = {}): RoutedRequest => ({
  id,
  work: "Write the troubleshooting page.",
  answered: false,
  createdAt: "2026-11-16T21:04:11Z",
  ...extra,
});
const report = (id: string, extra: Partial<CoordinatorReport> = {}): CoordinatorReport => ({
  id,
  author: "daemon",
  verb: "task_done",
  requestId: null,
  taskId: "t-12ab",
  key: null,
  note: null,
  createdAt: "2026-11-16T21:04:11Z",
  ...extra,
});
const state = (extra: Partial<CoordinatorState> = {}): CoordinatorState => ({
  scopeSentence: null,
  openTasks: 0,
  openRequests: 0,
  routedRequests: [],
  reports: [],
  ...extra,
});

describe("coordinatorModel", () => {
  it("coordinator_state_is_null_when_the_field_is_absent", () => {
    expect(stateFrom(null)).toBeNull();
    expect(stateFrom(undefined)).toBeNull();
    expect(stateFrom({})).toBeNull();
    expect(stateFrom({ coordinator: state() })).toEqual(state());
  });

  it("coordinator_open_requests_leave_out_answered_ones_and_stop_at_three", () => {
    const rows = [request("r-1", { answered: true }), request("r-2"), request("r-3"), request("r-4"), request("r-5")];
    expect(openRequestsOf(state({ routedRequests: rows })).map((row) => row.id)).toEqual(["r-2", "r-3", "r-4"]);
    const reports = ["a", "b", "c", "d"].map((id) => report(id));
    expect(reportsOf(state({ reports }))).toHaveLength(COORDINATOR_MAX_ROWS);
  });

  it("coordinator_counts_and_request_label_are_plain_text", () => {
    expect(countsText(state({ openTasks: 2, openRequests: 1 }))).toBe("Open tasks: 2. Open routed requests: 1.");
    expect(requestLabel(request("r-0123456789abcdef"))).toBe("r-0123456789abcdef: Write the troubleshooting page.");
    expect(requestLabel(request("r-1", { work: `${"a".repeat(200)}` })).length).toBeLessThan(200);
  });

  it("coordinator_report_names_the_author_and_the_verb_in_words", () => {
    expect(authorText("daemon")).toBe("App");
    expect(authorText("coordinator")).toBe("Coordinator");
    expect(authorText("other")).toBe("Unknown author");
    expect(verbText("task_ready")).toBe("Task ready for review");
    expect(verbText("question_answered")).toBe("Question answered");
    expect(verbText("caveat")).toBe("Caveat");
    expect(verbText("new_verb")).toBe("Unknown (new_verb)");
    const label = reportLabel(report("p-1", { author: "coordinator", verb: "answer", requestId: "r-1", taskId: null, key: "docs" }));
    expect(label).toContain("Coordinator: Answer, request r-1, key docs, ");
    expect(reportLabel(report("p-2"))).toContain("App: Task done, task t-12ab, ");
  });

  it("coordinator_note_is_one_plain_line_without_hidden_characters", () => {
    expect(noteText(report("p-1"))).toBeNull();
    expect(noteText(report("p-2", { note: "  " }))).toBeNull();
    expect(noteText(report("p-3", { note: "**Done**\u202e\nnext" }))).toBe("**Done** next");
  });

  it("coordinator_scope_check_counts_bytes_and_refuses_lines_and_hidden_characters", () => {
    expect(scopeError("")).toBeNull();
    expect(scopeError("Owns the user documentation.")).toBeNull();
    expect(scopeError("a".repeat(300))).toBeNull();
    expect(scopeError("a".repeat(301))).toMatch(/too long: 301 bytes/);
    // 150 two-byte letters are 150 UTF-16 units but 300 bytes; one more is over the daemon's limit.
    expect(scopeError("é".repeat(150))).toBeNull();
    expect(scopeError("é".repeat(151))).toMatch(/302 bytes/);
    expect(scopeError("one\ntwo")).toBe("The scope must be one line.");
    expect(scopeError("tab\there")).toBe("The scope must be one line.");
    expect(scopeError("bidi\u202eflip")).toMatch(/hidden or control character/);
    expect(scopeError("zero\u200bwidth")).toMatch(/hidden or control character/);
    expect(scopeError("joined\u200demoji")).toBeNull();
  });

  it("coordinator_scope_sends_the_trimmed_sentence_or_null_to_clear_it", () => {
    expect(scopeToSend("  Owns docs.  ")).toBe("Owns docs.");
    expect(scopeToSend("   ")).toBeNull();
  });

  it("coordinator_scope_lock_reason_needs_a_connected_approver_session_with_the_capability", () => {
    const ok = { live: true, connected: true, supported: true, approver: true };
    expect(scopeLockReason(ok)).toBeNull();
    expect(scopeLockReason({ ...ok, live: false, connected: false })).toBeNull();
    expect(scopeLockReason({ ...ok, connected: false })).toMatch(/^Offline/);
    expect(scopeLockReason({ ...ok, supported: false })).toMatch(/older/);
    expect(scopeLockReason({ ...ok, approver: false })).toBe(APPROVER_REQUIRED_NOTICE);
  });
});
