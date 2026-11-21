import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE, DaemonError } from "../../../daemon/errors";
import type { MonitoringState } from "../../../sources/surfaces/types";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { MonitoringChip } from "./MonitoringChip";
import type { MonitoringModel } from "./useMonitoring";

afterEach(cleanup);

const STATE: MonitoringState = { mode: "on", paused: [], unseen: [] };

function model(overrides: Partial<MonitoringModel> = {}): MonitoringModel {
  return { supported: true, state: STATE, pending: null, error: null, note: null, setMode: vi.fn(async () => true), ack: vi.fn(), ...overrides };
}

function mount(value: MonitoringModel, canChange = true) {
  const user = userEvent.setup();
  render(
    <ServicesProvider services={createServices()}>
      <MonitoringChip model={value} target={{ kind: "global" }} canChange={canChange} />
    </ServicesProvider>,
  );
  return { user, button: () => screen.getByRole("button", { name: /Monitoring:/ }) };
}

describe("MonitoringChip", () => {
  it.each([
    ["on", "on"],
    ["quiet", "quiet"],
    ["away", "away"],
    ["off", "off"],
    ["from-the-future", "unknown"],
  ])("monitoring_chip_dot_tone_for_mode_%s_is_%s", (mode, tone) => {
    const { button } = mount(model({ state: { ...STATE, mode } }));
    expect(button().querySelector(".ws-monitoring-dot")).toHaveAttribute("data-tone", tone);
  });

  it("monitoring_chip_dot_turns_paused_when_a_limit_pauses_this_target", () => {
    const paused = [{ target: { kind: "global" as const }, reason: "hourly", until: null }];
    const { button } = mount(model({ state: { ...STATE, paused } }));
    expect(button().querySelector(".ws-monitoring-dot")).toHaveAttribute("data-tone", "paused");
  });

  it("monitoring_chip_renders_the_mode_and_opens_a_menu_of_four_radios_with_one_checked", async () => {
    const { user, button } = mount(model());
    expect(button()).toHaveTextContent("Monitoring: on");
    expect(button()).toHaveAttribute("aria-haspopup", "menu");
    await user.click(button());
    expect(button()).toHaveAttribute("aria-expanded", "true");
    const items = screen.getAllByRole("menuitemradio");
    expect(items).toHaveLength(4);
    expect(items.map((item) => item.getAttribute("aria-checked"))).toEqual(["true", "false", "false", "false"]);
    expect(screen.getByRole("link", { name: "Checks and limits are on the Settings page" })).toHaveAttribute("href", "#/settings");
  });

  it("monitoring_chip_arrow_keys_move_enter_picks_and_escape_returns_focus_to_the_chip", async () => {
    const value = model();
    const { user, button } = mount(value);
    await user.click(button());
    const items = screen.getAllByRole("menuitemradio");
    await waitFor(() => expect(items[0]).toHaveFocus());
    await user.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(value.setMode).toHaveBeenCalledTimes(1);
    expect(value.setMode).toHaveBeenCalledWith("quiet");
    await user.click(button());
    await user.keyboard("{Escape}");
    expect(button()).toHaveFocus();
    expect(button()).toHaveAttribute("aria-expanded", "false");
  });

  it("monitoring_chip_items_are_aria_disabled_while_a_change_is_in_flight", async () => {
    const value = model({ pending: "quiet" });
    const { user, button } = mount(value);
    await user.click(button());
    for (const item of screen.getAllByRole("menuitemradio")) expect(item).toHaveAttribute("aria-disabled", "true");
    await user.click(screen.getAllByRole("menuitemradio")[2]!);
    expect(value.setMode).not.toHaveBeenCalled();
  });

  it("monitoring_chip_failure_keeps_the_old_value_and_shows_the_error", () => {
    mount(model({ error: new DaemonError("timeout", "the daemon did not answer") }));
    expect(screen.getByRole("button", { name: /Monitoring:/ })).toHaveTextContent("Monitoring: on");
    expect(screen.getByText(/Changing the monitoring mode/)).toBeInTheDocument();
  });

  it("monitoring_chip_paused_text_is_a_status_with_a_link_to_the_settings_page", () => {
    const paused: MonitoringState = { ...STATE, paused: [{ target: { kind: "global" }, reason: "hourly", until: null }] };
    mount(model({ state: paused }));
    const status = screen.getByText(/Monitoring paused: limit reached/).closest('[role="status"]')!;
    expect(status).toBeInTheDocument();
    expect(status.querySelector("a")).toHaveAttribute("href", "#/settings");
    expect(status).toHaveTextContent("Change limits");
  });

  it("monitoring_chip_without_approver_disables_the_items_and_links_the_notice", async () => {
    const value = model();
    const { user, button } = mount(value, false);
    expect(button()).toHaveTextContent("Monitoring: on");
    await user.click(button());
    const notice = screen.getByText(APPROVER_REQUIRED_NOTICE);
    for (const item of screen.getAllByRole("menuitemradio")) {
      expect(item).toHaveAttribute("aria-disabled", "true");
      expect(item.getAttribute("aria-describedby")).toBe(notice.id);
    }
    await user.click(screen.getAllByRole("menuitemradio")[1]!);
    expect(value.setMode).not.toHaveBeenCalled();
  });

  it("monitoring_chip_is_not_rendered_without_the_capability", () => {
    mount(model({ supported: false }));
    expect(screen.queryByRole("button", { name: /Monitoring:/ })).not.toBeInTheDocument();
  });

  it("brief_away_item_explains_itself_and_sets_away_for_an_approver", async () => {
    const value = model();
    const { user, button } = mount(value);
    await user.click(button());
    const away = screen.getByRole("menuitemradio", { name: /^Away: replies stay quiet, standing approvals keep working, and you get a summary when you return\. It adds no permissions\.$/ });
    await user.click(away);
    expect(value.setMode).toHaveBeenCalledTimes(1);
    expect(value.setMode).toHaveBeenCalledWith("away");
  });

  it("brief_away_item_is_locked_for_a_non_approver_with_the_notice", async () => {
    const value = model({ state: { mode: "away", paused: [], unseen: [] } });
    const { user, button } = mount(value, false);
    expect(button()).toHaveTextContent("Monitoring: away");
    await user.click(button());
    const notice = screen.getByText(APPROVER_REQUIRED_NOTICE);
    const away = screen.getByRole("menuitemradio", { name: /^Away:/ });
    expect(away).toHaveAttribute("aria-checked", "true");
    expect(away).toHaveAttribute("aria-disabled", "true");
    expect(away.getAttribute("aria-describedby")).toBe(notice.id);
    await user.click(screen.getByRole("menuitemradio", { name: /^Quiet:/ }));
    expect(value.setMode).not.toHaveBeenCalled();
  });

  it("monitoring_chip_shows_the_fixture_note_after_a_change", () => {
    mount(model({ note: "Fixture data: nothing was saved" }));
    expect(screen.getByText("Fixture data: nothing was saved")).toBeInTheDocument();
  });

  it("monitoring_chip_says_a_change_is_being_saved_in_a_status_region", () => {
    mount(model({ pending: "quiet" }));
    expect(screen.getByText("Changing the monitoring mode…")).toHaveAttribute("role", "status");
  });

  it("monitoring_chip_says_a_saved_change_in_a_status_region", () => {
    mount(model({ note: "Monitoring set to quiet." }));
    expect(screen.getByText("Monitoring set to quiet.")).toHaveAttribute("role", "status");
  });
});
