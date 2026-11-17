import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { UsageTokenWarningsWire } from "../../daemon/usageWire";
import { createFixtureSurfaces } from "../../sources/surfaces/fixtures";
import type { Provenance, UsageSource, UsageSummary } from "../../sources/surfaces/types";
import { createServices } from "../createServices";
import { ServicesProvider } from "../services";
import { UsagePill } from "./UsagePill";

const provenance: Provenance = { kind: "daemon", id: "D12", methods: ["usage.summary"] };
const observed = { observedAt: "2026-11-16T06:00:00.000Z", ageMs: 0 };

function summary(tokenWarnings: UsageTokenWarningsWire): UsageSummary {
  return {
    windowStart: "2026-11-16T06:00:00.000Z",
    timezone: "America/Boise",
    currency: "USD",
    costCoverage: { status: "unknown", sessions: 1, sessionsWithCost: 0 },
    reported: { status: "unknown", reason: "No provider reported a cost in this window." },
    estimated: { status: "unknown", reason: "No estimate was recorded." },
    budget: { status: "unsupported", reason: "The daemon records no budget." },
    coverage: [],
    tokenWarnings,
  };
}

function renderPill(tokenWarnings: UsageTokenWarningsWire) {
  const usage: UsageSource = { provenance, usage: async () => ({ status: "ok", data: summary(tokenWarnings), provenance, ...observed }) };
  const services = createServices({ surfaces: { ...createFixtureSurfaces(), usage } });
  render(
    <ServicesProvider services={services}>
      <UsagePill />
    </ServicesProvider>,
  );
}

const rule = (provider: string, metric: "output_tokens" | "input_tokens", limit: string, observedCount: string | null, exceeded: boolean | null) => ({
  provider,
  metric,
  limit,
  observed: observedCount,
  exceeded,
});

describe("usage pill daily token warning badge", () => {
  it("renders a badge naming the harness and the token category when a threshold is exceeded", async () => {
    renderPill({ schema_version: 1, state: "ok", rules: [rule("codex", "output_tokens", "20", "21", true)] });
    const badge = await screen.findByTestId("top-token-warning");
    expect(badge).toHaveTextContent("Token threshold exceeded");
    expect(badge.getAttribute("title")).toMatch(/output tokens \(tokens, not USD\)/);
    await userEvent.setup().click(badge);
    expect(await screen.findByLabelText("Daily token warnings")).toHaveTextContent("threshold exceeded, 21 output tokens recorded; daily output tokens warning threshold 20.");
    expect(screen.getByLabelText("Daily token warnings")).toHaveTextContent("do not stop agents");
  });

  it("shows no badge when the threshold is equalled, usage is unknown, or warnings are off", async () => {
    renderPill({
      schema_version: 1,
      state: "ok",
      rules: [rule("codex", "output_tokens", "20", "20", false), rule("claude-code", "input_tokens", "10", null, null)],
    });
    await waitFor(() => expect(screen.getByTestId("top-usage-value")).toBeInTheDocument());
    expect(screen.queryByTestId("top-token-warning")).toBeNull();
    expect(screen.getByLabelText("Daily token warnings")).toHaveTextContent("count unavailable");
  });

  it("shows no badge when warnings are off", async () => {
    renderPill({ schema_version: 1, state: "off", rules: [] });
    await waitFor(() => expect(screen.getByTestId("top-usage-value")).toBeInTheDocument());
    expect(screen.queryByTestId("top-token-warning")).toBeNull();
  });

  it("shows a visible configuration error badge without hiding usage", async () => {
    renderPill({ schema_version: 1, state: "invalid_configuration", rules: [] });
    const badge = await screen.findByTestId("top-token-warning");
    expect(badge).toHaveTextContent("Token warnings unavailable");
    expect(screen.getByTestId("top-usage-value")).toHaveTextContent("not available");
  });
});
