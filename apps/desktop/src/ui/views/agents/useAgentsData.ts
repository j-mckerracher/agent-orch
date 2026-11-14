/** Data for the Agents & Sessions view: one shape for daemon-backed (live) and fixture mode. */
import { useMemo, useState } from "react";
import { asDaemonError, presentError } from "../../../daemon/errors";
import type { SourceResult } from "../../../sources/types";
import { useGate, useServices, useSource, useStore } from "../../services";
import { inventoryFromDaemon, inventoryFromPreview, type InventoryRow } from "./inventoryModel";
import { rowsFromDaemon, rowsFromPreview, type WorkRow } from "./model";

export type Load<T> =
  | { status: "loading" }
  | { status: "empty" }
  | { status: "unavailable"; reason: string }
  | { status: "error"; message: string }
  | { status: "ready"; data: T; stale: string | null };

export interface AgentsData {
  /** True when the sources are daemon-backed; wireframe and fixture values never appear then. */
  live: boolean;
  work: Load<WorkRow[]>;
  inventory: Load<InventoryRow[]>;
  reloadWork: () => void;
  reloadInventory: () => void;
}

const asLoad = <T, U>(result: { state: "loading" } | SourceResult<T>, map: (data: T) => U): Load<U> => {
  switch (result.state) {
    case "loading":
      return { status: "loading" };
    case "empty":
      return { status: "empty" };
    case "unavailable":
      return { status: "unavailable", reason: result.reason };
    case "error":
      return { status: "error", message: result.message };
    case "ready":
      return { status: "ready", data: map(result.data), stale: null };
  }
};

export function useAgentsData(): AgentsData {
  const { data, sources, connection } = useServices();
  const snapshot = useStore(data);
  const gate = useGate();
  const live = sources.execution.provenance.kind === "daemon";
  const [nonce, setNonce] = useState(0);

  const preview = useSource(
    () => (live ? Promise.resolve({ state: "empty" } as SourceResult<never>) : sources.execution.listPreviewTasks()),
    `agents-preview:${live}`,
    sources.execution.provenance,
  );

  const inventorySource = useSource<InventoryRow[]>(
    async () => {
      if (!live) {
        const result = await sources.agents.listAgents();
        return result.state === "ready" ? { state: "ready", data: inventoryFromPreview(result.data) } : result;
      }
      try {
        const entries = await connection.api().agentsCapabilities();
        return entries.length === 0 ? { state: "empty" } : { state: "ready", data: inventoryFromDaemon(entries) };
      } catch (error) {
        const failed = asDaemonError(error);
        return { state: "error", message: `${presentError(failed).reason} (${failed.kind}: ${failed.message})` };
      }
    },
    `agents-inventory:${live}:${nonce}`,
    sources.agents.provenance,
  );

  const work = useMemo<Load<WorkRow[]>>(() => {
    if (!live) {
      // A known-empty preview is an answer: zero running, zero queued, not an unavailable list.
      const mapped = asLoad(preview, rowsFromPreview);
      return mapped.status === "empty" ? { status: "ready", data: [], stale: null } : mapped;
    }
    const { tasks, sessions, approvals } = snapshot;
    if (tasks.data) {
      const failed = tasks.status === "error" || sessions.status === "error";
      const stale = failed ? "The last refresh failed; these rows may be out of date." : null;
      return { status: "ready", data: rowsFromDaemon(tasks.data, sessions.data, approvals.data ?? []), stale };
    }
    if (tasks.status === "error") return { status: "error", message: tasks.error?.message ?? "tasks.list failed" };
    if (gate.connected || gate.settling) return { status: "loading" };
    return { status: "unavailable", reason: "Not connected to the daemon, so tasks cannot be read." };
  }, [live, preview, snapshot, gate.connected, gate.settling]);

  return {
    live,
    work,
    inventory: asLoad(inventorySource, (list) => list),
    reloadWork: () => void data.refreshAll(),
    reloadInventory: () => setNonce((n) => n + 1),
  };
}
