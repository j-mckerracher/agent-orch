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
 * - A daemon without `agents.catalog` keeps the earlier behavior: providers
 *   from `agents.capabilities` with models and efforts `unsupported`.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError, presentError } from "../../daemon/errors";
import { agentDisplayName } from "../../daemon/agentDisplayNames";
import type { AgentCatalogEntry, CatalogFacet, CatalogStateWire } from "../../daemon/types";
import { liveRead } from "./liveSupport";
import { measured, notOk, observation, unsupportedValue } from "./result";
import type {
  Measured,
  ModeGroup,
  ModeOption,
  ModelCatalog,
  ModelCatalogSource,
  ModelOption,
  ModelProvider,
  Observation,
  Provenance,
} from "./types";

export const MODELS_METHODS = ["agents.capabilities", "agents.catalog", "agents.validate_selection"] as const;

const RETRY_SHORTLY = "The catalog is still being read; retry shortly.";

function parseOption(value: unknown): ModeOption | null {
  if (typeof value !== "object" || value === null) return null;
  const option = value as Record<string, unknown>;
  if (
    typeof option.id !== "string" ||
    typeof option.label !== "string" ||
    typeof option.description !== "string" ||
    typeof option.available !== "boolean" ||
    typeof option.needs_confirmation !== "boolean" ||
    typeof option.default !== "boolean" ||
    typeof option.allows_actions !== "boolean"
  ) {
    return null;
  }
  return {
    id: option.id,
    label: option.label,
    description: option.description,
    available: option.available,
    unavailableReason: typeof option.unavailable_reason === "string" ? option.unavailable_reason : null,
    needsConfirmation: option.needs_confirmation,
    isDefault: option.default,
    allowsActions: option.allows_actions,
  };
}

/** A group with a malformed option is dropped whole: a half-read group would offer choices the daemon did not state. */
function parseGroup(value: unknown): ModeGroup | null {
  if (typeof value !== "object" || value === null) return null;
  const group = value as Record<string, unknown>;
  if (typeof group.id !== "string" || typeof group.label !== "string" || !Array.isArray(group.options)) return null;
  const options = group.options.map(parseOption);
  if (options.some((option) => option === null)) return null;
  return { id: group.id, label: group.label, options: options as ModeOption[] };
}

/** An absent `modes` field means the daemon predates chat modes, so the list is unsupported rather than empty. */
function parseModes(raw: unknown, obs: Observation): Measured<ModeGroup[]> {
  if (!Array.isArray(raw)) return unsupportedValue("The daemon does not report chat modes for this agent.");
  return measured(
    raw.map(parseGroup).filter((group): group is ModeGroup => group !== null),
    obs,
  );
}

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
          if (failed === null) return { status: "unknown", reason: `The catalog returned no row for this agent; ${what} have not been observed.` };
          if (failed.kind === "method-unavailable" || failed.kind === "capability-unavailable") {
            return unsupportedValue<T>(`This daemon does not support the model catalog; ${what} are unavailable.`);
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
          const models = row
            ? facetValue(row, row.models, obs, (items) =>
                items.map<ModelOption>((item) => ({
                  id: item.id,
                  label: item.display_name || item.id,
                  efforts: item.efforts,
                  defaultEffort: item.default_effort,
                })),
              )
            : withoutRow<ModelOption[]>("models");
          return {
            id: entry.agent,
            name: agentDisplayName(entry.agent),
            installed: entry.installed,
            launchable: entry.launchable,
            ...(entry.chat_compatibility ? { chatCompatibility: entry.chat_compatibility } : {}),
            ...(entry.mode_compatibility ? { modeCompatibility: entry.mode_compatibility } : {}),
            modelSelection: supported,
            models,
            modelsNote: row && models.status === "ok" && row.models.detail ? row.models.detail : undefined,
            ...(row && row.state !== "available" && row.detail ? { detail: row.detail } : {}),
            efforts: row
              ? facetValue(row, row.efforts, obs, (items) => items)
              : withoutRow<string[]>("effort settings"),
            modes: row ? parseModes(row.modes, obs) : withoutRow<ModeGroup[]>("chat modes"),
          };
        });
        return { providers };
      }),

    validate: async (agent, selection) => {
      if (selection.model === undefined && selection.effort === undefined) {
        return notOk(provenance, "unknown", "Choose a model or an effort to check.", observation(now()));
      }
      return liveRead(provenance, now, async () => {
        const check = await connection.api().agentsValidateSelection(agent, selection);
        return check.valid ? { valid: true as const } : { valid: false as const, reason: check.reason, detail: check.detail };
      });
    },

    // DPES-1105: one billable call only when the user asks for it; the daemon caches the answer.
    checkCopilotSignIn: (force) => liveRead(provenance, now, () => connection.api().agentsCopilotSignInCheck(force)),

    // Zero-cost: the sign-in record rides on `agents.capabilities`. A daemon without the row says so.
    copilotSignInStatus: () =>
      liveRead(provenance, now, async () => {
        const row = (await connection.api().agentsCapabilities()).find((entry) => entry.agent === "github-copilot");
        return (
          row?.sign_in ?? {
            state: "not_checked" as const,
            reason: "The daemon reports no GitHub Copilot sign-in state.",
            checked_at_ms: null,
            premium_requests: null,
            ai_credits: null,
            warning: null,
          }
        );
      }),
  };
}
