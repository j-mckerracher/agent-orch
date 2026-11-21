import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { SettingsWire } from "../../../daemon/types";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { renderSettingsPage } from "../../../test/settingsPage";
import { createServices } from "../../createServices";
import { AUTONOMOUS_LABEL, CHAT_PR_LABEL, MCP_LABEL } from "./AutonomySection";

const BASE: SettingsWire = {
  load: { cpu_percent: 80, memory_percent: 85, disk_percent: 90 },
  gate: { state: "open", hold: null, meters: [] },
  feed: "sampler",
};
const ALL: SettingsWire = { ...BASE, autonomous_actions: true, chat_pr_delivery: false, mcp: { enabled: false } };

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

async function connected(wire: SettingsWire, approver: boolean | null = true) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  let current = wire;
  fake.stub("settings.get", () => current);
  fake.stub("settings.set", (params: Record<string, unknown>) => {
    if (typeof params.autonomous_actions === "boolean") current = { ...current, autonomous_actions: params.autonomous_actions };
    if (typeof params.chat_pr_delivery === "boolean") current = { ...current, chat_pr_delivery: params.chat_pr_delivery };
    const mcp = params.mcp as { enabled?: unknown } | undefined;
    if (typeof mcp?.enabled === "boolean") current = { ...current, mcp: { enabled: mcp.enabled } };
    return current;
  });
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  return { fake, services };
}

const toggle = (name: string) => screen.findByRole("switch", { name });

describe("Autonomy switches", () => {
  it("autonomy_shows_three_labelled_switches_with_a_one_line_hint_and_the_long_text_under_details", async () => {
    const { services } = await connected(ALL);
    renderSettingsPage(services, "autonomy");
    expect(await toggle(AUTONOMOUS_LABEL)).toBeChecked();
    expect(screen.getByRole("switch", { name: CHAT_PR_LABEL })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: MCP_LABEL })).not.toBeChecked();
    expect(screen.getByText("Starts and retries tasks without asking.")).toBeInTheDocument();
    expect(screen.getByText("You still approve each one.")).toBeInTheDocument();
    for (const details of document.querySelectorAll("details")) expect(details).not.toHaveAttribute("open");
    expect(screen.getByText(/are never done on its own\. When this is off, every action waits for your approval/)).toBeInTheDocument();
  });

  it("autonomy_has_no_save_button_of_its_own", async () => {
    const { services } = await connected(ALL);
    renderSettingsPage(services, "autonomy");
    await toggle(AUTONOMOUS_LABEL);
    expect(screen.queryByRole("button", { name: /^Save/ })).not.toBeInTheDocument();
    expect(screen.queryByTestId("settings-savebar")).not.toBeInTheDocument();
  });

  it("autonomy_leaves_out_a_switch_the_daemon_does_not_send", async () => {
    const { services } = await connected({ ...BASE, autonomous_actions: true });
    renderSettingsPage(services, "autonomy");
    await toggle(AUTONOMOUS_LABEL);
    expect(screen.queryByRole("switch", { name: CHAT_PR_LABEL })).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: MCP_LABEL })).not.toBeInTheDocument();
  });

  it.each([
    [AUTONOMOUS_LABEL, { autonomous_actions: false }],
    [CHAT_PR_LABEL, { chat_pr_delivery: true }],
    [MCP_LABEL, { mcp: { enabled: true } }],
  ])("autonomy_saves_%s_the_moment_it_is_clicked, sending only that setting", async (label, sent) => {
    const user = userEvent.setup();
    const { fake, services } = await connected(ALL);
    renderSettingsPage(services, "autonomy");
    const box = await toggle(label);
    const row = box.closest<HTMLElement>(".settings-switch")!;
    const was = (box as HTMLInputElement).checked;
    await user.click(box);
    await waitFor(() => expect(within(row).getByRole("status")).toHaveTextContent("Saved"));
    const calls = fake.callsTo("settings.set");
    expect(calls).toHaveLength(1);
    expect(calls[0]!.params).toEqual({ idempotency_key: expect.any(String), ...sent });
    expect(screen.getByRole("switch", { name: label })).toHaveProperty("checked", !was);
  });

  it("autonomy_puts_a_refused_switch_back_in_an_alert_and_retries", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected(ALL);
    fake.failNext("settings.set", { code: -32602, message: "invalid params" });
    renderSettingsPage(services, "autonomy");
    const row = (await toggle(AUTONOMOUS_LABEL)).closest<HTMLElement>(".settings-switch")!;
    await user.click(within(row).getByRole("switch"));
    const alert = await within(row).findByRole("alert");
    expect(alert).toHaveTextContent("The daemon refused these values.");
    expect(screen.getByRole("switch", { name: AUTONOMOUS_LABEL })).toBeChecked();
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(within(row).getByRole("status")).toHaveTextContent("Saved."));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("switch", { name: AUTONOMOUS_LABEL })).not.toBeChecked();
  });

  it("autonomy_shows_saving_and_refuses_a_second_flip_with_a_reason_while_the_first_is_saved", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected(ALL);
    fake.delayNext("settings.set", 200);
    renderSettingsPage(services, "autonomy");
    const box = await toggle(AUTONOMOUS_LABEL);
    const row = box.closest<HTMLElement>(".settings-switch")!;
    await user.click(box);
    expect(box).toHaveAttribute("aria-busy", "true");
    expect(within(row).getByText("Saving…")).toBeInTheDocument();
    await user.click(box);
    expect(within(row).getByText("Still saving the last change. Change it again when this finishes.")).toBeInTheDocument();
    expect(fake.callsTo("settings.set")).toHaveLength(1);
    await waitFor(() => expect(within(row).getByRole("status")).toHaveTextContent("Saved."));
    expect(box).not.toHaveAttribute("aria-busy");
    expect(box).not.toBeChecked();
  });

  it("autonomy_locks_the_switches_for_a_non_approver_and_says_why", async () => {
    const { services } = await connected(ALL, false);
    renderSettingsPage(services, "autonomy");
    for (const label of [AUTONOMOUS_LABEL, CHAT_PR_LABEL, MCP_LABEL]) expect(await toggle(label)).toBeDisabled();
    expect(screen.getAllByText(APPROVER_REQUIRED_NOTICE).length).toBeGreaterThan(0);
  });

  it("autonomy_fixture_switch_says_nothing_was_saved_and_stays_put", async () => {
    const user = userEvent.setup();
    renderSettingsPage(createServices({ surfaces: createFixtureSurfaces() }), "autonomy");
    const box = await toggle(AUTONOMOUS_LABEL);
    expect(box).toBeChecked();
    await user.click(box);
    const row = box.closest<HTMLElement>(".settings-switch")!;
    await waitFor(() => expect(within(row).getByText("Fixture data: nothing was saved.")).toBeInTheDocument());
    expect(screen.getByRole("switch", { name: AUTONOMOUS_LABEL })).toBeChecked();
  });
});
