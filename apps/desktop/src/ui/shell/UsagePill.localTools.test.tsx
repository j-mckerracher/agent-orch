import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { createFixtureSurfaces, type LocalToolsFixtureState } from "../../sources/surfaces/fixtures";
import type { LocalToolsSource, SurfaceResult, LocalToolsSnapshot } from "../../sources/surfaces/types";
import { createServices } from "../createServices";
import { ServicesProvider } from "../services";
import { OPT_IN_LABEL } from "./localToolsCopy";
import { UsagePill } from "./UsagePill";

const fixtureSource = (state: LocalToolsFixtureState): LocalToolsSource => createFixtureSurfaces({ localTools: state }).localTools;

function renderPill(localTools: LocalToolsSource) {
  const services = createServices({ surfaces: { ...createFixtureSurfaces(), localTools } });
  const user = userEvent.setup();
  render(
    <ServicesProvider services={services}>
      <UsagePill />
    </ServicesProvider>,
  );
  return { user };
}

/** Opens the usage popover the way a reader does, by its summary. */
async function openPopover(user: ReturnType<typeof userEvent.setup>) {
  const group = screen.getByRole("group", { name: "Daily usage" });
  await user.click(group.querySelector("summary") as HTMLElement);
  return group.querySelector("details") as HTMLDetailsElement;
}

/** A source whose opt-in change waits until the test releases it, to see what shows while a change is saving. */
function gated(base: LocalToolsSource) {
  let open!: () => void;
  const gate = new Promise<void>((resolve) => (open = resolve));
  const source: LocalToolsSource = {
    ...base,
    setEnabled: (enabled) => gate.then(() => base.setEnabled(enabled)),
  };
  return { source, release: () => open() };
}

/** A source whose ok results are changed by `change`, for states the fixtures do not hold. */
function withSnapshot(base: LocalToolsSource, change: (snapshot: LocalToolsSnapshot) => LocalToolsSnapshot): LocalToolsSource {
  const map = async (result: Promise<SurfaceResult<LocalToolsSnapshot>>): Promise<SurfaceResult<LocalToolsSnapshot>> => {
    const value = await result;
    return value.status === "ok" ? { ...value, data: change(value.data) } : value;
  };
  return {
    provenance: base.provenance,
    local: () => map(base.local()),
    setEnabled: (enabled) => map(base.setEnabled(enabled)),
    refresh: () => map(base.refresh()),
  };
}

