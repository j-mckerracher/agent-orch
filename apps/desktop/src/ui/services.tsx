import { createContext, useContext, useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import type { AppData } from "../daemon/appData";
import type { ConnectionManager, ConnectionSnapshot } from "../daemon/connection";
import type { Store } from "../daemon/store";
import type { Capability } from "../daemon/types";
import type { Provenance, SourceResult, Sources } from "../sources/types";
import type { SurfaceResult, SurfaceSources } from "../sources/surfaces/types";
import type { DraftsStore } from "./drafts";
import type { RouterStore } from "./router";

export interface Services {
  connection: ConnectionManager;
  data: AppData;
  sources: Sources;
  /** D9-D16 operator-experience sources (M7-03): derived when a connector is bound, fixtures otherwise. */
  surfaces: SurfaceSources;
  router: RouterStore;
  drafts: DraftsStore;
  /** False in explicit fixture demo mode: no connector, sample data only. */
  bridgeBound: boolean;
  /** S6 is owned by M5-04; it plugs in here and receives the same services. */
  renderReview?: (props: { projectId: string; taskId: string | null; query?: string }) => ReactNode;
}

const ServicesContext = createContext<Services | null>(null);

export function ServicesProvider({ services, children }: { services: Services; children: ReactNode }) {
  return <ServicesContext.Provider value={services}>{children}</ServicesContext.Provider>;
}

export function useServices(): Services {
  const services = useContext(ServicesContext);
  if (!services) throw new Error("ServicesProvider is missing");
  return services;
}

export function useStore<T>(store: Store<T>): T {
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}

export interface Gate {
  connection: ConnectionSnapshot;
  connected: boolean;
  /** Mutations need a live connection and the negotiated capability. */
  can(capability: Capability): boolean;
}

export function useGate(): Gate {
  const { connection } = useServices();
  const snapshot = useStore(connection);
  return useMemo(() => {
    const connected = snapshot.status === "connected";
    return {
      connection: snapshot,
      connected,
      can: (capability: Capability) => connected && snapshot.capabilities.includes(capability),
    };
  }, [snapshot]);
}

export type SourceState<T> = { state: "loading" } | SourceResult<T>;

/**
 * Loads a D1-D6 source result; `key` changes re-run the loader. A daemon source
 * (`provenance.kind === "daemon"`) also re-runs when the connection is
 * re-established or its status changes, because its answers depend on it.
 */
export function useSource<T>(
  load: () => Promise<SourceResult<T>>,
  key: string,
  provenance?: Provenance,
): SourceState<T> {
  const { connection } = useServices();
  const snapshot = useStore(connection);
  const fullKey =
    provenance?.kind === "daemon"
      ? `${key}@${snapshot.generation}:${snapshot.status}:${snapshot.history.reason ?? ""}`
      : key;
  const [result, setResult] = useState<{ key: string; value: SourceResult<T> } | null>(null);
  useEffect(() => {
    let live = true;
    load().then(
      (value) => live && setResult({ key: fullKey, value }),
      (error: unknown) =>
        live &&
        setResult({
          key: fullKey,
          value: { state: "error", message: error instanceof Error ? error.message : "source failed" },
        }),
    );
    return () => {
      live = false;
    };
    // The loader closes over `key`; re-running on key is the contract.
  }, [fullKey]);
  return result && result.key === fullKey ? result.value : { state: "loading" };
}

export type SurfaceState<T> = { status: "loading" } | SurfaceResult<T>;

export interface SurfaceRefresh {
  /** Re-load on this interval. */
  pollMs?: number;
  /** Re-load when the window regains focus or the page becomes visible. */
  onFocus?: boolean;
  /** Re-load just after each local midnight, for figures scoped to the local day. */
  atLocalMidnight?: boolean;
}

function msUntilLocalMidnight(): number {
  const now = new Date();
  const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime();
  return Math.max(next - now.getTime(), 0) + 1_000;
}

/**
 * Loads a D9-D16 result. Same re-run rule as `useSource`: a daemon-provenance
 * surface also re-runs when the connection generation or status changes. A
 * rejected load becomes `unavailable` with the source's own provenance.
 *
 * `refresh` re-loads in place: the previous answer stays on screen until the new one
 * arrives, a slower earlier answer never overwrites a later one, and every timer and
 * listener is released on unmount or when the key changes.
 */
export function useSurface<T>(
  load: () => Promise<SurfaceResult<T>>,
  key: string,
  provenance: Provenance,
  refresh?: SurfaceRefresh,
): SurfaceState<T> {
  const { connection } = useServices();
  const snapshot = useStore(connection);
  const fullKey =
    provenance.kind === "daemon"
      ? `${key}@${snapshot.generation}:${snapshot.status}:${snapshot.history.reason ?? ""}`
      : key;
  const [result, setResult] = useState<{ key: string; value: SurfaceResult<T> } | null>(null);
  const { pollMs, onFocus, atLocalMidnight } = refresh ?? {};
  useEffect(() => {
    let live = true;
    let latest = 0;
    const run = () => {
      const seq = ++latest;
      const apply = (value: SurfaceResult<T>) => live && seq === latest && setResult({ key: fullKey, value });
      load().then(
        apply,
        (error: unknown) =>
          apply({
            status: "unavailable",
            reason: error instanceof Error ? error.message : "source failed",
            provenance,
          }),
      );
    };
    run();
    const cleanups: Array<() => void> = [];
    if (pollMs) {
      const id = setInterval(run, pollMs);
      cleanups.push(() => clearInterval(id));
    }
    if (onFocus) {
      const onVisible = () => document.visibilityState !== "hidden" && run();
      window.addEventListener("focus", run);
      document.addEventListener("visibilitychange", onVisible);
      cleanups.push(() => {
        window.removeEventListener("focus", run);
        document.removeEventListener("visibilitychange", onVisible);
      });
    }
    if (atLocalMidnight) {
      let timer: ReturnType<typeof setTimeout>;
      const schedule = () => {
        timer = setTimeout(() => {
          run();
          schedule();
        }, msUntilLocalMidnight());
      };
      schedule();
      cleanups.push(() => clearTimeout(timer));
    }
    return () => {
      live = false;
      cleanups.forEach((cleanup) => cleanup());
    };
    // The loader closes over `key`; re-running on key is the contract.
  }, [fullKey, pollMs, onFocus, atLocalMidnight]);
  return result && result.key === fullKey ? result.value : { status: "loading" };
}
