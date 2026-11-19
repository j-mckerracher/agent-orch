import { describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE, DaemonError } from "../../../daemon/errors";
import type { ActionProposal } from "../../../sources/surfaces/types";
import {
  actionPhrase,
  cardTitle,
  failureText,
  fieldsOf,
  newerRecord,
  openTaskId,
  openWorkstreamId,
  outcomeAnnouncement,
  requestErrorText,
  viewOf,
} from "./actionCardModel";

const at = "2026-11-10T21:00:00.000Z";
const later = "2026-11-10T21:05:00.000Z";
const NAMES = new Map([["ws-1", "Alpha stream"]]);
const LIVE = { canDecide: true, connected: true, approver: true, deciding: false, names: NAMES };

function proposal(over: Partial<ActionProposal> = {}): ActionProposal {
  return {
    id: "proposal-1",
    turnId: "turn-1",
    index: 0,
    kind: "workstream.create",
    params: { title: "Post-MVP work", summary: "" },
    digest: "digest-1",
    dependsOn: null,
    state: "pending",
    resultRef: null,
    errorCode: null,
    denial: null,
    target: null,
    updatedAt: at,
    ...over,
  };
}

const create = proposal();
const dependent = proposal({
  id: "proposal-2",
  index: 1,
  kind: "objective.add",
  params: { workstream_ref: "proposal-1", title: "Ship it", description: "", gates: [{ description: "Tests pass" }] },
  dependsOn: "proposal-1",
});

describe("memory.set card", () => {
  const text = "Prefer small pull requests, one concern each, with a test for every fix and no drive-by refactors in the same change.";
  const memory = (document: string, over: Partial<ActionProposal> = {}) =>
    proposal({ kind: "memory.set", params: { kind: "memory.set", document, key: "small-prs-with-tests", text }, ...over });

  it("names the document in the heading and shows the full key and text", () => {
    expect(cardTitle("memory.set", memory("shared").params)).toBe("Proposed: Save a shared preference");
    expect(cardTitle("memory.set", memory("local").params)).toBe("Proposed: Save a preference for this conversation");
    expect(cardTitle("memory.set", memory("learnings").params)).toBe("Proposed: Record a learning");
    expect(fieldsOf(memory("shared"), NAMES, null)).toEqual([
      { label: "Saves to", value: "Shared preferences" },
      { label: "Key", value: "small-prs-with-tests" },
      { label: "Text", value: text },
    ]);
    expect(actionPhrase(memory("learnings"), NAMES)).toBe("record a learning small-prs-with-tests");
  });

  it("says Saved once executed and is a normal approvable proposal", () => {
    expect(viewOf(memory("local", { state: "executed" }), null, LIVE).pill.text).toBe("Saved");
    expect(viewOf(memory("learnings", { state: "executed" }), null, LIVE).message.text).toBe("Recorded the learning “small-prs-with-tests”.");
    const pending = viewOf(memory("shared"), null, LIVE);
    expect(pending.approve).toMatchObject({ enabled: true, variant: "primary" });
  });

  it("explains each denial and the budget failure in plain words", () => {
    const denied = (denial: string | null) => memory("shared", { state: "failed", errorCode: "action_denied", denial });
    expect(failureText(denied("shared_preferences_read_only"))).toBe(
      "Shared preferences are read-only here. Report it to the global conversation, or set it in Settings.",
    );
    expect(failureText(denied("no_local_preferences_here"))).toBe(
      "The global conversation has no local preferences. Use a shared preference instead.",
    );
    expect(failureText(denied("something_new"))).toBe("The app refused this entry. Nothing was saved.");
    expect(failureText(memory("shared", { state: "failed", errorCode: "over_budget" }))).toBe(
      "The document is over its byte budget. Nothing was saved.",
    );
  });
});

