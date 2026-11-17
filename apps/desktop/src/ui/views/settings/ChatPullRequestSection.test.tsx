import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { SettingsWire } from "../../../daemon/types";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { SettingsView } from "./SettingsView";

const LABEL = "Let the orchestrator propose tasks that end in a pull request. You still approve each task.";
const SAVE = "Save pull request setting";

const BASE: SettingsWire = {
  load: { cpu_percent: 80, memory_percent: 85, disk_percent: 90 },
  gate: { state: "open", hold: null, meters: [] },
  feed: "sampler",
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

function page(services: Services) {
  return render(
    <ServicesProvider services={services}>
      <SettingsView />
    </ServicesProvider>,
  );
}

async function connected(wire: SettingsWire, approver: boolean | null = true) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  let current = wire;
  fake.stub("settings.get", () => current);
  fake.stub("settings.set", (params: Record<string, unknown>) => {
    if (typeof params.chat_pr_delivery === "boolean") current = { ...current, chat_pr_delivery: params.chat_pr_delivery };
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

describe("Pull requests from chat section", () => {
  it("chat_pr_section_shows_one_unchecked_checkbox_with_the_consequence_and_a_disabled_save", async () => {
    const { services } = await connected({ ...BASE, chat_pr_delivery: false });
    page(services);
    expect(await screen.findByRole("heading", { level: 2, name: "Pull requests from chat" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: LABEL })).not.toBeChecked();
    expect(screen.getByText(/When this is off, chat can only propose a report or a review/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: SAVE })).toBeDisabled();
    expect(screen.getByRole("button", { name: SAVE })).toHaveAccessibleDescription("Change the setting to save.");
  });

  it("chat_pr_section_is_hidden_for_a_daemon_that_does_not_send_the_setting", async () => {
    const { services } = await connected(BASE);
    page(services);
    await screen.findByRole("heading", { level: 2, name: "Machine load limits" });
    await screen.findByLabelText("CPU limit (percent)");
    expect(screen.queryByRole("heading", { name: "Pull requests from chat" })).not.toBeInTheDocument();
  });

  it("chat_pr_section_save_sends_only_the_boolean_through_settings_set_and_announces_it", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected({ ...BASE, chat_pr_delivery: false });
    page(services);
    await user.click(await screen.findByRole("checkbox", { name: LABEL }));
    await user.click(screen.getByRole("button", { name: SAVE }));
    await waitFor(() => expect(screen.getByRole("status", { name: "Save pull request setting result" })).toHaveTextContent("Saved."));
    const calls = fake.callsTo("settings.set");
    expect(calls).toHaveLength(1);
    expect(calls[0]!.params).toEqual({ idempotency_key: expect.any(String), chat_pr_delivery: true });
    expect(screen.getByRole("checkbox", { name: LABEL })).toBeChecked();
  });

  it("chat_pr_section_locks_the_checkbox_and_save_with_the_approver_notice_for_a_non_approver", async () => {
    const { services } = await connected({ ...BASE, chat_pr_delivery: false }, false);
    page(services);
    const box = await screen.findByRole("checkbox", { name: LABEL });
    expect(box).toBeDisabled();
    expect(box).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(screen.getByRole("button", { name: SAVE })).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
  });

  it("chat_pr_section_fixture_save_says_nothing_was_saved", async () => {
    const user = userEvent.setup();
    page(createServices({ surfaces: createFixtureSurfaces() }));
    await user.click(await screen.findByRole("checkbox", { name: LABEL }));
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText("Fixture data: nothing was saved.")).toBeInTheDocument();
  });
});
