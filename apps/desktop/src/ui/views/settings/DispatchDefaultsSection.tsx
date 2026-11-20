import { useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import type { DispatchDefaults, ModelProvider } from "../../../sources/surfaces/types";
import { useGate, useServices, useSurface } from "../../services";
import { Button, Disclosure } from "../../theme";
import { Notice } from "../../widgets";
import { effortsFor } from "../workspace/ModelPicker";
import { outcomeOf, usePageSave, type Outcome } from "./pageSave";
import { NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import type { SettingsModel } from "./useSettings";

export function DispatchDefaultsSection({ model }: { model: SettingsModel }) {
  const saved = model.load.status === "ready" ? model.load.snapshot.dispatchDefaults : undefined;
  return saved === undefined ? null : <DefaultsForm model={model} saved={saved} />;
}

type Selection = { model?: string; effort?: string };

/** One provider's model and effort pickers. `names` are the two labels. */
function PairRows({
  provider,
  selection,
  names,
  idPrefix,
  disabled,
  onChoose,
}: {
  provider: ModelProvider;
  selection: Selection;
  names: { model: string; effort: string };
  idPrefix: string;
  disabled: boolean;
  onChoose: (selection: Selection) => void;
}) {
  const models = provider.models.status === "ok" ? provider.models.value : [];
  const selected = models.find((item) => item.id === selection.model);
  const efforts = effortsFor(provider, selected);
  return (
    <>
      <div className="settings-row">
        <label htmlFor={`${idPrefix}-model`}>{names.model}</label>
        <select
          id={`${idPrefix}-model`}
          value={selection.model ?? ""}
          disabled={disabled || (models.length === 0 && !selection.model)}
          onChange={(event) => onChoose(event.target.value ? { model: event.target.value } : {})}
        >
          <option value="">Provider default model</option>
          {selection.model && !selected && <option value={selection.model}>{selection.model} (unavailable)</option>}
          {models.map((item) => (
            <option key={item.id} value={item.id}>
              {item.label}
            </option>
          ))}
        </select>
        {provider.models.status !== "ok" && <p className="settings-hint">{provider.models.reason}</p>}
      </div>
      <div className="settings-row">
        <label htmlFor={`${idPrefix}-effort`}>{names.effort}</label>
        <select
          id={`${idPrefix}-effort`}
          value={selection.effort ?? ""}
          disabled={disabled || (efforts.length === 0 && !selection.effort)}
          onChange={(event) => onChoose({ ...selection, effort: event.target.value || undefined })}
        >
          <option value="">Provider default effort</option>
          {selection.effort && !efforts.includes(selection.effort) && <option value={selection.effort}>{selection.effort} (unavailable)</option>}
          {efforts.map((effort) => (
            <option key={effort} value={effort}>
              {effort}
            </option>
          ))}
        </select>
      </div>
    </>
  );
}

function DefaultsForm({ model, saved }: { model: SettingsModel; saved: DispatchDefaults }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const id = useId();
  const [reloads, setReloads] = useState(0);
  const catalog = useSurface(() => surfaces.models.catalog(), `dispatch-models:${reloads}`, surfaces.models.provenance);
  const [typed, setTyped] = useState<DispatchDefaults | null>(null);
  const [fresh, setFresh] = useState<{ base: DispatchDefaults; value: DispatchDefaults } | null>(null);
  const value = typed ?? (fresh?.base === saved ? fresh.value : saved);
  const providers = catalog.status === "ok" ? catalog.data.providers : [];
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;
  const change = (next: DispatchDefaults) => setTyped(next);
  const choose = (agent: string, selection: Selection) => {
    const next = { ...value.providers };
    if (selection.model || selection.effort) next[agent] = selection;
    else delete next[agent];
    change({ ...value, providers: next });
  };

  usePageSave(reason, async (): Promise<Outcome> => {
    try {
      if (model.live && surfaces.models.validate) {
        for (const [agent, selection] of Object.entries(value.providers)) {
          const checked = await surfaces.models.validate(agent, selection);
          if (checked.status !== "ok") return { kind: "failed", text: `${agent}: ${checked.reason}` };
          if (!checked.data.valid) return { kind: "failed", text: `${agent}: ${checked.data.detail}` };
        }
      }
      const receipt = await surfaces.settings.setGroups({ dispatchDefaults: value });
      if (receipt.simulated) return outcomeOf({ kind: "fixture" }, "");
      // Keep the accepted values visible until the next settings read catches up.
      setFresh({ base: saved, value: receipt.snapshot.dispatchDefaults ?? value });
      setTyped(null);
      model.reload();
      return outcomeOf({ kind: "saved" }, "Saved. New tasks use these defaults.");
    } catch (error) {
      return outcomeOf({ kind: "failed", message: saveFailureText(asDaemonError(error)) }, "");
    }
  });

  const defaultProvider = providers.find((provider) => provider.id === value.defaultAgent);
  const others = providers.filter((provider) => provider.id !== value.defaultAgent);
  const overridden = others.filter((provider) => value.providers[provider.id] !== undefined).length;
  return (
    <section aria-label="Defaults for new tasks" className="settings-section">
      <p className="settings-intro">Applies to new tasks only. Chat, queued tasks and running tasks are unchanged.</p>
      {catalog.status === "loading" && <p role="status">Loading the model catalog...</p>}
      {catalog.status !== "loading" && catalog.status !== "ok" && <Notice tone="warn">{catalog.reason}</Notice>}
      <div className="settings-row">
        <label htmlFor={`${id}-provider`}>Default provider</label>
        <select
          id={`${id}-provider`}
          value={value.defaultAgent ?? ""}
          disabled={locked}
          onChange={(event) => change({ ...value, defaultAgent: event.target.value || null })}
        >
          <option value="">No default (tasks must choose a provider)</option>
          {value.defaultAgent && !providers.some((p) => p.id === value.defaultAgent) && (
            <option value={value.defaultAgent}>{value.defaultAgent} (unavailable)</option>
          )}
          {providers.map((provider) => (
            <option key={provider.id} value={provider.id} disabled={!provider.launchable}>
              {provider.name}
              {provider.launchable ? "" : " (unavailable)"}
            </option>
          ))}
        </select>
      </div>
      {defaultProvider && (
        <PairRows
          provider={defaultProvider}
          selection={value.providers[defaultProvider.id] ?? {}}
          names={{ model: "Default model", effort: "Default effort" }}
          idPrefix={`${id}-default`}
          disabled={locked}
          onChoose={(selection) => choose(defaultProvider.id, selection)}
        />
      )}
      {others.length > 0 && (
        <Disclosure summary="Per-provider overrides" count={overridden > 0 ? `${overridden} set` : undefined}>
          {others.map((provider) => (
            <fieldset key={provider.id} disabled={locked}>
              <legend>{provider.name}</legend>
              {!provider.launchable && <p className="settings-hint">{provider.detail ?? "This provider is unavailable."}</p>}
              <PairRows
                provider={provider}
                selection={value.providers[provider.id] ?? {}}
                names={{ model: `${provider.name} model`, effort: `${provider.name} effort` }}
                idPrefix={`${id}-${provider.id}`}
                disabled={false}
                onChoose={(selection) => choose(provider.id, selection)}
              />
            </fieldset>
          ))}
        </Disclosure>
      )}
      {Object.keys(value.providers)
        .filter((agent) => catalog.status === "ok" && !providers.some((p) => p.id === agent))
        .map((agent) => (
          <Notice key={agent} tone="warn">
            Saved provider {agent} is unavailable.{" "}
            <Button disabled={locked} onClick={() => choose(agent, {})}>
              Clear {agent} choice
            </Button>
          </Notice>
        ))}
      <div className="settings-inline-actions">
        <Button onClick={() => setReloads((n) => n + 1)}>Refresh models</Button>
        <Button disabled={locked} onClick={() => change({ defaultAgent: null, providers: {} })}>
          Clear defaults
        </Button>
      </div>
    </section>
  );
}