describe("usage pill: other AI tools", () => {
  it("offers the opt-in unchecked by default, with no chip and no figures", async () => {
    const { user } = renderPill(fixtureSource("off"));
    await openPopover(user);
    const optIn = await screen.findByRole("checkbox", { name: OPT_IN_LABEL });
    expect(optIn).not.toBeChecked();
    expect(screen.queryByTestId("top-usage-chip")).toBeNull();
    expect(screen.getByTestId("top-usage-tools")).not.toHaveTextContent("Estimated from local session files");
    expect(screen.queryByTestId("top-usage-tools-claude-code")).toBeNull();
  });

  it("calls the source when opted in, and shows the estimate as its own labelled chip", async () => {
    const { user } = renderPill(fixtureSource("off"));
    await openPopover(user);
    await user.click(await screen.findByRole("checkbox", { name: OPT_IN_LABEL }));
    const chip = await screen.findByTestId("top-usage-chip");
    expect(chip).toHaveTextContent(/^Other tools ≈ \$0\.0[56] est\. \(partial\)$/);
    const tools = screen.getByTestId("top-usage-tools");
    expect(tools).toHaveTextContent("Estimated from local session files");
    expect(tools).toHaveTextContent("List-price estimate, not a bill.");
    expect(tools).toHaveTextContent("1 model with tokens but no price: 4000 tokens not priced. Total is a lower bound.");
    expect(tools.textContent).not.toMatch(/\$0(?:\.0+)?(?![\d.])/);
    expect(screen.getByTestId("top-usage-value")).toHaveTextContent("$26.00");
    expect(screen.getByTestId("top-usage-value")).not.toHaveTextContent("Other tools");
  });

  it("hides every figure the moment it is turned off, before the source answers", async () => {
    const control = gated(fixtureSource("ok"));
    const { user } = renderPill(control.source);
    await openPopover(user);
    const optIn = await screen.findByRole("checkbox", { name: OPT_IN_LABEL });
    expect(optIn).toBeChecked();
    expect(screen.getByTestId("top-usage-tools-claude-code")).toBeInTheDocument();

    await user.click(optIn);
    expect(screen.getByRole("checkbox", { name: OPT_IN_LABEL })).not.toBeChecked();
    expect(screen.queryByTestId("top-usage-tools-claude-code")).toBeNull();
    expect(screen.queryByTestId("top-usage-chip")).toBeNull();

    control.release();
    await waitFor(() => expect(screen.getByRole("checkbox", { name: OPT_IN_LABEL })).not.toBeChecked());
    expect(screen.queryByTestId("top-usage-tools-claude-code")).toBeNull();
  });

  it("says in words when a change did not take, and shows the daemon's current answer", async () => {
    const base = fixtureSource("off");
    const refused: LocalToolsSource = {
      ...base,
      setEnabled: async () => ({ status: "unavailable", reason: "the daemon refused", provenance: base.provenance, observedAt: new Date().toISOString(), ageMs: 0 }),
    };
    const { user } = renderPill(refused);
    await openPopover(user);
    await user.click(await screen.findByRole("checkbox", { name: OPT_IN_LABEL }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The setting did not turn on: the daemon refused.");
    expect(screen.getByRole("checkbox", { name: OPT_IN_LABEL })).not.toBeChecked();
  });

  it("disables refresh while a refresh is running", async () => {
    const { user } = renderPill(withSnapshot(fixtureSource("ok"), (snapshot) => ({ ...snapshot, refreshing: true })));
    await openPopover(user);
    expect(await screen.findByRole("button", { name: "Refreshing…" })).toBeDisabled();
  });

  it("shows no control on a daemon without the method, and says so", async () => {
    const base = fixtureSource("off");
    const absent: LocalToolsSource = {
      ...base,
      local: async () => ({ status: "unsupported", reason: "usage.localTools is not offered", provenance: base.provenance, observedAt: new Date().toISOString(), ageMs: 0 }),
    };
    const { user } = renderPill(absent);
    await openPopover(user);
    expect(await screen.findByText("Other AI tools are not available on this daemon.")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: OPT_IN_LABEL })).toBeNull();
    expect(screen.queryByTestId("top-usage-chip")).toBeNull();
  });

  it("shows checking while the first read has not finished", async () => {
    const { user } = renderPill(fixtureSource("not_started"));
    await openPopover(user);
    expect(await screen.findByTestId("top-usage-chip")).toHaveTextContent("Other tools: checking");
    expect(screen.getByTestId("top-usage-tools-reader")).toHaveTextContent("Checking local tools now.");
  });

  it("names the pinned version and the fix for an unsupported codeburn, and says not available", async () => {
    const { user } = renderPill(fixtureSource("unsupported_version"));
    await openPopover(user);
    expect(await screen.findByTestId("top-usage-chip")).toHaveTextContent("Other tools: not available");
    expect(screen.getByTestId("top-usage-tools-reader")).toHaveTextContent(
      "unsupported codeburn 0.9.10; MCAO supports 0.9.28. Development builds: point AGENT_ORCH_CODEBURN_BIN at codeburn 0.9.28 or remove the other install from PATH.",
    );
    expect(screen.getByTestId("top-usage-tools-source")).toHaveTextContent("Using the codeburn found on PATH (development builds only).");
    expect(screen.queryByTestId("top-usage-tools-claude-code")).toBeNull();
    expect(screen.getByTestId("top-usage-tools").textContent).not.toMatch(/npm/i);
  });

  it("says a release build has no reader bundled, with no fix and no npm", async () => {
    const { user } = renderPill(fixtureSource("not_bundled"));
    await openPopover(user);
    expect(await screen.findByTestId("top-usage-chip")).toHaveTextContent("Other tools: not available");
    expect(screen.getByTestId("top-usage-tools-reader")).toHaveTextContent("not available: the usage reader is not bundled in this build.");
    expect(screen.queryByTestId("top-usage-tools-source")).toBeNull();
    expect(screen.getByTestId("top-usage-tools").textContent).not.toMatch(/npm/i);
  });

  it("opens the popover from its chip", async () => {
    const { user } = renderPill(fixtureSource("ok_unpriced"));
    const chip = await screen.findByTestId("top-usage-chip");
    const details = screen.getByRole("group", { name: "Daily usage" }).querySelector("details") as HTMLDetailsElement;
    expect(details.open).toBe(false);
    await user.click(chip);
    expect(details.open).toBe(true);
  });
});
