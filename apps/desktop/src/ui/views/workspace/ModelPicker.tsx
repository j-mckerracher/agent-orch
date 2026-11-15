import { useEffect, useId, useState } from "react";
import type {
  ModelCatalog,
  ModelCatalogSource,
  ModelOption,
  ModelProvider,
  ModelSelectionCheck,
  Provenance,
  SurfaceResult,
} from "../../../sources/surfaces/types";
import type { SurfaceState } from "../../services";
import { formatAge } from "../../shell/ResourceStrip";
import { Badge, Button } from "../../theme";
import { usePopover } from "../../usePopover";

const LIVE_NOTE =
  "Models and efforts come from the daemon's catalog, and the daemon checks each choice. The choice is not sent with messages or task submissions yet.";
const FIXTURE_NOTE = "Placeholder options. A choice stays in this window and is never sent anywhere.";
const DEFAULT_MODEL = "Default model";
const DEFAULT_EFFORT = "Default effort";
const RETRY_MS = 2000;
const MAX_AUTO_RETRIES = 5;
const COLD_REASONS = new Set(["agent_state_unknown", "models_unknown", "effort_unknown"]);

type Check = { key: string; result: SurfaceResult<ModelSelectionCheck> | "checking" };

/** Per-model efforts win; an agent that reports none per model uses its adapter-wide list. */
function effortsFor(provider: ModelProvider | undefined, model: ModelOption | undefined): string[] {
  if (model?.efforts && model.efforts.length > 0) return model.efforts;
  return provider?.efforts.status === "ok" ? provider.efforts.value : [];
}

/** A catalog reason shown as helper text: one sentence with a capital letter. */
function sentence(text: string): string {
  const trimmed = text.trim();
  if (trimmed === "") return trimmed;
  const first = trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
  return /[.!?]$/.test(first) ? first : `${first}.`;
}

/** The model part of the chip: the chosen model, the default model, or an honest "not confirmed". */
function modelChip(provider: ModelProvider, model: ModelOption | undefined): string {
  if (model) return model.label;
  switch (provider.models.status) {
    case "ok":
    case "unsupported":
      return "default model";
    case "unavailable":
      return "models unavailable";
    default:
      return "models not confirmed";
  }
}

/** Helper text under the Model select, or null when there is nothing to say. */
function modelHelp(provider: ModelProvider | undefined): string | null {
  if (provider === undefined) return null;
  if (provider.models.status === "ok") return provider.modelsNote ?? null;
  return sentence(provider.models.reason);
}

/** Helper text under the Effort select when it offers no levels. */
function effortHelp(
  provider: ModelProvider | undefined,
  model: ModelOption | undefined,
  efforts: string[],
  live: boolean,
): string | null {
  if (provider === undefined || efforts.length > 0) return null;
  const perModel = provider.models.status === "ok" && provider.models.value.some((item) => (item.efforts?.length ?? 0) > 0);
  if (live && model === undefined && perModel) return "Effort levels are listed per model. Choose a model to see its levels.";
  if (provider.efforts.status !== "ok") return sentence(provider.efforts.reason);
  return null;
}

/**
 * Provider / model / effort menu. Live (daemon provenance): real catalog data with per-model
 * efforts, each choice checked with `agents.validate_selection`, and a cold catalog retried
 * shortly. Fixtures: placeholder options that can be tried locally. Neither mode sends or
 * persists a choice.
 */