describe("approval card copy", () => {
  it("shows a task.message with its text and the task it resumes", () => {
    const message = proposal({
      kind: "task.message",
      params: { task_id: "task-9", text: "The database is back.\nRun the suite again." },
      target: { taskTitle: "Run the suite", taskState: "Waiting" },
    });
    expect(cardTitle("task.message", {})).toBe("Proposed: Message the waiting worker");
    expect(fieldsOf(message, NAMES, null)).toEqual([
      { label: "Task title", value: "Run the suite" },
      { label: "Task state now", value: "Waiting" },
      { label: "Message", value: ["The database is back.", "Run the suite again."] },
      { label: "Task id", value: "task-9" },
    ]);
    expect(openTaskId(message)).toBe("task-9");
  });

  it("names each kind's heading, fields and button action", () => {
    expect(cardTitle("workstream.create", {})).toBe("Proposed: Create workstream");
    expect(actionPhrase(create, NAMES)).toBe("create workstream Post-MVP work");
    expect(fieldsOf(create, NAMES, null)).toEqual([
      { label: "Title", value: "Post-MVP work" },
      { label: "Summary", value: "(none)" },
    ]);

    const update = proposal({
      kind: "workstream.update",
      params: { workstream_id: "ws-1", expected_revision: 3, state: "archived" },
    });
    expect(cardTitle(update.kind, {})).toBe("Proposed: Update workstream");
    expect(actionPhrase(update, NAMES)).toBe("update workstream Alpha stream");
    expect(fieldsOf(update, NAMES, null).map((field) => field.label)).toEqual([
      "Workstream",
      "Based on revision",
      "New title",
      "New summary",
      "New state",
    ]);

    expect(fieldsOf(dependent, NAMES, create)).toEqual([
      { label: "Workstream", value: "The workstream from “Create workstream: Post-MVP work”" },
      { label: "Title", value: "Ship it" },
      { label: "Description", value: "(none)" },
      { label: "Gates", value: ["Tests pass"] },
    ]);
  });

  it("shows the canonical fields of an unknown kind as sorted JSON", () => {
    const unknown = proposal({ kind: "workstream.delete", params: { zeta: 1, alpha: "x" } });
    expect(cardTitle(unknown.kind, {})).toBe("Proposed: Unknown action");
    expect(fieldsOf(unknown, NAMES, null)).toEqual([
      { label: "alpha", value: '"x"' },
      { label: "zeta", value: "1" },
    ]);
  });

  it("keeps Approve off for an unknown kind and says why", () => {
    const view = viewOf(proposal({ kind: "workstream.delete" }), null, LIVE);
    expect(view.approve.enabled).toBe(false);
    expect(view.approve.reason).toBe("This app version cannot show this action. Update the app to approve it.");
    expect(view.reject.enabled).toBe(true);
  });

  it("keeps Approve off until the parent has executed, and names the parent", () => {
    const waiting = viewOf(dependent, create, LIVE);
    expect(waiting.approve.enabled).toBe(false);
    expect(waiting.message.text).toBe(
      "Approve “Create workstream: Post-MVP work” first. This objective goes into that workstream.",
    );
    expect(waiting.reject.enabled).toBe(true);

    const executedParent = proposal({ state: "executed", resultRef: "ws-new" });
    expect(viewOf(dependent, executedParent, LIVE).approve.enabled).toBe(true);
  });

  it("keeps Approve off for a dependent whose parent is not in the list", () => {
    const unknownParent = viewOf(dependent, null, LIVE);
    expect(unknownParent.approve.enabled).toBe(false);
    expect(unknownParent.message.text).toBe(
      "This objective goes into a workstream that is not created yet. Approve that workstream first.",
    );
    expect(unknownParent.reject.enabled).toBe(true);
    expect(viewOf(create, null, LIVE).approve.enabled).toBe(true);
  });

  it("disables both decisions while disconnected and while one is in flight", () => {
    const offline = viewOf(create, null, { ...LIVE, canDecide: false, connected: false, approver: false });
    expect(offline.approve.enabled).toBe(false);
    expect(offline.reject.enabled).toBe(false);
    expect(offline.message.text).toBe("Reconnect to the daemon to approve or reject.");

    const sending = viewOf(create, null, { ...LIVE, deciding: true });
    expect(sending.approve.enabled).toBe(false);
    expect(sending.reject.enabled).toBe(false);
    expect(sending.message.text).toBe("Sending your decision…");
  });

  it("explains in plain words why a connected session that is not the approver cannot decide", () => {
    const view = viewOf(create, null, { ...LIVE, canDecide: false, approver: false });
    expect(view.approve.enabled).toBe(false);
    expect(view.approve.reason).toBe(APPROVER_REQUIRED_NOTICE);
    expect(view.reject.enabled).toBe(false);
    expect(view.reject.reason).toBe(APPROVER_REQUIRED_NOTICE);
    expect(view.message.text).toBe(APPROVER_REQUIRED_NOTICE);
  });

  it("uses the danger variant for an archive", () => {
    const archive = proposal({
      kind: "workstream.update",
      params: { workstream_id: "ws-1", expected_revision: 3, state: "archived" },
    });
    expect(viewOf(archive, null, LIVE).approve.variant).toBe("danger");
    expect(viewOf(proposal({ kind: "workstream.update", params: { workstream_id: "ws-1", state: "active" } }), null, LIVE).approve.variant).toBe(
      "primary",
    );
  });

  it("gives each state a pill with its text, and no decision outside pending", () => {
    expect(viewOf(create, null, LIVE).pill).toEqual({ tone: "info", text: "Pending approval" });
    expect(viewOf(proposal({ state: "approved" }), null, LIVE).pill.text).toBe("Approved, running");
    expect(viewOf(proposal({ state: "executed", resultRef: "ws-new" }), null, LIVE).pill.text).toBe("Created");
    expect(viewOf(proposal({ state: "rejected" }), null, LIVE).pill.text).toBe("Rejected");
    expect(viewOf(proposal({ state: "expired" }), null, LIVE).pill).toEqual({ tone: "warn", text: "Expired" });
    expect(viewOf(proposal({ state: "archived-later" }), null, LIVE).pill.text).toBe("Unknown state: archived-later");
    for (const state of ["approved", "executed", "rejected", "failed", "superseded", "expired"]) {
      const view = viewOf(proposal({ state }), null, LIVE);
      expect(view.approve.enabled, state).toBe(false);
      expect(view.reject.enabled, state).toBe(false);
    }
  });

  it.each([
    ["stale_revision", "A newer change to this workstream exists. Ask the orchestrator to propose again."],
    ["not_found", "The workstream, objective or gate this names no longer exists. Nothing was changed."],
    [
      "conflict",
      "The workstream is not in a state that allows this change, for example it is archived. Nothing was changed.",
    ],
    ["invalid", "The daemon could not apply these values. Nothing was changed. Ask the orchestrator to propose again."],
    ["dependency_not_executed", "The workstream this needs was not created, so this did not run. Nothing was changed."],
    ["internal", "The app could not finish this change. Nothing else was changed. Ask the orchestrator to propose it again."],
  ])("explains %s in plain words", (errorCode, expected) => {
    expect(failureText(proposal({ state: "failed", errorCode }))).toBe(expected);
  });

  it("says objective when a revise is stale, and falls back to the code", () => {
    const revise = proposal({ kind: "objective.revise", state: "failed", errorCode: "stale_revision" });
    expect(failureText(revise)).toBe("A newer change to this objective exists. Ask the orchestrator to propose again.");
    expect(failureText(proposal({ state: "failed", errorCode: "disk_full" }))).toBe("The action failed (disk_full).");
    expect(failureText(proposal({ state: "failed", errorCode: null }))).toBe("The action failed.");
  });

  it("reports a created workstream as an announcement and a rejection as a no-op", () => {
    const executed = proposal({ state: "executed", resultRef: "ws-new" });
    expect(outcomeAnnouncement(executed, null, NAMES)).toBe("Created workstream “Post-MVP work”.");
    expect(outcomeAnnouncement(proposal({ state: "rejected" }), null, NAMES)).toBe(
      "Rejected: create workstream Post-MVP work. Nothing was changed.",
    );
    expect(outcomeAnnouncement(proposal({ state: "failed", errorCode: "not_found" }), null, NAMES)).toBe(
      "Failed: create workstream Post-MVP work. The workstream, objective or gate this names no longer exists. Nothing was changed.",
    );
  });

  it("links Open workstream to the workstream each kind changes", () => {
    expect(openWorkstreamId(proposal({ state: "executed", resultRef: "ws-new" }), null, null)).toBe("ws-new");
    expect(openWorkstreamId(dependent, proposal({ state: "executed", resultRef: "ws-new" }), null)).toBe("ws-new");
    expect(openWorkstreamId(proposal({ kind: "objective.revise", params: { objective_id: "o-1" } }), null, "ws-1")).toBe("ws-1");
    expect(openWorkstreamId(proposal({ kind: "objective.revise", params: { objective_id: "o-1" } }), null, null)).toBeNull();
  });

  it.each([
    ["proposal-not-found", "This proposal no longer exists. The conversation history may have been cleared."],
    ["proposal-not-pending", "This proposal was already decided. Showing the daemon's current state."],
    [
      "idempotency-in-progress",
      "Still finishing. Refresh to see the result, then try again if it is still pending.",
    ],
    [
      "timeout",
      "The daemon did not answer. Your decision may have been recorded. Trying again reuses the same request.",
    ],
  ] as const)("says what a %s request error means", (kind, expected) => {
    expect(requestErrorText(new DaemonError(kind, "refused"), null)).toBe(expected);
  });

  it("names the parent when the daemon refuses a dependent approval", () => {
    expect(requestErrorText(new DaemonError("proposal-invalid", "refused", -32023), create)).toBe(
      "Approve “Create workstream: Post-MVP work” first.",
    );
    expect(requestErrorText(new DaemonError("proposal-invalid", "refused", -32023), null)).toBe(
      "The daemon cannot approve this now.",
    );
  });

  it("keeps the later record of a listed proposal and a daemon answer", () => {
    const listed = proposal();
    const answered = proposal({ state: "executed", updatedAt: later });
    expect(newerRecord(listed, undefined)).toBe(listed);
    expect(newerRecord(listed, answered)).toBe(answered);
    expect(newerRecord(listed, proposal({ state: "rejected", updatedAt: "2026-11-10T20:00:00.000Z" }))).toBe(listed);
  });
});

