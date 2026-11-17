import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import type { ActionProposal } from "../../../sources/surfaces/types";
import {
  actionPhrase,
  cardTitle,
  failureText,
  fieldsOf,
  newerRecord,
  openWorkstreamId,
  outcomeAnnouncement,
  requestErrorText,
  viewOf,
} from "./actionCardModel";

const at = "2026-11-10T21:00:00.000Z";
const later = "2026-11-10T21:05:00.000Z";
const NAMES = new Map([["ws-1", "Alpha stream"]]);
const LIVE = { canDecide: true, deciding: false, names: NAMES };

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

describe("approval card copy", () => {
  it("names each kind's heading, fields and button action", () => {
    expect(cardTitle("workstream.create")).toBe("Proposed: Create workstream");
    expect(actionPhrase(create, NAMES)).toBe("create workstream Post-MVP work");
    expect(fieldsOf(create, NAMES, null)).toEqual([
      { label: "Title", value: "Post-MVP work" },
      { label: "Summary", value: "(none)" },
    ]);

    const update = proposal({
      kind: "workstream.update",
      params: { workstream_id: "ws-1", expected_revision: 3, state: "archived" },
    });
    expect(cardTitle(update.kind)).toBe("Proposed: Update workstream");
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
    expect(cardTitle(unknown.kind)).toBe("Proposed: Unknown action");
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
    const offline = viewOf(create, null, { ...LIVE, canDecide: false });
    expect(offline.approve.enabled).toBe(false);
    expect(offline.reject.enabled).toBe(false);
    expect(offline.message.text).toBe("Reconnect to the daemon to approve or reject.");

    const sending = viewOf(create, null, { ...LIVE, deciding: true });
    expect(sending.approve.enabled).toBe(false);
    expect(sending.reject.enabled).toBe(false);
    expect(sending.message.text).toBe("Sending your decision…");
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
