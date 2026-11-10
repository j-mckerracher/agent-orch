import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "../../App";
import { createFixtureSurfaces, type SurfaceFixtureOptions } from "../../sources/surfaces/fixtures";
import type { SurfaceResult, SystemMetrics } from "../../sources/surfaces/types";
import { FakeDaemon } from "../../test/fakeDaemon";
import { createServices } from "../createServices";
import { DEFAULT_ROUTE } from "../router";
import { ServicesProvider, type Services, type SurfaceState } from "../services";
import { ResourceStrip, ResourceStripView, formatAge } from "./ResourceStrip";

const NOW = Date.parse("2026-01-01T12:00:00Z");
const fixtureProvenance = { kind: "fixture", id: "D13", fixture: "system-metrics" } as const;
const daemonProvenance = { kind: "daemon", id: "D13", methods: [] } as const;

const created: Services[] = [];
afterEach(() => {
  while (created.length) created.pop()!.connection.stop();
  vi.useRealTimers();
  window.location.hash = "";
});

function fixtureServices(options: SurfaceFixtureOptions = {}): Services {
  const services = createServices({
    surfaces: createFixtureSurfaces({ now: () => NOW, ...options }),
    initialRoute: DEFAULT_ROUTE,
    connection: { backoffMs: [5], requestTimeoutMs: 100 },
  });
  created.push(services);
  return services;
}

function renderStrip(services: Services, defaultOpen = true) {
  return render(
    <ServicesProvider services={services}>
      <ResourceStrip defaultOpen={defaultOpen} now={() => NOW} />
    </ServicesProvider>,
  );
}

const ok = (data: Partial<SystemMetrics>): SurfaceState<SystemMetrics> => {
  const base = { observedAt: new Date(NOW - 4000).toISOString(), ageMs: 4000 };
  const measured = (value: number, unit: string, label: string) => ({
    status: "ok" as const,
    ...base,
    value: { value, unit, kind: "utilization" as const, label },
  });
  return {
    status: "ok",
    provenance: daemonProvenance,
    ...base,
    data: {
      cpu: measured(10, "%", "CPU"),
      memory: measured(20, "%", "RAM"),
      disk: measured(30, "%", "Volume"),
      gpu: { status: "unsupported", reason: "no gpu meter" },
      ...data,
    },
  };
};