const TASK_PARAMS = {
  project: "p-3fa9c1d2",
  objective: "Add unit tests for the retry module.\nRun them locally and report what failed.",
  agent: "codex",
  task_kind: "Implement",
  delivery: "LocalReview",
};
const RUNNING_TASK = { taskTitle: "Add refresh-token expiry", taskState: "Running" };
const propose = proposal({ id: "proposal-3", kind: "task.propose", params: TASK_PARAMS, target: { projectName: "alpha" } });
const cancel = proposal({ id: "proposal-4", kind: "task.cancel", params: { task_id: "task-0123" }, target: RUNNING_TASK });
const retry = proposal({ id: "proposal-5", kind: "task.retry", params: { task_id: "task-0123" }, target: RUNNING_TASK });
const link = proposal({
  id: "proposal-6",
  kind: "workstream.link_task",
  params: { workstream_id: "ws-1", task_id: "task-0123" },
  target: RUNNING_TASK,
});
const denied = proposal({
  id: "proposal-7",
  kind: "task.propose",
  params: { ...TASK_PARAMS, delivery: "PullRequest", objective: "Fix the flaky login test and open a pull request." },
  target: { projectName: "alpha" },
  state: "failed",
  errorCode: "action_denied",
  denial: "delivery_not_authorized",
});
const waitingTask = proposal({
  id: "proposal-8",
  kind: "task.propose",
  params: { ...TASK_PARAMS, workstream_ref: "proposal-1" },
  dependsOn: "proposal-1",
});
const executedOf = (p: ActionProposal, over: Partial<ActionProposal> = {}): ActionProposal => ({ ...p, state: "executed", ...over });

