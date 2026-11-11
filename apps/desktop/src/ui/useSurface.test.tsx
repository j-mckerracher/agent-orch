import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Store } from "../daemon/store";
import type { Provenance } from "../sources/types";
import type { SurfaceResult } from "../sources/surfaces/types";
import { ServicesProvider, useSurface, type Services, type SurfaceRefresh } from "./services";

class StubConnection extends Store<{ generation: number; status: string; history: { reason: string | null } }> {
  constructor() {
    super({ generation: 1, status: "connected", history: { reason: null } });
  }
}

const daemon: Provenance = { kind: "daemon", id: "D12", methods: ["usage.summary"] };
const ok = (data: string): SurfaceResult<string> => ({
  status: "ok",
  data,
  provenance: daemon,
  observation: { observedAtMs: 0, ageMs: 0 },
} as unknown as SurfaceResult<string>);

function Probe({ load, refresh }: { load: () => Promise<SurfaceResult<string>>; refresh?: SurfaceRefresh }) {
  const state = useSurface(load, "probe", daemon, refresh);
  return <p data-testid="out">{state.status === "ok" ? state.data : state.status}</p>;
}

function mountProbe(load: () => Promise<SurfaceResult<string>>, refresh?: SurfaceRefresh) {
  const services = { connection: new StubConnection() } as unknown as Services;
  return render(
    <ServicesProvider services={services}>
      <Probe load={load} refresh={refresh} />
    </ServicesProvider>,
  );
}

const advance = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));

describe("useSurface refresh", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout", "Date"] }));
  afterEach(() => vi.useRealTimers());

  it("loads once and never again without a refresh option", async () => {
    const load = vi.fn(async () => ok("a"));
    mountProbe(load);
    await advance(10 * 60_000);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("polls, keeps the previous answer while reloading, and stops on unmount", async () => {
    let value = "first";
    const load = vi.fn(async () => ok(value));
    const view = mountProbe(load, { pollMs: 1_000 });
    await advance(0);
    expect(screen.getByTestId("out")).toHaveTextContent("first");
    value = "second";
    await advance(1_000);
    expect(load).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId("out")).toHaveTextContent("second");
    view.unmount();
    await advance(5_000);
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("reloads on window focus and when the page becomes visible, and releases the listeners", async () => {
    const load = vi.fn(async () => ok("a"));
    const view = mountProbe(load, { onFocus: true });
    await advance(0);
    act(() => void window.dispatchEvent(new Event("focus")));
    await advance(0);
    expect(load).toHaveBeenCalledTimes(2);
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    await advance(0);
    expect(load).toHaveBeenCalledTimes(3);
    view.unmount();
    act(() => void window.dispatchEvent(new Event("focus")));
    await advance(0);
    expect(load).toHaveBeenCalledTimes(3);
  });

  it("reloads just after each local midnight", async () => {
    vi.setSystemTime(new Date(2026, 0, 10, 23, 59, 0));
    const load = vi.fn(async () => ok("a"));
    const view = mountProbe(load, { atLocalMidnight: true });
    await advance(0);
    await advance(30_000);
    expect(load).toHaveBeenCalledTimes(1);
    await advance(31_000);
    expect(load).toHaveBeenCalledTimes(2);
    await advance(24 * 3_600_000);
    expect(load).toHaveBeenCalledTimes(3);
    view.unmount();
    await advance(48 * 3_600_000);
    expect(load).toHaveBeenCalledTimes(3);
  });

  it("does not let a slow earlier answer overwrite a later one", async () => {
    const pending: Array<(value: SurfaceResult<string>) => void> = [];
    const load = vi.fn(() => new Promise<SurfaceResult<string>>((resolve) => pending.push(resolve)));
    mountProbe(load, { pollMs: 1_000 });
    await advance(1_000);
    expect(pending).toHaveLength(2);
    await act(async () => pending[1]!(ok("newer")));
    await act(async () => pending[0]!(ok("older")));
    expect(screen.getByTestId("out")).toHaveTextContent("newer");
  });
});
