import { useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import type { DispatchDefaults } from "../../../sources/surfaces/types";
import { useGate, useServices, useSurface } from "../../services";
import { Button } from "../../theme";
import { Notice } from "../../widgets";
import { effortsFor } from "../workspace/ModelPicker";
import { SaveButton, useSaveGuard } from "./SaveBar";
import { FIXTURE_SAVE_TEXT, NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import type { SaveResult, SettingsModel } from "./useSettings";

export function DispatchDefaultsSection({ model }: { model: SettingsModel }) {
  const saved = model.load.status === "ready" ? model.load.snapshot.dispatchDefaults : undefined;
  return saved === undefined ? null : <DefaultsForm model={model} saved={saved} />;
}

function DefaultsForm({ model, saved }: { model: SettingsModel; saved: DispatchDefaults }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const id = useId();
  const [reloads, setReloads] = useState(0);
  const catalog = useSurface(() => surfaces.models.catalog(), `dispatch-models:${reloads}`, surfaces.models.provenance);
  const [typed, setTyped] = useState<DispatchDefaults | null>(null);
  const [fresh, setFresh] = useState<{ base: DispatchDefaults; value: DispatchDefaults } | null>(null);
  const [saving, setSaving] = useState(false);
  const busy = useRef(false);
  const [result, setResult] = useState<SaveResult | null>(null);
  const { tried, guard } = useSaveGuard();
  const value = typed ?? (fresh?.base === saved ? fresh.value : saved);
  const providers = catalog.status === "ok" ? catalog.data.providers : [];
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;
  const change = (next: DispatchDefaults) => {
    setTyped(next);
    setResult(null);
  };
  const choose = (agent: string, selection: { model?: string; effort?: string }) => {
    const next = { ...value.providers };
    if (selection.model || selection.effort) next[agent] = selection;
    else delete next[agent];
    change({ ...value, providers: next });
  };
  const save = async () => {
    if (busy.current) return;
    busy.current = true;
    setSaving(true);
    setResult(null);
    try {
      if (model.live && surfaces.models.validate) {
        for (const [agent, selection] of Object.entries(value.providers)) {
          const checked = await surfaces.models.validate(agent, selection);
          if (checked.status !== "ok") {
            setResult({ kind: "failed", message: `${agent}: ${checked.reason}` });
            return;
          }
          if (!checked.data.valid) {
            setResult({ kind: "failed", message: `${agent}: ${checked.data.detail}` });
            return;
          }
        }
      }
      const receipt = await surfaces.settings.setGroups({ dispatchDefaults: value });
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        // Keep the accepted values visible until the next settings read catches up.
        setFresh({ base: saved, value: receipt.snapshot.dispatchDefaults ?? value });
        setTyped(null);
        setResult({ kind: "saved" });
        model.reload();
      }
    } catch (error) {
      setResult({ kind: "failed", message: saveFailureText(asDaemonError(error)) });
    } finally {
      busy.current = false;
      setSaving(false);
    }
  };
  return (
    <section aria-labelledby={`${id}-heading`} className="settings-section">
      <h2 id={`${id}-heading`}>Agent dispatch defaults</h2>
      <p className="settings-intro">
        New tasks use these choices unless explicitly overridden. Each provider keeps its own model and effort.
        Chat, queued tasks, and running tasks are unchanged.
      </p>
      {catalog.status === "loading" && <p role="status">Loading the model catalog...</p>}
      {catalog.status !== "loading" && catalog.status !== "ok" && <Notice tone="warn">{catalog.reason}</Notice>}
      <Button onClick={() => setReloads((n) => n + 1)}>Refresh dispatch models</Button>
      <form onSubmit={guard(reason, () => void save())}>
        <div className="settings-row">
          <label htmlFor={`${id}-provider`}>Default provider</label>
          <select id={`${id}-provider`} value={value.defaultAgent ?? ""} disabled={locked || saving}
            onChange={(event) => change({ ...value, defaultAgent: event.target.value || null })}>
            <option value="">No default (tasks must choose a provider)</option>
            {value.defaultAgent && !providers.some((p) => p.id === value.defaultAgent) &&
              <option value={value.defaultAgent}>{value.defaultAgent} (unavailable)</option>}
            {providers.map((provider) => (
              <option key={provider.id} value={provider.id} disabled={!provider.launchable}>
                {provider.name}{provider.launchable ? "" : " (unavailable)"}
              </option>
            ))}
          </select>
        </div>
        {providers.map((provider) => {
          const selection = value.providers[provider.id] ?? {};
          const models = provider.models.status === "ok" ? provider.models.value : [];
          const selected = models.find((item) => item.id === selection.model);
          const efforts = effortsFor(provider, selected);
          const prefix = `${id}-${provider.id}`;
          return (
            <fieldset key={provider.id} disabled={locked || saving}>
              <legend>{provider.name}</legend>
              {!provider.launchable && <p className="settings-hint">{provider.detail ?? "This provider is unavailable."}</p>}
              <div className="settings-row">
                <label htmlFor={`${prefix}-model`}>{provider.name} model</label>
                <select id={`${prefix}-model`} value={selection.model ?? ""}
                  disabled={models.length === 0 && !selection.model}
                  onChange={(event) => choose(provider.id, event.target.value ? { model: event.target.value } : {})}>
                  <option value="">Provider default model</option>
                  {selection.model && !selected && <option value={selection.model}>{selection.model} (unavailable)</option>}
                  {models.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
                </select>
                {provider.models.status !== "ok" && <p className="settings-hint">{provider.models.reason}</p>}
              </div>
              <div className="settings-row">
                <label htmlFor={`${prefix}-effort`}>{provider.name} effort</label>
                <select id={`${prefix}-effort`} value={selection.effort ?? ""}
                  disabled={efforts.length === 0 && !selection.effort}
                  onChange={(event) => choose(provider.id, { ...selection, effort: event.target.value || undefined })}>
                  <option value="">Provider default effort</option>
                  {selection.effort && !efforts.includes(selection.effort) &&
                    <option value={selection.effort}>{selection.effort} (unavailable)</option>}
                  {efforts.map((effort) => <option key={effort} value={effort}>{effort}</option>)}
                </select>
                {efforts.length === 0 && <p className="settings-hint">No effort levels are reported for this choice.</p>}
              </div>
            </fieldset>
          );
        })}
        {Object.keys(value.providers).filter((agent) => catalog.status === "ok" && !providers.some((p) => p.id === agent)).map((agent) => (
          <Notice key={agent} tone="warn">
            Saved provider {agent} is unavailable.{" "}
            <Button disabled={locked || saving} onClick={() => choose(agent, {})}>Clear {agent} choice</Button>
          </Notice>
        ))}
        <div className="settings-actions">
          <SaveButton label="Save dispatch defaults" reason={reason} saving={saving} tried={tried} />
          <Button disabled={locked || saving} onClick={() => change({ defaultAgent: null, providers: {} })}>Clear dispatch defaults</Button>
          <div role="status" aria-live="polite" aria-label="Dispatch defaults save result" className="settings-result">
            {result?.kind === "saved" && <p>Saved. New tasks use these defaults.</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
            {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
          </div>
        </div>
      </form>
    </section>
  );
}
