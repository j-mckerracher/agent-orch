/**
 * Live D14 model catalog (M7-24), bound to `agents.catalog` and
 * `agents.validate_selection`, joined with `agents.capabilities` for the
 * installed and launchable flags the catalog does not carry.
 *
 * - Models and efforts come from the daemon only. Codex reports efforts per
 *   model (on each option); Claude reports adapter-wide efforts; Pi is
 *   unsupported. Nothing is invented to fill a gap.
 * - A cold or slow catalog read is `unknown` with the daemon's "retry shortly"
 *   hint, not an error. A refused catalog read (`Conflict`) is treated the same.
 * - A daemon without `agents.catalog` keeps the earlier behaviour: providers
 *   from `agents.capabilities` with models and efforts `unsupported`.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError, presentError } from "../../daemon/errors";
import type { AgentCatalogEntry, CatalogFacet, CatalogStateWire } from "../../daemon/types";
import { liveRead } from "./liveSupport";
import { measured, notOk, observation, unsupportedValue } from "./result";
import type {
  Measured,
  ModelCatalog,
  ModelCatalogSource,
  ModelOption,
  ModelProvider,
  Observation,
  Provenance,
} from "./types";

export const MODELS_METHODS = ["agents.capabilities", "agents.catalog", "agents.validate_selection"] as const;

const RETRY_SHORTLY = "The catalog is still being read; retry shortly.";

function facetValue<T, U>(
  row: AgentCatalogEntry,
  facet: CatalogFacet<T>,
  obs: Observation,
  map: (items: T[]) => U,
): Measured<U> {
  const state: CatalogStateWire = facet.state === "available" && row.state !== "available" ? row.state : facet.state;
  const detail = facet.detail || row.detail;
  switch (state) {
    case "available":
      return measured(map(facet.items), obs);
    case "unknown":
      return { status: "unknown", reason: detail || RETRY_SHORTLY };
    case "unsupported":
      return { status: "unsupported", reason: detail || "The agent does not support this setting." };
    case "authentication_failed":
      return { status: "unavailable", reason: `Sign-in required: ${detail || "authentication failed"}` };
    default:
      return { status: "unavailable", reason: detail || "The agent is not available." };
  }
}

export function createLiveModels(
  connection: ConnectionManager,
  provenance: Provenance,
  now: () => number,
): Required<ModelCatalogSource> {
  return {
    provenance,
    catalog: () =>
      liveRead<ModelCatalog>(provenance, now, async () => {
        const api = connection.api();
        const obs = observation(now());
        const [capabilities, catalog] = await Promise.all([
          api.agentsCapabilities(),
          api.agentsCatalog().then(
            (rows) => ({ rows }),
            (error: unknown) => ({ failed: asDaemonError(error) }),
          ),
        ]);
        const rows = "rows" in catalog ? catalog.rows : null;
        const failed = "failed" in catalog ? catalog.failed : null;

        const withoutRow = <T,>(what: string): Measured<T> => {
          if (failed === null) return unsupportedValue<T>(`The daemon lists agents, not their ${what}.`);
          if (failed.kind === "method-unavailable" || failed.kind === "capability-unavailable") {
            return unsupportedValue<T>(`The daemon lists agents, not their ${what}.`);
          }
          if (failed.kind === "conflict") return { status: "unknown", reason: RETRY_SHORTLY };
          return { status: "unavailable", reason: presentError(failed).reason };
        };

        const providers = capabilities.map<ModelProvider>((entry) => {
          const row = rows?.find((candidate) => candidate.agent === entry.agent);
          const supported =
            row?.state === "unsupported"
              ? ("unsupported" as const)
              : entry.capabilities === null
                ? ("unknown" as const)
                : entry.capabilities.model_selection
                  ? ("supported" as const)
                  : ("unsupported" as const);
          return {
            id: entry.agent,
            name: entry.agent,
            installed: entry.installed,
            launchable: entry.launchable,
            modelSelection: supported,
            models: row
              ? facetValue(row, row.models, obs, (items) =>
                  items.map<ModelOption>((item) => ({
                    id: item.id,
                    label: item.display_name || item.id,
                    efforts: item.efforts,
                    defaultEffort: item.default_effort,
                  })),
                )
              : withoutRow<ModelOption[]>("models"),
            efforts: row
              ? facetValue(row, row.efforts, obs, (items) => items)
              : withoutRow<string[]>("effort settings"),
          };
        });
        return { providers };
      }),

    validate: async (agent, selection) => {
      if (selection.model === undefined && selection.effort === undefined) {
        return notOk(provenance, "unknown", "Choose a model or an effort to check.");
      }
      return liveRead(provenance, now, async () => {
        const check = await connection.api().agentsValidateSelection(agent, selection);
        return check.valid ? { valid: true as const } : { valid: false as const, reason: check.reason, detail: check.detail };
      });
    },
  };
}