export function ModelPicker({
  catalog,
  provenance,
  validate,
  onRetry,
}: {
  catalog: SurfaceState<ModelCatalog>;
  provenance: Provenance;
  validate?: ModelCatalogSource["validate"];
  onRetry?: () => void;
}) {
  const popover = usePopover();
  const ids = useId();
  const live = provenance.kind === "daemon";
  const [providerId, setProviderId] = useState<string | null>(null);
  const [modelId, setModelId] = useState<string | null>(null);
  const [effort, setEffort] = useState<string | null>(null);
  const [check, setCheck] = useState<Check | null>(null);
  const [retries, setRetries] = useState(0);

  const providers = catalog.status === "ok" ? catalog.data.providers : [];
  // A provider the daemon cannot launch is listed but never chosen, and never the default.
  const provider: ModelProvider | undefined =
    providers.find((item) => item.launchable && item.id === providerId) ??
    providers.find((item) => item.launchable);
  const models = provider?.models.status === "ok" ? provider.models.value : [];
  // No model chosen is the default model: no `--model` is given, so the agent's own choice applies.
  const model = models.find((item) => item.id === modelId);
  const efforts = live ? effortsFor(provider, model) : provider?.efforts.status === "ok" ? provider.efforts.value : [];
  let chosenEffort: string | null;
  if (live) {
    const fallback = model?.defaultEffort && efforts.includes(model.defaultEffort) ? model.defaultEffort : null;
    chosenEffort = efforts.includes(effort ?? "") ? effort : fallback;
  } else {
    chosenEffort = efforts.includes(effort ?? "") ? effort : (efforts[1] ?? efforts[0] ?? null);
  }

  const summary =
    provider === undefined
      ? "Model settings unavailable"
      : `${provider.name} · ${modelChip(provider, model)} · ${chosenEffort ?? (live ? "default effort" : "effort not available")}`;

  const lock = provider === undefined;
  const note = live ? LIVE_NOTE : FIXTURE_NOTE;
  const noteId = `${ids}-note`;
  const modelHelpId = `${ids}-model-help`;
  const effortHelpId = `${ids}-effort-help`;
  const modelHelpText = modelHelp(provider);
  const effortHelpText = effortHelp(provider, model, efforts, live);
  const describedBy = (...parts: (string | null)[]) => parts.filter(Boolean).join(" ");
  const triggerBadge = "PLACEHOLDER (D14)";
  const panelBadge = "PLACEHOLDER DATA (D14)";

  // A cold catalog is "unknown": retry a few times, then leave a manual Retry.
  const cold = live && catalog.status === "ok" && providers.some((item) => item.models.status === "unknown");
  useEffect(() => {
    if (!cold) {
      setRetries(0);
      return;
    }
    if (!onRetry || retries >= MAX_AUTO_RETRIES) return;
    const timer = setTimeout(() => {
      setRetries((count) => count + 1);
      onRetry();
    }, RETRY_MS);
    return () => clearTimeout(timer);
  }, [cold, retries, onRetry]);

  const selectionKey = `${provider?.id}|${model?.id}|${chosenEffort}|${catalog.status}`;
  useEffect(() => {
    if (!live || !validate || !provider || !popover.open || (!model && !chosenEffort)) return;
    let current = true;
    setCheck({ key: selectionKey, result: "checking" });
    validate(provider.id, {
      ...(model ? { model: model.id } : {}),
      ...(chosenEffort ? { effort: chosenEffort } : {}),
    }).then(
      (result) => current && setCheck({ key: selectionKey, result }),
      (error: unknown) => {
        if (!current) return;
        const observedAtMs = Date.now();
        setCheck({
          key: selectionKey,
          result: {
            status: "unavailable",
            reason: error instanceof Error ? error.message : "check failed",
            provenance,
            observedAt: new Date(observedAtMs).toISOString(),
            ageMs: 0,
          },
        });
      },
    );
    return () => {
      current = false;
    };
    // The check follows the selection; `validate` is stable per source.
  }, [live, validate, popover.open, selectionKey]);

  let unavailable: string | null = null;
  if (catalog.status === "loading") unavailable = "Loading the model catalog…";
  else if (catalog.status !== "ok") {
    unavailable = `Model settings are not available: ${catalog.reason} (observed ${formatAge(catalog.observedAt, catalog.ageMs, Date.now())})`;
  }
  else if (providers.length === 0) unavailable = "No providers are reported.";
  else if (!providers.some((item) => item.launchable)) unavailable = "No launchable providers are reported.";

  const verdict = check?.key === selectionKey ? check.result : null;
  let verdictText: string | null = null;
  if (live && verdict !== null) {
    if (verdict === "checking") verdictText = "Checking this choice with the daemon…";
    else if (verdict.status !== "ok") {
      verdictText = `This choice could not be checked: ${verdict.reason} (observed ${formatAge(verdict.observedAt, verdict.ageMs, Date.now())})`;
    }
    else if (verdict.data.valid) verdictText = "The daemon accepts this choice.";
    else if (COLD_REASONS.has(verdict.data.reason)) {
      verdictText = `Not confirmed yet: the catalog is still being read; retry shortly. (${verdict.data.reason})`;
    } else verdictText = `Not supported: ${verdict.data.detail} (${verdict.data.reason})`;
  }

  return (
    <div
      ref={popover.rootRef}
      className="ws-popover-root ws-model-root"
      onKeyDown={popover.onKeyDown}
      onBlur={popover.onBlur}
    >
      <button
        ref={popover.buttonRef}
        type="button"
        className="ws-chip-button"
        aria-expanded={popover.open}
        aria-controls={`${ids}-menu`}
        aria-label={`Model settings: ${summary}${!live && catalog.status === "ok" ? ` ${triggerBadge}` : ""}`}
        onClick={() => popover.setOpen((value) => !value)}
      >
        <span>{summary}</span>
        {!live && catalog.status === "ok" && (
          <>
            {" "}
            <Badge tone="placeholder">{triggerBadge}</Badge>
          </>
        )}
        <span aria-hidden="true">⌄</span>
      </button>
      <div id={`${ids}-menu`} className="ws-popover ws-model-panel" hidden={!popover.open}>
        <p className="ws-menu-title">
          {live ? "Model settings" : "Conversation settings"} {!live && <Badge tone="placeholder">{panelBadge}</Badge>}
        </p>
        {unavailable && <p role="status">{unavailable}</p>}
        {cold && (
          <p role="status" className="ws-small">
            The catalog is still being read; retry shortly.{" "}
            {retries < MAX_AUTO_RETRIES ? "Retrying automatically." : "Automatic retries stopped."}{" "}
            {onRetry && (
              <Button
                variant="quiet"
                onClick={() => {
                  setRetries(0);
                  onRetry();
                }}
              >
                Retry now
              </Button>
            )}
          </p>
        )}
        <label className="ws-field">
          Provider
          <select
            value={provider?.id ?? ""}
            disabled={lock}
            aria-describedby={noteId}
            onChange={(event) => {
              setProviderId(event.target.value);
              setModelId(null);
              setEffort(null);
            }}
          >
            {provider === undefined && (
              <option value="">{providers.length === 0 ? "Not available" : "No launchable providers"}</option>
            )}
            {providers.map((item) => (
              <option key={item.id} value={item.id} disabled={!item.launchable}>
                {item.name}
                {item.launchable ? "" : " (not launchable)"}
              </option>
            ))}
          </select>
        </label>
        <label className="ws-field">
          Model
          <select
            value={model?.id ?? ""}
            disabled={lock || models.length === 0}
            aria-describedby={describedBy(modelHelpText ? modelHelpId : null, noteId)}
            onChange={(event) => {
              setModelId(event.target.value === "" ? null : event.target.value);
              setEffort(null);
            }}
          >
            {provider === undefined ? <option value="">Not available</option> : <option value="">{DEFAULT_MODEL}</option>}
            {models.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        {modelHelpText && (
          <p id={modelHelpId} className="ws-small">
            {modelHelpText}
          </p>
        )}
        <label className="ws-field">
          Effort
          <select
            value={chosenEffort ?? ""}
            disabled={lock || efforts.length === 0}
            aria-describedby={describedBy(effortHelpText ? effortHelpId : null, noteId)}
            onChange={(event) => setEffort(event.target.value === "" ? null : event.target.value)}
          >
            {efforts.length === 0 && <option value="">{live && provider !== undefined ? DEFAULT_EFFORT : "Not available"}</option>}
            {live && efforts.length > 0 && <option value="">{DEFAULT_EFFORT}</option>}
            {efforts.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        {effortHelpText && (
          <p id={effortHelpId} className="ws-small">
            {effortHelpText}
          </p>
        )}
        {verdictText && (
          <p role="status" className="ws-small">
            {verdictText}
          </p>
        )}
        <p id={noteId} className="ws-small">
          {note}
        </p>
      </div>
    </div>
  );
}
