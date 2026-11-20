import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createFixtureSources } from "../../../sources/fixtures";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { SettingsView } from "./SettingsView";

const CODEX = {
  agent: "codex",
  installed: true,
  launchable: true,
  capabilities: {
    streaming: true,
    interactive_input: false,
    structured_output: true,
    cancellation: true,
    session_resumption: true,
    approvals: false,
    model_selection: false,
  },
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

async function settings(fake: FakeDaemon, { live = true } = {}) {
  const services = createServices({
    connector: live ? fake : undefined,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: live ? undefined : createFixtureSources(),
  });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  if (live) {
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
  }
  render(
    <ServicesProvider services={services}>
      <SettingsView />
    </ServicesProvider>,
  );
  return userEvent.setup();
}

describe("Settings > Agents inventory", () => {
  it("lists each adapter with the daemon's exact installed and launchable flags, inside Settings", async () => {
    const fake = new FakeDaemon();
    fake.stub("agents.capabilities", [CODEX, { agent: "pi", installed: true, launchable: false, capabilities: null }]);
    await settings(fake);
    const section = within(await screen.findByTestId("agents-inventory"));
    expect(section.getByRole("heading", { level: 2, name: "Agents" })).toBeInTheDocument();
    const table = await section.findByRole("table", { name: "Adapter inventory" });
    const codex = within(table).getByRole("row", { name: /Codex/ });
    expect(within(codex).getByText("Installed and launchable")).toBeInTheDocument();
    expect(within(codex).getByText("installed: yes; launchable: yes")).toBeInTheDocument();
    const capabilities = within(codex).getByRole("list", { name: "Codex capabilities" });
    for (const flag of ["Streaming: Yes", "Interactive input: No", "Session resumption: Yes", "Model selection: No"]) {
      expect(within(capabilities).getByText(flag)).toBeInTheDocument();
    }
    const pi = within(table).getByRole("row", { name: /Pi/ });
    expect(within(pi).getByText("Installed, not launchable")).toBeInTheDocument();
    expect(within(pi).getAllByText("Unknown")).toHaveLength(2);
    expect(within(pi).getByText("Not reported")).toBeInTheDocument();
    for (const pill of within(table).getAllByText("Unknown")) expect(pill).toHaveClass("settings-flag");
  });

  it("shows a retryable error when the inventory cannot load", async () => {
    const fake = new FakeDaemon();
    fake.stub("agents.capabilities", [CODEX]);
    fake.failNext("agents.capabilities", { code: -32603, message: "probe failed" }, 5);
    const user = await settings(fake);
    const section = within(await screen.findByTestId("agents-inventory"));
    expect(await section.findByRole("alert")).toHaveTextContent("Adapter inventory could not be loaded");
    await user.click(section.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(fake.callsTo("agents.capabilities").length).toBeGreaterThan(1));
  });

  it("says when the daemon reports no adapters, never an idle-worker count", async () => {
    const fake = new FakeDaemon();
    fake.stub("agents.capabilities", []);
    await settings(fake);
    expect(await screen.findByText("No adapters reported")).toBeInTheDocument();
  });

  it("labels the fixture inventory as placeholder data", async () => {
    await settings(new FakeDaemon(), { live: false });
    expect(await within(await screen.findByTestId("agents-inventory")).findByTestId("badge-D1")).toHaveTextContent("PLACEHOLDER DATA (D1)");
  });
});
