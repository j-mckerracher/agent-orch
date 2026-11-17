/** DPES-1105 Phase 6b: the "Read-only task tools" section of the Settings page (`mcp.enabled`, off by default). */
import { useEffect, useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { useGate, useServices } from "../../services";
import { Notice } from "../../widgets";
import { FIXTURE_SAVE_TEXT, NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import { SaveButton, useSaveGuard } from "./SaveBar";
import type { SaveResult, SettingsModel } from "./useSettings";

export const MCP_LABEL = "Give the orchestrator chat read-only task tools.";
export const MCP_CONSEQUENCE =
  "When on, the orchestrator chat can look up a task's state and its saved report while it answers. The tools only read; they cannot change anything. Applies from the next message.";
export const MCP_SAVED_TEXT = "Saved. The next message in the orchestrator chat follows this setting.";

export function McpToolsSection({ model }: { model: SettingsModel }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const checkId = useId();
  const headingId = useId();
  const snapshot = model.load.status === "ready" ? model.load.snapshot : null;
  // What the daemon answered a save with, until the page reads a newer snapshot.
  const [fresh, setFresh] = useState<boolean | null>(null);
  const [typed, setTyped] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<SaveResult | null>(null);
  const { tried, guard } = useSaveGuard();
  useEffect(() => setFresh(null), [snapshot]);

  // An older daemon does not send the setting: nothing to show, and the load section already says what it can.
  if (snapshot === null || snapshot.mcpEnabled === undefined) return null;
  const saved = fresh ?? snapshot.mcpEnabled;
  const checked = typed ?? saved;
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setResult(null);
    try {
      const receipt = await surfaces.settings.setGroups({ mcpEnabled: checked });
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        setFresh(receipt.snapshot.mcpEnabled ?? checked);
        setTyped(null);
        setResult({ kind: "saved" });
        model.reload();
      }
    } catch (error) {
      setResult({ kind: "failed", message: saveFailureText(asDaemonError(error)) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>Read-only task tools for the orchestrator chat</h2>
      <p className="settings-intro">{MCP_CONSEQUENCE}</p>
      <form noValidate onSubmit={guard(reason, () => void save())}>
        <div className="settings-field">
          <input
            id={checkId}
            type="checkbox"
            checked={checked}
            disabled={locked}
            onChange={(event) => setTyped(event.target.checked)}
          />{" "}
          <label htmlFor={checkId}>{MCP_LABEL}</label>
        </div>
        <div className="settings-actions">
          <SaveButton label="Save read-only tools setting" reason={reason} saving={saving} tried={tried} />
          <div role="status" aria-live="polite" aria-label="Save read-only tools setting result" className="settings-result">
            {result?.kind === "saved" && <p>{MCP_SAVED_TEXT}</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
            {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
          </div>
        </div>
      </form>
    </section>
  );
}
