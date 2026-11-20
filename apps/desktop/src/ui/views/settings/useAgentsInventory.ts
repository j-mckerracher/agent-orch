/** Adapter inventory for Settings > Agents: one shape for the daemon (`agents.capabilities`) and the fixture source. */
import { useState } from "react";
import { asDaemonError, presentError } from "../../../daemon/errors";
import { useServices, useSource } from "../../services";
import { asLoad, type Load } from "../agents/useAgentsData";
import { inventoryFromDaemon, inventoryFromPreview, type InventoryRow } from "./inventoryModel";

export function useAgentsInventory(): { live: boolean; inventory: Load<InventoryRow[]>; reload: () => void } {
  const { sources, connection } = useServices();
  const live = sources.agents.provenance.kind === "daemon";
  const [nonce, setNonce] = useState(0);
  const source = useSource<InventoryRow[]>(
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
  return { live, inventory: asLoad(source, (list) => list), reload: () => setNonce((n) => n + 1) };
}