describe("task card copy", () => {
  it("names each task kind's heading and button phrase", () => {
    expect(cardTitle("task.propose", {})).toBe("Proposed: Create task");
    expect(cardTitle("task.cancel", {})).toBe("Proposed: Cancel task");
    expect(cardTitle("task.retry", {})).toBe("Proposed: Retry task");
    expect(cardTitle("workstream.link_task", {})).toBe("Proposed: Link task");
    expect(actionPhrase(propose, NAMES)).toBe("create task Add unit tests for the retry module. Run them locally and re…");
    expect(actionPhrase(cancel, NAMES)).toBe("cancel task Add refresh-token expiry");
    expect(actionPhrase(retry, NAMES)).toBe("retry task Add refresh-token expiry");
    expect(actionPhrase(link, NAMES)).toBe("link task Add refresh-token expiry");
    expect(actionPhrase({ ...cancel, target: null }, NAMES)).toBe("cancel task task-0123");
  });

  it("cuts a long task to one line of 60 characters without splitting a character", () => {
    const long = proposal({ kind: "task.propose", params: { ...TASK_PARAMS, objective: "x".repeat(80) } });
    expect(actionPhrase(long, NAMES)).toBe(`create task ${"x".repeat(60)}…`);
    const emoji = proposal({ kind: "task.propose", params: { ...TASK_PARAMS, objective: "😀".repeat(70) } });
    expect(actionPhrase(emoji, NAMES)).toBe(`create task ${"😀".repeat(60)}…`);
    const exact = proposal({ kind: "task.propose", params: { ...TASK_PARAMS, objective: "y".repeat(60) } });
    expect(actionPhrase(exact, NAMES)).toBe(`create task ${"y".repeat(60)}`);
  });

  it("shows a proposed task's exact parameters and the two fixed policy rows", () => {
    expect(fieldsOf(propose, NAMES, null)).toEqual([
      { label: "Project", value: "alpha (p-3fa9c1d2)" },
      { label: "Task", value: ["Add unit tests for the retry module.", "Run them locally and report what failed."] },
      { label: "Agent", value: "codex" },
      { label: "Task kind", value: "Implement" },
      { label: "Delivery", value: "LocalReview" },
      { label: "Autonomy", value: "Approval required, fixed by the app" },
      { label: "Verification", value: "None, fixed by the app" },
    ]);
  });

  it("lists a task objective one entry per line and drops blank lines", () => {
    const spaced = proposal({ kind: "task.propose", params: { ...TASK_PARAMS, objective: "First\r\n\n  \nSecond" } });
    expect(fieldsOf(spaced, NAMES, null)[1]).toEqual({ label: "Task", value: ["First", "Second"] });
  });

  it("shows the project code alone when the daemon named no project, and the workstream and objective a task joins", () => {
    expect(fieldsOf({ ...propose, target: null }, NAMES, null)[0]).toEqual({ label: "Project", value: "p-3fa9c1d2" });

    const linked = proposal({ kind: "task.propose", params: { ...TASK_PARAMS, workstream_id: "ws-1", objective_id: "objective-9" } });
    expect(fieldsOf(linked, NAMES, null).slice(-2)).toEqual([
      { label: "Workstream", value: "Alpha stream" },
      { label: "Objective", value: "objective-9" },
    ]);
    expect(fieldsOf(waitingTask, NAMES, create).slice(-1)).toEqual([
      { label: "Workstream", value: "The workstream from “Create workstream: Post-MVP work”" },
    ]);
  });

  it("shows a cancel, retry and link by the task's title, state now and id", () => {
    expect(fieldsOf(cancel, NAMES, null)).toEqual([
      { label: "Task title", value: "Add refresh-token expiry" },
      { label: "Task state now", value: "Running" },
      { label: "Task id", value: "task-0123" },
    ]);
    expect(fieldsOf({ ...retry, target: null }, NAMES, null)).toEqual([
      { label: "Task title", value: "(not available)" },
      { label: "Task state now", value: "(not available)" },
      { label: "Task id", value: "task-0123" },
    ]);
    expect(fieldsOf(link, NAMES, null)).toEqual([
      { label: "Workstream", value: "Alpha stream" },
      { label: "Task title", value: "Add refresh-token expiry" },
      { label: "Task id", value: "task-0123" },
      { label: "Objective", value: "(none)" },
    ]);
  });

  it("uses the danger variant for a task cancel and the primary variant for the other task kinds", () => {
    expect(viewOf(cancel, null, LIVE).approve.variant).toBe("danger");
    for (const p of [propose, retry, link]) expect(viewOf(p, null, LIVE).approve.variant, p.kind).toBe("primary");
    for (const p of [propose, cancel, retry, link]) expect(viewOf(p, null, LIVE).approve.enabled, p.kind).toBe(true);
  });

  it("keeps a proposed task's Approve off until the workstream it joins is created", () => {
    const waiting = viewOf(waitingTask, create, LIVE);
    expect(waiting.approve.enabled).toBe(false);
    expect(waiting.message.text).toBe("Approve “Create workstream: Post-MVP work” first. This task goes into that workstream.");
    expect(viewOf(waitingTask, null, LIVE).message.text).toBe(
      "This task goes into a workstream that is not created yet. Approve that workstream first.",
    );
    expect(viewOf(waitingTask, proposal({ state: "executed", resultRef: "ws-new" }), LIVE).approve.enabled).toBe(true);
  });

  it("gives an executed task change its pill and says a cancel was only requested", () => {
    expect(viewOf(executedOf(propose, { resultRef: "task-new" }), null, LIVE).pill).toEqual({ tone: "ok", text: "Queued" });
    expect(viewOf(executedOf(retry), null, LIVE).pill.text).toBe("Retry queued");
    expect(viewOf(executedOf(link), null, LIVE).pill.text).toBe("Linked");

    const stopped = viewOf(executedOf(cancel), null, LIVE);
    expect(stopped.pill).toEqual({ tone: "ok", text: "Cancellation requested" });
    expect(stopped.message.text).toBe("Cancellation requested for “Add refresh-token expiry”. The task stops when its agent confirms.");
    expect(`${stopped.pill.text} ${stopped.message.text}`).not.toMatch(/cancelled/i);
  });

  it("announces what each executed task change did", () => {
    expect(outcomeAnnouncement(executedOf(propose), null, NAMES)).toBe(
      "Created and queued task “Add unit tests for the retry module. Run them locally and re…”. Open the task to follow it.",
    );
    expect(outcomeAnnouncement(executedOf(cancel), null, NAMES)).toBe(
      "Cancellation requested for “Add refresh-token expiry”. The task stops when its agent confirms.",
    );
    expect(outcomeAnnouncement(executedOf(retry), null, NAMES)).toBe("Retry queued for “Add refresh-token expiry”.");
    expect(outcomeAnnouncement(executedOf(link), null, NAMES)).toBe("Linked “Add refresh-token expiry” to the workstream.");
  });

  it.each([
    [
      "delivery_not_authorized",
      "This task asks for a delivery the app does not allow from chat. Nothing was created. Ask the orchestrator for local review or a report only.",
    ],
    ["autonomy_not_authorized", "This task asks for more autonomy than chat may request. Nothing was created."],
    ["verification_not_authorized", "This task asks for verification commands, which chat may not request. Nothing was created."],
    ["contract_invalid", "The task text is not valid. Nothing was created. Ask the orchestrator to propose again."],
    ["a_key_from_a_newer_daemon", "The app refused this task. Nothing was created."],
  ])("explains the %s denial in plain words", (denial, expected) => {
    expect(viewOf({ ...denied, denial }, null, LIVE).message).toEqual({ tone: "warn", text: expected });
  });

  it("shows a denied proposal as a Denied warning with no decision, and any other failure as Failed", () => {
    const view = viewOf(denied, null, LIVE);
    expect(view.state).toBe("failed");
    expect(view.pill).toEqual({ tone: "warn", text: "Denied" });
    expect(view.approve.enabled).toBe(false);
    expect(view.reject.enabled).toBe(false);
    expect(viewOf({ ...denied, denial: null }, null, LIVE).message.text).toBe("The app refused this task. Nothing was created.");
    expect(viewOf({ ...denied, errorCode: "conflict", denial: null }, null, LIVE).pill).toEqual({ tone: "bad", text: "Failed" });
  });

  it("announces a denied proposal as Denied, and a task's own full stop or ellipsis is not doubled", () => {
    expect(outcomeAnnouncement(denied, null, NAMES)).toBe(
      "Denied: create task Fix the flaky login test and open a pull request. This task asks for a delivery the app does not allow from chat. Nothing was created. Ask the orchestrator for local review or a report only.",
    );
    expect(outcomeAnnouncement({ ...propose, state: "rejected" }, null, NAMES)).toBe(
      "Rejected: create task Add unit tests for the retry module. Run them locally and re… Nothing was changed.",
    );
    expect(outcomeAnnouncement({ ...cancel, state: "rejected" }, null, NAMES)).toBe(
      "Rejected: cancel task Add refresh-token expiry. Nothing was changed.",
    );
  });

  it.each([
    ["not_found", "The project, task or workstream this names was not found. Nothing was changed."],
    [
      "conflict",
      "The task, workstream or link is not in a state that allows this (for example the task is already running or finished). Nothing was changed.",
    ],
    ["forbidden", "The app does not allow this change. Nothing was changed."],
    ["invalid", "The daemon could not apply these values. Nothing was changed. Ask the orchestrator to propose again."],
    ["dependency_not_executed", "The workstream this needs was not created, so this did not run. Nothing was changed."],
  ])("explains %s for a task change in plain words", (errorCode, expected) => {
    for (const p of [propose, cancel, retry, link]) expect(failureText({ ...p, state: "failed", errorCode }), p.kind).toBe(expected);
  });

  it("words forbidden for a workstream change too and keeps its workstream wording for conflict", () => {
    expect(failureText(proposal({ state: "failed", errorCode: "forbidden" }))).toBe("The app does not allow this change. Nothing was changed.");
    expect(failureText(proposal({ state: "failed", errorCode: "conflict" }))).toBe(
      "The workstream is not in a state that allows this change, for example it is archived. Nothing was changed.",
    );
  });

  it("links Open task to the task a propose created or a cancel and a retry name", () => {
    expect(openTaskId(executedOf(propose, { resultRef: "task-new" }))).toBe("task-new");
    expect(openTaskId(propose)).toBeNull();
    expect(openTaskId(cancel)).toBe("task-0123");
    expect(openTaskId(retry)).toBe("task-0123");
    expect(openTaskId(link)).toBeNull();
    expect(openTaskId(create)).toBeNull();
  });

  it("links Open workstream to the workstream a task joins", () => {
    const joined = proposal({ kind: "task.propose", params: { ...TASK_PARAMS, workstream_id: "ws-1" } });
    expect(openWorkstreamId(joined, null, null)).toBe("ws-1");
    expect(openWorkstreamId(waitingTask, proposal({ state: "executed", resultRef: "ws-new" }), null)).toBe("ws-new");
    expect(openWorkstreamId(link, null, null)).toBe("ws-1");
    expect(openWorkstreamId(propose, null, "ws-9")).toBeNull();
    expect(openWorkstreamId(cancel, null, "ws-1")).toBeNull();
  });
});

