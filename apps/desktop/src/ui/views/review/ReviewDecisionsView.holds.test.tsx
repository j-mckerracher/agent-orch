import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import { createFixtureChangeRequestSource } from "../../../review/sources";
import { approval, evidenceFor, evidenceSource, makeClient, taskRecord } from "../../../review/testing";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { populate } from "../../../sources/surfaces/testkit";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { holdRange } from "./holdDates";
import { ReviewDecisionsView } from "./ReviewDecisionsView";

const APPROVALS = [approval({ id: "approval-1", taskId: "task-1" }), approval({ id: "approval-2", taskId: "task-2", commit: "c".repeat(40) })];

function view(services: Services) {
  const client = makeClient({
    "approvals.list": () => APPROVALS,
    "tasks.list": () => [taskRecord("task-1", "project-1"), taskRecord("task-2", "project-2")],
    "tasks.get": (p) => taskRecord(String(p.task_id), "project-1"),
  });
  return render(
    <ServicesProvider services={services}>
      <ReviewDecisionsView
        client={client}
        evidenceSource={evidenceSource("daemon", (a) => ({ state: "available", evidence: evidenceFor(a) }), APPROVALS)}
        changeRequestSource={createFixtureChangeRequestSource()}
        newKey={() => "key"}
      />
    </ServicesProvider>,
  );
}

const panel = () => screen.getByRole("tabpanel");
const status = () => screen.getByTestId("holds-status");

