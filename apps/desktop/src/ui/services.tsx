import { createContext, useContext, useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import type { AppData } from "../daemon/appData";
import type { ConnectionManager, ConnectionSnapshot } from "../daemon/connection";
import type { Store } from "../daemon/store";
import type { Capability } from "../daemon/types";
import type { Provenance, SourceResult, Sources } from "../sources/types";
import type { DraftsStore } from "./drafts";
import type { RouterStore } from "./router";

export interface Services {
  connection: ConnectionManager;
  data: AppData;
  sources: Sources;
  router: RouterStore;
  drafts: DraftsStore;
  /** False when the host has not supplied a daemon connector (M5-05 bridge). */
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
