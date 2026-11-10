import { useId, useState } from "react";
import type { Measured, ModelCatalog, ModelProvider, Provenance } from "../../../sources/surfaces/types";
import type { SurfaceState } from "../../services";
import { Badge } from "../../theme";
import { usePopover } from "./usePopover";

const LIVE_REASON =
  "Provider, model and effort cannot be chosen: the daemon takes no conversation settings, so nothing here would be applied (D14).";
const FIXTURE_NOTE = "Placeholder options. A choice stays in this window and is never sent anywhere.";

const unavailableText = <T,>(value: Measured<T>, fallback: string): string =>
  value.status === "ok" ? fallback : value.reason;

/**
 * Provider / model / effort menu. D14 is PLACEHOLDER data: in live mode (daemon provenance)
 * the controls are disabled and listed from the real agent inventory only; with fixtures they
 * can be tried locally. Neither mode sends or persists a choice.
 */
export function ModelPicker({ catalog, provenance }: { catalog: SurfaceState<ModelCatalog>; provenance: Provenance }) {
  const popover = usePopover();
  const ids = useId();
  const live = provenance.kind === "daemon";
  const [providerId, setProviderId] = useState<string | null>(null);
  const [modelId, setModelId] = useState<string | null>(null);
  const [effort, setEffort] = useState<string | null>(null);

  const providers = catalog.status === "ok" ? catalog.data.providers : [];
  const provider: ModelProvider | undefined = providers.find((item) => item.id === providerId) ?? providers[0];
  const models = provider?.models.status === "ok" ? provider.models.value : [];
  const efforts = provider?.efforts.status === "ok" ? provider.efforts.value : [];
  const model = models.find((item) => item.id === modelId) ?? models[0];
  const chosenEffort = efforts.includes(effort ?? "") ? effort : (efforts[1] ?? efforts[0] ?? null);

  const summary =
    provider === undefined
      ? "Model settings unavailable"
      : `${provider.name} · ${model?.label ?? "model not available"} · ${chosenEffort ?? "effort not available"}`;

  const lock = live || provider === undefined;
  const note = live ? LIVE_REASON : FIXTURE_NOTE;
  const noteId = `${ids}-note`;
  const triggerBadge = "PLACEHOLDER (D14)";
  const panelBadge = live ? "PLACEHOLDER DATA (D14), unsupported" : "PLACEHOLDER DATA (D14)";

  let unavailable: string | null = null;
  if (catalog.status === "loading") unavailable = "Loading the model catalog…";
  else if (catalog.status !== "ok") unavailable = `Model settings are not available: ${catalog.reason}`;
  else if (providers.length === 0) unavailable = "No providers are reported.";

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
        aria-label={`Model settings: ${summary}${catalog.status === "ok" ? ` ${triggerBadge}` : ""}`}
        onClick={() => popover.setOpen((value) => !value)}
      >
        <span>{summary}</span>
        {catalog.status === "ok" && (
          <>
            {" "}
            <Badge tone="placeholder">{triggerBadge}</Badge>
          </>
        )}
        <span aria-hidden="true">⌄</span>
      </button>
      <div id={`${ids}-menu`} className="ws-popover ws-model-panel" hidden={!popover.open}>
        <p className="ws-menu-title">
          Conversation settings <Badge tone="placeholder">{panelBadge}</Badge>
        </p>
        {unavailable && <p role="status">{unavailable}</p>}
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
            {providers.length === 0 && <option value="">Not available</option>}
            {providers.map((item) => (
              <option key={item.id} value={item.id}>
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
            aria-describedby={noteId}
            onChange={(event) => setModelId(event.target.value)}
          >
            {models.length === 0 && (
              <option value="">{provider ? unavailableText(provider.models, "No models reported") : "Not available"}</option>
            )}
            {models.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label className="ws-field">
          Effort
          <select
            value={chosenEffort ?? ""}
            disabled={lock || efforts.length === 0}
            aria-describedby={noteId}
            onChange={(event) => setEffort(event.target.value)}
          >
            {efforts.length === 0 && (
              <option value="">{provider ? unavailableText(provider.efforts, "No effort levels reported") : "Not available"}</option>
            )}
            {efforts.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <p id={noteId} className="ws-small">
          {note}
        </p>
      </div>
    </div>
  );
}