describe("ResourceStrip with D13 fixtures", () => {
  it("shows four rows with value, unit, scope and age, labelled PLACEHOLDER DATA", async () => {
    renderStrip(fixtureServices());
    const cpu = await screen.findByRole("meter", { name: "CPU utilization" });
    expect(cpu).toHaveAttribute("aria-valuenow", "44");
    expect(cpu).toHaveAttribute("aria-valuemin", "0");
    expect(cpu).toHaveAttribute("aria-valuemax", "100");
    expect(cpu).toHaveAttribute("aria-valuetext", "44%, scope CPU, observed 2 s ago");

    const disk = screen.getByRole("meter", { name: "Disk capacity used" });
    expect(disk).toHaveAttribute("aria-valuenow", "71");
    expect(disk).toHaveAttribute("aria-valuetext", "71%, scope Data volume, observed 30 s ago");

    expect(screen.getAllByRole("meter")).toHaveLength(2);
    expect(screen.getAllByTestId("badge-D13")[0]).toHaveTextContent("PLACEHOLDER DATA (D13)");
  });

  it("prints a measured value without a total as number and unit, never as a drawn gauge", async () => {
    renderStrip(fixtureServices());
    const memory = await screen.findByRole("group", { name: "Memory capacity used" });
    expect(within(memory).getByText("18.5 GiB")).toBeInTheDocument();
    expect(memory).toHaveTextContent("Scope: Memory in use");
    expect(memory).toHaveTextContent("no total reported, so no scale is drawn");
    expect(within(memory).queryByRole("meter")).toBeNull();
  });

  it("keeps memory a named group (no invented range) that still carries value, unit, scope and age as text", async () => {
    renderStrip(fixtureServices());
    const memory = await screen.findByRole("group", { name: "Memory capacity used" });
    expect(memory).not.toHaveAttribute("role", "meter");
    expect(memory).not.toHaveAttribute("aria-valuenow");
    expect(memory).not.toHaveAttribute("aria-valuemax");
    expect(memory).toHaveTextContent(/18\.5 GiB/);
    expect(memory).toHaveTextContent(/Scope: Memory in use · observed \d+ s ago/);
  });

  it("shows GPU as unsupported text, not as zero or a healthy meter", async () => {
    renderStrip(fixtureServices());
    const gpu = await screen.findByRole("group", { name: "GPU utilization" });
    expect(gpu).toHaveAttribute("data-state", "unsupported");
    expect(gpu).toHaveTextContent("unsupported");
    expect(gpu).toHaveTextContent("No cross-platform GPU meter exists on this platform.");
    expect(gpu).not.toHaveTextContent(/\b0\s?%/);
    expect(screen.queryByRole("meter", { name: /GPU/ })).toBeNull();
  });

  it("summarises the values while collapsed and keeps the badge visible", async () => {
    renderStrip(fixtureServices(), false);
    expect(await screen.findByTestId("resource-strip-compact")).toHaveTextContent(
      "CPU 44% · Memory 18.5 GiB · GPU unsupported · Disk 71%",
    );
    expect(screen.getByTestId("badge-D13")).toBeVisible();
    expect(screen.getByTestId("resource-strip")).not.toHaveAttribute("open");
  });

  it.each([
    ["unavailable", "Fixture scenario: the metrics source is unavailable."],
    ["unsupported", "Fixture scenario: system metrics are not supported."],
    ["unknown", "Fixture scenario: system metrics are unknown."],
  ] as const)("renders the %s scenario as text on every row, with no meter and no number", async (scenario, reason) => {
    renderStrip(fixtureServices({ scenario }));
    const rows = await screen.findAllByText(reason);
    expect(rows).toHaveLength(4);
    expect(screen.queryAllByRole("meter")).toHaveLength(0);
    const groups = screen.getAllByRole("group").filter((group) => group.classList.contains("rs-item"));
    expect(groups).toHaveLength(4);
    for (const group of groups) {
      expect(group).toHaveAttribute("data-state", scenario);
      expect(group).toHaveTextContent(scenario);
      expect(group).not.toHaveTextContent(/\d/);
    }
    expect(screen.getByTestId("badge-D13")).toBeInTheDocument();
  });

  it("distinguishes the three failure states by their text", async () => {
    const texts: string[] = [];
    for (const scenario of ["unavailable", "unsupported", "unknown"] as const) {
      const { unmount } = renderStrip(fixtureServices({ scenario }));
      await screen.findAllByRole("group");
      texts.push(screen.getByTestId("resource-strip-compact").textContent ?? "");
      unmount();
    }
    expect(new Set(texts).size).toBe(3);
  });

  it("says it is loading and shows no value while the source has not answered", async () => {
    renderStrip(fixtureServices({ scenario: "loading" }));
    expect(await screen.findAllByText("Waiting for the metrics source.")).toHaveLength(4);
    expect(screen.queryAllByRole("meter")).toHaveLength(0);
    expect(screen.getByTestId("resource-strip-compact")).toHaveTextContent("CPU loading… · Memory loading…");
  });
});

describe("ResourceStrip in live mode", () => {
  it("reports unsupported from the derived source with no placeholder badge and none of the illustrative numbers", async () => {
    const services = createServices({ connector: new FakeDaemon(), connection: { backoffMs: [5], requestTimeoutMs: 100 } });
    created.push(services);
    renderStrip(services);
    expect(await screen.findAllByText("The daemon reports no host metrics.")).toHaveLength(4);
    expect(screen.queryByTestId("badge-D13")).toBeNull();
    expect(screen.queryAllByRole("meter")).toHaveLength(0);
    const strip = screen.getByTestId("resource-strip");
    expect(strip).not.toHaveTextContent(/PLACEHOLDER|44|18\.5|71/);
  });
});

