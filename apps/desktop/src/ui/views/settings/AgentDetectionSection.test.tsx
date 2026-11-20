import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { createFixtureSources } from "../../../sources/fixtures";
import type { AgentCapabilitiesSource, Sources } from "../../../sources/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { AgentDetectionSection } from "./AgentDetectionSection";

const fixture = createFixtureSources();
const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

function page(services: Services) {
  return render(
    <ServicesProvider services={services}>
      <AgentDetectionSection />
    </ServicesProvider>,
  );
}

describe("Agent detection section (Settings)", () => {
  it("labels D1 and D2 as placeholder data in fixture mode", async () => {
    page(createServices({ sources: fixture }));
    expect(await screen.findByTestId("badge-D1")).toHaveTextContent("PLACEHOLDER DATA");
    expect(screen.getByTestId("badge-D2")).toHaveTextContent("PLACEHOLDER DATA");
    expect(screen.getByText(/Launch available: no\./)).toBeInTheDocument();
    expect(screen.getByText(/Codex \(preview\): installed/)).toBeInTheDocument();
  });

  it.each([
    ["loading", "Loading placeholder data…"],
    ["empty", "No agents reported."],
    ["unavailable", /Unavailable: Agent discovery is not available/],
    ["error", /Placeholder source error: Fixture discovery failed/],
  ] as const)("renders the D1 %s state", async (scenario, text) => {
    const sources: Sources = { ...fixture, agents: createFixtureSources({ scenario }).agents };
    page(createServices({ sources }));
    const section = screen.getByRole("region", { name: "Detected agents (placeholder)" });
    expect(await within(section).findByText(text)).toBeInTheDocument();
  });

  it("swaps in a daemon-provenance adapter with no component change", async () => {
    const agents: AgentCapabilitiesSource = {
      provenance: { kind: "daemon", id: "D1" },
      listAgents: () =>
        Promise.resolve({
          state: "ready",
          data: [{ id: "real-1", name: "Real Agent", availability: "installed", features: [], discoveredAt: "now" }],
        }),
    };
    page(createServices({ sources: { ...fixture, agents } }));
    const section = screen.getByRole("region", { name: "Detected agents" });
    expect(await within(section).findByText(/Real Agent: installed/)).toBeInTheDocument();
    expect(within(section).getByTestId("badge-D1")).toHaveTextContent("DAEMON (D1)");
    expect(within(section).getByTestId("badge-D1")).not.toHaveTextContent("PLACEHOLDER");
  });

  it("reads the daemon's agent detection without placeholder or simulated wording", async () => {
    const fake = new FakeDaemon();
    fake.stub("agents.capabilities", [{ agent: "codex", installed: true, launchable: true, capabilities: null }]);
    const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
    stops.push(() => {
      services.connection.stop();
      services.data.dispose();
    });
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    page(services);
    expect(await screen.findByText(/Codex: installed/)).toBeInTheDocument();
    const section = within(screen.getByRole("region", { name: "Agent detection" }));
    expect(section.queryByText(/placeholder/i)).not.toBeInTheDocument();
    expect(section.queryByText(/simulated/i)).not.toBeInTheDocument();
  });
});
