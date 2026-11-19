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

const LABEL = "Let the orchestrator act on its own";
const SAVE = "Save acting on its own";

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
    if (typeof params.autonomous_actions === "boolean") current = { ...current, autonomous_actions: params.autonomous_actions };
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

describe("Acting on its own section", () => {
  it("autonomous_section_shows_the_switch_on_with_what_still_waits_for_approval", async () => {
    const { services } = await connected({ ...BASE, autonomous_actions: true });
    page(services);
    expect(await screen.findByRole("heading", { level: 2, name: "Acting on its own" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: LABEL })).toBeChecked();
    expect(screen.getByText(/are never done on its own\. When this is off, every action waits for your approval, unless a standing approval covers it\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: SAVE })).toBeEnabled();
  });

  it("autonomous_section_is_hidden_for_a_daemon_that_does_not_send_the_setting", async () => {
    const { services } = await connected(BASE);
    page(services);
    await screen.findByLabelText("CPU limit (percent)");
    expect(screen.queryByRole("heading", { name: "Acting on its own" })).not.toBeInTheDocument();
  });

  it("autonomous_section_save_sends_only_the_boolean_through_settings_set", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected({ ...BASE, autonomous_actions: true });
    page(services);
    await user.click(await screen.findByRole("switch", { name: LABEL }));
    await user.click(screen.getByRole("button", { name: SAVE }));
    await waitFor(() => expect(screen.getByRole("status", { name: `${SAVE} result` })).toHaveTextContent("Saved."));
    const calls = fake.callsTo("settings.set");
    expect(calls).toHaveLength(1);
    expect(calls[0]!.params).toEqual({ idempotency_key: expect.any(String), autonomous_actions: false });
    expect(screen.getByRole("switch", { name: LABEL })).not.toBeChecked();
  });

  it("autonomous_section_locks_the_switch_for_a_non_approver", async () => {
    const { services } = await connected({ ...BASE, autonomous_actions: true }, false);
    page(services);
    expect(await screen.findByRole("switch", { name: LABEL })).toBeDisabled();
    expect(screen.getByRole("button", { name: SAVE })).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
  });

  it("autonomous_section_fixture_is_on_and_a_save_says_nothing_was_saved", async () => {
    const user = userEvent.setup();
    page(createServices({ surfaces: createFixtureSurfaces() }));
    const box = await screen.findByRole("switch", { name: LABEL });
    expect(box).toBeChecked();
    await user.click(box);
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText("Fixture data: nothing was saved.")).toBeInTheDocument();
  });
});
