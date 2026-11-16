import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { createFixtureSurfaces } from "../../sources/surfaces/fixtures";
import type { Provenance, SurfaceResult, UsageSource, UsageSummary } from "../../sources/surfaces/types";
import { createServices } from "../createServices";
import { ServicesProvider } from "../services";
import { UsagePill } from "./UsagePill";

const provenance: Provenance = { kind: "daemon", id: "D12", methods: ["usage.summary"] };
const observed = { observedAt: "2026-11-16T06:00:00.000Z", ageMs: 0 };

/** A live-shaped summary: nothing reported, nothing estimated, with the given session count. */
function summary(sessions: number, overrides: Partial<UsageSummary> = {}): UsageSummary {
  return {
    windowStart: "2026-11-16T06:00:00.000Z",
    timezone: "America/Boise",
    currency: "USD",
    costCoverage: { status: "unknown", sessions, sessionsWithCost: 0 },
    reported: { status: "unknown", reason: "No provider reported a cost in this window." },
    estimated: { status: "unknown", reason: "No estimate was recorded." },
    budget: { status: "unsupported", reason: "The daemon records no budget." },
    coverage: [],
    ...overrides,
  };
}

const answered = (data: UsageSummary): SurfaceResult<UsageSummary> => ({ status: "ok", data, provenance, ...observed });
const unanswered = (reason: string): SurfaceResult<UsageSummary> => ({ status: "unavailable", reason, provenance, ...observed });

function usageWith(result: SurfaceResult<UsageSummary>): UsageSource {
  return { provenance, usage: async () => result };
}

function renderPill(usage: UsageSource) {
  const services = createServices({ surfaces: { ...createFixtureSurfaces(), usage } });
  render(
    <ServicesProvider services={services}>
      <UsagePill />
    </ServicesProvider>,
  );
}

describe("usage pill headline", () => {
  it("says nothing is recorded yet, not not-available, when no session has started", async () => {
    renderPill(usageWith(answered(summary(0))));
    await waitFor(() => expect(screen.getByTestId("top-usage-value")).toHaveTextContent("nothing recorded yet"));
    expect(screen.getByTestId("top-usage-value")).not.toHaveTextContent("not available");
    expect(screen.getByTestId("top-usage-value").textContent).not.toMatch(/\$|\d/);
  });

  it("keeps not available when sessions started but none carried a cost, with the reason in the popover", async () => {
    renderPill(usageWith(answered(summary(3))));
    await waitFor(() => expect(screen.getByTestId("top-usage-value")).toHaveTextContent("not available"));
    expect(screen.getByTestId("top-usage-value").textContent).not.toMatch(/\$|\d/);
  });

  it("shows the reported amount when a cost was reported", async () => {
    renderPill(
      usageWith(
        answered(
          summary(2, {
            costCoverage: { status: "complete", sessions: 2, sessionsWithCost: 2 },
            reported: { status: "ok", value: 26, ...observed },
          }),
        ),
      ),
    );
    await waitFor(() => expect(screen.getByTestId("top-usage-value")).toHaveTextContent("$26.00"));
  });

  it("keeps not available when the reader or daemon could not answer at all", async () => {
    renderPill(usageWith(unanswered("daemon down")));
    await waitFor(() => expect(screen.getByTestId("top-usage-value")).toHaveTextContent("not available"));
  });
});

describe("other AI tools opt-in control", () => {
  it("names the release fix beside a control, with no figures, when this build has no reader", async () => {
    const localTools = createFixtureSurfaces({ localTools: "not_bundled" }).localTools;
    const services = createServices({ surfaces: { ...createFixtureSurfaces(), localTools } });
    const user = userEvent.setup();
    render(
      <ServicesProvider services={services}>
        <UsagePill />
      </ServicesProvider>,
    );
    await user.click(screen.getByRole("group", { name: "Daily usage" }).querySelector("summary") as HTMLElement);
    expect(await screen.findByTestId("top-usage-tools-reader")).toHaveTextContent("Install a release build of MCAO");
    expect(screen.queryByTestId("top-usage-tools-source")).toBeNull();
    expect(screen.queryByTestId("top-usage-tools-claude-code")).toBeNull();
  });
});
