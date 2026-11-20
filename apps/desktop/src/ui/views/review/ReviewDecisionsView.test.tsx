import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { Approval } from "../../../daemon/types";
import { createFixtureChangeRequestSource } from "../../../review/sources";
import { DIFF, approval, evidenceFor, evidenceSource, makeClient, taskRecord } from "../../../review/testing";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { holdRange } from "./holdDates";
import { ReviewDecisionsView } from "./ReviewDecisionsView";

afterEach(cleanup);

function setup() {
  const approvals = [approval({ id: "approval-1", taskId: "task-1" }), approval({ id: "approval-2", taskId: "task-2", commit: "c".repeat(40) })];
  let list = [...approvals];
  const client = makeClient({
    "approvals.list": () => list.filter((a) => a.state.state === "requested"),
    "tasks.list": () => [taskRecord("task-1", "project-1"), taskRecord("task-2", "project-2")],
    "tasks.get": (p) => taskRecord(String(p.task_id), p.task_id === "task-2" ? "project-2" : "project-1"),
    "workspaces.diff": (p) => ({ task_id: p.task_id, workspace_id: "ws-1", diff: DIFF }),
    "approvals.resolve": (p) => {
      const target = list.find((a) => a.id === p.approval_id)!;
      const next: Approval = p.decision === "approve" ? { ...target, state: { state: "approved", by: { kind: "user", id: "u" } } } : { ...target, state: { state: "rejected" } };
      list = list.map((a) => (a.id === next.id ? next : a));
      return next;
    },
    "delivery.request": (p) => ({ task_id: p.task_id, step_id: "step-1", outcome: "recorded" }),
  });
  const services = createServices();
  let n = 0;
  render(
    <ServicesProvider services={services}>
      <ReviewDecisionsView
        client={client}
        evidenceSource={evidenceSource("daemon", (a) => ({ state: "available", evidence: evidenceFor(a) }), approvals)}
        changeRequestSource={createFixtureChangeRequestSource()}
        newKey={() => `key-${++n}`}
      />
    </ServicesProvider>,
  );
  const open = (taskId: string) => act(() => services.router.navigate({ name: "global", view: "review", taskId }));
  return { client, services, open };
}

describe("Review view routing", () => {
  it("opens_on_the_queue_with_one_h1_and_no_main_landmark", async () => {
    setup();
    expect(await screen.findByRole("heading", { level: 1, name: "Review" })).toBeTruthy();
    expect(screen.queryByRole("main")).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
  });

  it("lists_each_pending_candidate_and_opens_the_chosen_one_with_a_back_link_to_review", async () => {
    const { client, services } = setup();
    await userEvent.click(await screen.findByRole("button", { name: /^Review: Review changes: task task-2/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review", taskId: "task-2" });
    expect(await screen.findByRole("heading", { level: 1, name: /Objective of task-2/ })).toBeTruthy();
    expect(client.callsOf("tasks.get").map((p) => p.task_id)).toContain("task-2");
    await userEvent.click(screen.getByRole("button", { name: "← Review" }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review" });
    expect(await screen.findByRole("tab", { name: /^Needs you/ })).toBeTruthy();
  });

  it("shows_one_primary_accept_with_reject_beside_it_and_four_tabs", async () => {
    const { open } = setup();
    await open("task-1");
    const accept = await screen.findByRole("button", { name: "Accept" });
    expect(accept).toHaveClass("ui-btn-primary");
    expect(document.querySelectorAll(".ui-btn-primary")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Reject" })).not.toHaveClass("ui-btn-primary");
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Changes 2", "Evidence", "Request changes", "History"]);
  });

  it("keeps_the_six_facts_under_a_collapsed_technical_details_until_opened", async () => {
    const { open } = setup();
    await open("task-1");
    await screen.findByRole("button", { name: "Accept" });
    const details = screen.getByText("Technical details").closest("details")!;
    expect(details).not.toHaveAttribute("open");
    await userEvent.click(screen.getByText("Technical details"));
    expect(details).toHaveAttribute("open");
    await waitFor(() => expect(within(details).getByText("Six separate facts")).toBeTruthy());
  });

  it("accepting_records_the_decision_and_then_offers_the_separate_delivery_step", async () => {
    const { client, open } = setup();
    await open("task-1");
    await userEvent.click(await screen.findByRole("button", { name: "Accept" }));
    await waitFor(() => expect(client.callsOf("approvals.resolve")[0]).toMatchObject({ approval_id: "approval-1", decision: "approve" }));
    expect(await screen.findByRole("button", { name: "Request delivery" })).toBeTruthy();
    expect(client.callsOf("delivery.request")).toEqual([]);
  });

  it("says_what_a_hold_did_beside_its_control_on_the_candidate_page", async () => {
    const { open } = setup();
    await open("task-1");
    await userEvent.click(await screen.findByRole("button", { name: /^Hold until… decisions for task task-1/ }));
    fireEvent.change(screen.getByLabelText("Hold until"), { target: { value: holdRange(new Date()).max } });
    const submit = screen.getByRole("button", { name: "Hold decision" });
    await userEvent.click(submit);
    expect(submit).toHaveAttribute("aria-busy", "true");
    expect(await screen.findByText(/Held: task task-1 until/)).toBeTruthy();
  });
});
