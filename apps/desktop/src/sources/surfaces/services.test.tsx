import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../../daemon/errors";
import type { DaemonConnector } from "../../daemon/session";
import { FakeDaemon, mkTask } from "../../test/fakeDaemon";
import { createServices } from "../../ui/createServices";
import { ServicesProvider, useSurface, type Services } from "../../ui/services";

const created: Services[] = [];
afterEach(() => {
  while (created.length) created.pop()!.connection.stop();
});

function make(connector?: DaemonConnector): Services {
  const services = createServices({ connector, connection: { backoffMs: [5], requestTimeoutMs: 100 } });
  created.push(services);
  return services;
}
const wrapper = (services: Services) => ({ children }: { children: ReactNode }) => (
  <ServicesProvider services={services}>{children}</ServicesProvider>
);

describe("services.surfaces", () => {
  it("is fixture-backed in explicit demo mode (no connector)", async () => {
    const { surfaces } = make();
    const result = await surfaces.workstreams.workstreams();
    expect(result).toMatchObject({ status: "ok", provenance: { kind: "fixture", id: "D9" } });
  });

  it("is derived when a connector is bound and never serves fixtures when the daemon is unreachable", async () => {
    const failing: DaemonConnector = { open: () => Promise.reject(new DaemonError("disconnected", "no daemon")) };
    const services = make(failing);
    services.connection.start();
    for (const read of [
      () => services.surfaces.workstreams.workstreams(),
      () => services.surfaces.activity.activity(),
      () => services.surfaces.decisions.queue(),
      () => services.surfaces.models.catalog(),
      () => services.surfaces.search.search("auth"),
      () => services.surfaces.usage.usage(),
      () => services.surfaces.metrics.metrics(),
    ]) {
      const result = await read();
      expect(result.status).not.toBe("ok");
      expect(result.provenance.kind).toBe("daemon");
      expect(JSON.stringify(result)).not.toMatch(/fixture/i);
    }
  });

  it("honours an explicit surfaces override", () => {
    const override = make().surfaces;
    const services = createServices({ surfaces: override });
    created.push(services);
    expect(services.surfaces).toBe(override);
  });
});

describe("useSurface", () => {
  it("loads, then re-runs for a daemon surface when the connection comes up", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "/repos/a" }];
    fake.tasks = [mkTask("t1", "/repos/a")];
    const services = make(fake);
    const { result } = renderHook(
      () => useSurface(() => services.surfaces.workstreams.workstreams(), "ws", services.surfaces.workstreams.provenance),
      { wrapper: wrapper(services) },
    );
    await waitFor(() => expect(result.current).toMatchObject({ status: "unavailable" }));
    services.connection.start();
    await waitFor(() => expect(result.current).toMatchObject({ status: "ok", provenance: { kind: "daemon" } }));
  });

  it("turns a rejected load into unavailable with the source provenance", async () => {
    const services = make();
    const provenance = services.surfaces.workstreams.provenance;
    const { result } = renderHook(() => useSurface(() => Promise.reject(new Error("kaput")), "x", provenance), {
      wrapper: wrapper(services),
    });
    await waitFor(() => expect(result.current).toMatchObject({ status: "unavailable", reason: "kaput", provenance }));
  });

  it("is loading while a fixture source never resolves", async () => {
    const services = make();
    const { result } = renderHook(() => useSurface(() => new Promise(() => undefined), "l", services.surfaces.usage.provenance), {
      wrapper: wrapper(services),
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(result.current).toEqual({ status: "loading" });
  });
});