describe("Held tab in fixture mode", () => {
  it("held_tab_lists_the_held_decision_with_its_date_in_words_and_a_release_button", async () => {
    view(createServices());
    await screen.findByText("2 independent decisions");
    await userEvent.click(await screen.findByRole("tab", { name: "Held (1)" }));
    expect(screen.getByRole("tab", { name: "Held (1)" })).toHaveAttribute("aria-selected", "true");
    expect(panel()).toHaveTextContent("Decisions you postponed. Each returns on its date.");
    const row = within(panel()).getByRole("listitem");
    expect(row).toHaveTextContent("Task fixture:task-docs-1");
    expect(row).toHaveTextContent("Held until 9 January");
    expect(within(row).getByText("Held")).toBeTruthy();
    expect(within(row).getByRole("button", { name: "Release: Task fixture:task-docs-1" })).toBeEnabled();
  });

  it("held_release_says_so_in_a_polite_region_keeps_the_fixture_notice_and_moves_focus_to_the_panel", async () => {
    view(createServices());
    await screen.findByText("2 independent decisions");
    await userEvent.click(await screen.findByRole("tab", { name: "Held (1)" }));
    expect(status()).toHaveAttribute("aria-live", "polite");
    await userEvent.click(screen.getByRole("button", { name: "Release: Task fixture:task-docs-1" }));
    await waitFor(() => expect(status()).toHaveTextContent("Released: Task fixture:task-docs-1."));
    expect(status()).toHaveTextContent("Fixture data: nothing was changed.");
    expect(screen.getByRole("tab", { name: "Held (0)" })).toBeTruthy();
    expect(panel()).toHaveTextContent("Nothing is held");
    await waitFor(() => expect(panel()).toHaveFocus());
  });

  it("held_hold_control_has_a_labelled_date_input_limited_to_today_through_sixty_days_with_a_hint", async () => {
    view(createServices());
    await screen.findByText("2 independent decisions");
    await userEvent.click(screen.getByRole("button", { name: "Hold until… decisions for task task-1" }));
    const input = screen.getByLabelText("Hold until");
    const range = holdRange(new Date());
    expect(input).toHaveAttribute("type", "date");
    expect(input).toHaveAttribute("min", range.min);
    expect(input).toHaveAttribute("max", range.max);
    expect(input).toHaveAccessibleDescription(/Pick a day from today to 60 days from today/);
    expect(screen.getByRole("button", { name: "Hold decision" })).toBeDisabled();
  });

  it("held_holding_a_task_moves_it_out_of_actionable_and_the_count_into_held", async () => {
    view(createServices());
    await screen.findByText("2 independent decisions");
    await screen.findByRole("tab", { name: "Held (1)" });
    expect(screen.getByRole("tab", { name: "Actionable (2)" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Hold until… decisions for task task-1" }));
    const day = holdRange(new Date()).max;
    fireEvent.change(screen.getByLabelText("Hold until"), { target: { value: day } });
    await userEvent.click(screen.getByRole("button", { name: "Hold decision" }));
    await waitFor(() => expect(status()).toHaveTextContent("Held: task task-1 until"));
    expect(status()).toHaveTextContent("Fixture data: nothing was changed.");
    expect(screen.getByRole("tab", { name: "Actionable (1)" })).toBeTruthy();
    expect(screen.getByText("1 independent decision")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Held (2)" })).toBeTruthy();
    expect(within(panel()).queryByRole("button", { name: /Task task-1/ })).toBeNull();
    await userEvent.click(screen.getByRole("tab", { name: "Held (2)" }));
    expect(within(panel()).getByRole("button", { name: "Review now: Task task-1" })).toBeTruthy();
  });
});

describe("Held tab against a daemon", () => {
  async function connected(setup: (fake: FakeDaemon) => void) {
    const fake = populate(new FakeDaemon());
    fake.stub("holds.list", { holds: [{ id: "hold-1", subject_kind: "approval", subject_id: "task-1", until_date: "2026-11-21", state: "active" }] });
    setup(fake);
    const services = createServices({ connector: fake });
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    return { fake, services };
  }

  it("held_release_without_the_approver_role_is_disabled_with_the_reason_linked", async () => {
    const { fake, services } = await connected((f) => {
      f.approver = false;
    });
    view(services);
    await userEvent.click(await screen.findByRole("tab", { name: /^Held/ }));
    const release = await screen.findByRole("button", { name: "Release: Task task-1" });
    expect(release).toBeDisabled();
    expect(release).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(fake.callsTo("holds.release")).toEqual([]);
    services.connection.stop();
  });

  it("held_release_with_the_approver_role_sends_holds_release_and_reads_the_daemons_answer", async () => {
    const { fake, services } = await connected((f) => {
      f.stub("holds.release", { hold: { id: "hold-1", subject_kind: "approval", subject_id: "task-1", until_date: "2026-11-21", state: "released" } });
    });
    view(services);
    await userEvent.click(await screen.findByRole("tab", { name: /^Held/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Release: Task task-1" }));
    await waitFor(() => expect(status()).toHaveTextContent("Released: Task task-1. It is back in the Actionable tab."));
    expect(status()).not.toHaveTextContent("Fixture data");
    expect(fake.callsTo("holds.release")[0]!.params).toMatchObject({ hold_id: "hold-1" });
    services.connection.stop();
  });

  it("held_refused_release_says_nothing_was_changed_and_keeps_the_row", async () => {
    const { fake, services } = await connected(() => undefined);
    fake.failNext("holds.release", { code: -32010, message: "not found" });
    view(services);
    await userEvent.click(await screen.findByRole("tab", { name: /^Held/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Release: Task task-1" }));
    await waitFor(() => expect(status()).toHaveTextContent("That hold has already ended. Nothing was changed."));
    expect(within(panel()).getByRole("button", { name: "Release: Task task-1" })).toBeTruthy();
    services.connection.stop();
  });

  it("held_tab_is_absent_and_silent_when_the_daemon_has_no_holds_capability", async () => {
    const { fake, services } = await connected((f) => {
      f.unknownCapabilities = ["holds"];
    });
    view(services);
    await screen.findByRole("tab", { name: /^Actionable/ });
    expect(screen.queryByRole("tab", { name: /^Held/ })).toBeNull();
    expect(screen.queryByTestId("holds-status")).toBeNull();
    expect(screen.queryByText(/Held decisions are not available/)).toBeNull();
    expect(fake.callsTo("holds.list")).toEqual([]);
    services.connection.stop();
  });
});