const revise = proposal({
  id: "proposal-8",
  kind: "task.revise",
  params: { task_id: "task-0123", added_intent: "Also cover the expired token.\nKeep the public API." },
  target: RUNNING_TASK,
});
const hold = proposal({
  id: "proposal-9",
  kind: "decision.hold",
  params: { task_id: "task-0123", until: "2026-11-21" },
  target: RUNNING_TASK,
});
const remove = proposal({
  id: "proposal-10",
  kind: "project.remove",
  params: { project: "p-3fa9c1d2" },
  target: { projectName: "alpha", taskCount: 4, runningTasks: 0 },
});
const failedWith = (p: ActionProposal, errorCode: string): ActionProposal => ({ ...p, state: "failed", errorCode });

describe("revise, hold and remove card copy", () => {
  it("phase4d_names_the_three_kinds_headings_button_phrases_and_done_labels", () => {
    expect(cardTitle("task.revise", {})).toBe("Proposed: Revise task");
    expect(cardTitle("decision.hold", {})).toBe("Proposed: Hold decision");
    expect(cardTitle("project.remove", {})).toBe("Proposed: Remove project");
    expect(actionPhrase(revise, NAMES)).toBe("revise task Add refresh-token expiry");
    expect(actionPhrase(hold, NAMES)).toBe("hold decision Add refresh-token expiry");
    expect(actionPhrase(remove, NAMES)).toBe("remove project alpha");
    expect(actionPhrase({ ...remove, target: null }, NAMES)).toBe("remove project p-3fa9c1d2");
    const done = (p: ActionProposal) => viewOf({ ...p, state: "executed" }, null, LIVE).pill.text;
    expect([done(revise), done(hold), done(remove)]).toEqual(["Revision started", "Held", "Removed"]);
  });

  it("phase4d_revise_card_shows_the_title_the_whole_added_instruction_and_the_effect", () => {
    expect(fieldsOf(revise, NAMES, null)).toEqual([
      { label: "Task title", value: "Add refresh-token expiry" },
      { label: "Task state now", value: "Running" },
      { label: "Added instruction", value: ["Also cover the expired token.", "Keep the public API."] },
      {
        label: "What happens",
        value: "The current attempt stops. Its work is kept. A new task starts with the old objective plus this instruction.",
      },
      { label: "Task id", value: "task-0123" },
    ]);
    expect(fieldsOf({ ...revise, target: null }, NAMES, null)[0]).toEqual({ label: "Task title", value: "(not available)" });
  });

  it("phase4d_hold_card_shows_the_date_in_words_and_where_the_decision_stays", () => {
    const rows = fieldsOf(hold, NAMES, null);
    expect(rows).toContainEqual({ label: "Until", value: "21 November 2026" });
    expect(rows).toContainEqual({
      label: "What happens",
      value: "It stays in Review and Decisions, marked Held, and comes back on that day.",
    });
  });

  it("phase4d_remove_card_shows_the_folder_name_and_the_task_counts_never_a_path", () => {
    expect(fieldsOf(remove, NAMES, null)).toEqual([
      { label: "Project", value: "alpha (p-3fa9c1d2)" },
      { label: "Tasks", value: "4 tasks are kept in history. Running tasks: 0" },
      expect.objectContaining({ label: "What happens" }),
    ]);
    const one = { ...remove, target: { projectName: "alpha", taskCount: 1, runningTasks: 1 } };
    expect(fieldsOf(one, NAMES, null)[1]).toEqual({ label: "Tasks", value: "1 task is kept in history. Running tasks: 1" });
    expect(fieldsOf({ ...remove, target: null }, NAMES, null)[1]).toEqual({ label: "Tasks", value: "(not available)" });
  });

  it("phase4d_pending_cards_are_approver_gated_with_a_visible_reason", () => {
    for (const card of [revise, hold, remove]) {
      const on = viewOf(card, null, LIVE);
      expect(on.approve.enabled).toBe(true);
      const off = viewOf(card, null, { ...LIVE, approver: false });
      expect(off.approve).toMatchObject({ enabled: false, reason: APPROVER_REQUIRED_NOTICE });
      expect(off.reject.enabled).toBe(false);
    }
    expect(viewOf(revise, null, LIVE).approve.variant).toBe("danger");
    expect(viewOf(hold, null, LIVE).approve.variant).toBe("primary");
  });

  it("phase4d_failure_words_per_kind_end_by_saying_nothing_was_changed", () => {
    for (const card of [revise, hold, remove]) {
      for (const code of ["conflict", "invalid", "not_found"]) {
        const words = failureText(failedWith(card, code));
        expect(words).toMatch(/Nothing was changed\.$/);
        expect(words).not.toMatch(/workstream/i);
      }
    }
    expect(failureText(failedWith(revise, "conflict"))).toMatch(/already finished/);
    expect(failureText(failedWith(hold, "invalid"))).toMatch(/60 days/);
    expect(failureText(failedWith(remove, "conflict"))).toMatch(/running work or decisions waiting/);
    expect(failureText(failedWith(remove, "forbidden"))).toBe("The app does not allow this change. Nothing was changed.");
  });

  it("phase4d_executed_and_announced_text_names_the_result", () => {
    const executed = (p: ActionProposal) => viewOf({ ...p, state: "executed" }, null, LIVE).message.text;
    expect(executed(revise)).toBe(
      "Revision started for “Add refresh-token expiry”. The old attempt is stopping and a new task is queued.",
    );
    expect(executed(hold)).toBe("Held the decision for “Add refresh-token expiry” until 21 November 2026.");
    expect(executed(remove)).toBe("Removed project “alpha”. Its tasks stay in history.");
    expect(outcomeAnnouncement(failedWith(hold, "conflict"), null, NAMES)).toBe(
      "Failed: hold decision Add refresh-token expiry. The task has already finished, so there is nothing to hold. Nothing was changed.",
    );
  });

  it("phase4d_open_task_goes_to_the_new_task_for_a_revise_and_the_held_task_for_a_hold", () => {
    expect(openTaskId(executedOf(revise, { resultRef: "task-new" }))).toBe("task-new");
    expect(openTaskId(revise)).toBeNull();
    expect(openTaskId(hold)).toBe("task-0123");
    expect(openTaskId(remove)).toBeNull();
  });
});