describe("ResourceStripView", () => {
  const view = (state: SurfaceState<SystemMetrics>, provenance = daemonProvenance) =>
    render(<ResourceStripView state={state} provenance={provenance} nowMs={NOW} defaultOpen />);

  it("draws a meter only for a real percentage and clamps nothing silently", () => {
    view(ok({}));
    expect(screen.getAllByRole("meter").map((meter) => meter.getAttribute("aria-valuenow"))).toEqual(["10", "20", "30"]);
    expect(screen.getByRole("meter", { name: "CPU utilization" })).toHaveAttribute(
      "aria-valuetext",
      "10%, scope CPU, observed 4 s ago",
    );
  });

  it("treats an out-of-range or non-finite percentage as unknown instead of clamping it", () => {
    const bad = { observedAt: new Date(NOW).toISOString(), ageMs: 0 };
    view(
      ok({
        cpu: { status: "ok", ...bad, value: { value: 140, unit: "%", kind: "utilization", label: "CPU" } },
        memory: { status: "ok", ...bad, value: { value: Number.NaN, unit: "%", kind: "capacity", label: "RAM" } },
        disk: { status: "ok", ...bad, value: { value: -3, unit: "%", kind: "capacity", label: "Volume" } },
      }),
    );
    expect(screen.queryAllByRole("meter")).toHaveLength(0);
    for (const name of ["CPU utilization", "Memory capacity used", "Disk capacity used"]) {
      expect(screen.getByRole("group", { name })).toHaveAttribute("data-state", "unknown");
    }
    expect(screen.getAllByText(/outside 0-100%/)).toHaveLength(2);
  });

  it("mixes measured and failed values per meter", () => {
    view(ok({ disk: { status: "unavailable", reason: "volume not mounted" } }));
    expect(screen.getAllByRole("meter")).toHaveLength(2);
    expect(screen.getByRole("group", { name: "Disk capacity used" })).toHaveTextContent("unavailable");
    expect(screen.getByText("volume not mounted")).toBeInTheDocument();
  });

  it("uses the failure result's own provenance for the badge", () => {
    const failed: SurfaceResult<SystemMetrics> = { status: "unsupported", reason: "nope", provenance: fixtureProvenance };
    view(failed, daemonProvenance);
    expect(screen.getByTestId("badge-D13")).toBeInTheDocument();
  });

  it("states that the colour scale is not an alert threshold and that disk means capacity", () => {
    view(ok({}));
    expect(screen.getByText(/not an alert threshold/)).toBeInTheDocument();
    expect(screen.getByText(/Disk is capacity used, not activity/)).toBeInTheDocument();
  });
});

describe("ResourceStrip behaviour", () => {
  it("toggles from the keyboard-focusable summary", async () => {
    const user = userEvent.setup();
    renderStrip(fixtureServices(), false);
    await screen.findByTestId("resource-strip-compact");
    const strip = screen.getByTestId("resource-strip") as HTMLDetailsElement;
    await user.tab();
    expect(strip.querySelector("summary")).toHaveFocus();
    await user.click(screen.getByText("Device resources"));
    expect(strip.open).toBe(true);
    expect(screen.getByRole("meter", { name: "CPU utilization" })).toBeInTheDocument();
  });

  it("keeps ageing the observation after the snapshot, so a stale value does not look current", async () => {
    let clock = NOW;
    const services = fixtureServices();
    render(
      <ServicesProvider services={services}>
        <ResourceStrip defaultOpen now={() => clock} />
      </ServicesProvider>,
    );
    await screen.findByRole("meter", { name: "CPU utilization" });
    expect(screen.getByRole("meter", { name: "CPU utilization" })).toHaveAttribute("aria-valuetext", expect.stringContaining("observed 2 s ago"));
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    // A new `now` identity restarts the interval under fake timers.
    const later = () => clock;
    clock = NOW + 120_000;
    const { unmount } = render(
      <ServicesProvider services={services}>
        <ResourceStrip defaultOpen now={later} />
      </ServicesProvider>,
    );
    await waitFor(() => expect(screen.getAllByRole("meter", { name: "CPU utilization" })).toHaveLength(2));
    await act(async () => {
      vi.advanceTimersByTime(5000);
    });
    const meters = screen.getAllByRole("meter", { name: "CPU utilization" });
    expect(meters[1]).toHaveAttribute("aria-valuetext", expect.stringContaining("observed 2 min ago"));
    unmount();
  });
});

describe("formatAge", () => {
  it.each([
    [0, "0 s ago"],
    [59_000, "59 s ago"],
    [60_000, "1 min ago"],
    [3_600_000, "1 h ago"],
    [172_800_000, "2 d ago"],
  ])("formats %i ms", (age, text) => {
    expect(formatAge(new Date(NOW - age).toISOString(), 0, NOW)).toBe(text);
  });

  it("never reports a negative age and falls back to the reported age for an unusable timestamp", () => {
    expect(formatAge(new Date(NOW + 5000).toISOString(), 0, NOW)).toBe("0 s ago");
    expect(formatAge("not a date", 7000, NOW)).toBe("7 s ago");
  });
});

describe("ResourceStrip in the shell", () => {
  it("renders inside the Device resources region without adding a landmark, in fixture mode", async () => {
    const services = fixtureServices();
    render(<App services={services} shellSlots={{ resources: <ResourceStrip now={() => NOW} /> }} />);
    const region = await screen.findByRole("region", { name: "Device resources" });
    expect(await within(region).findByTestId("resource-strip-compact")).toHaveTextContent("CPU 44%");
    expect(within(region).queryAllByRole("region")).toHaveLength(0);
    expect(within(region).queryAllByRole("navigation")).toHaveLength(0);
    expect(within(region).getByTestId("badge-D13")).toBeInTheDocument();
  });
});
