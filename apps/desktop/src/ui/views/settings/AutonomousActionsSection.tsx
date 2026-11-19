/** DPES-1108 Phase 5: the "Acting on its own" section of the Settings page (`autonomous_actions`). */
import { useEffect, useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { useGate, useServices } from "../../services";
import { Notice } from "../../widgets";
import { FIXTURE_SAVE_TEXT, NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import { SaveButton, useSaveGuard } from "./SaveBar";
import type { SaveResult, SettingsModel } from "./useSettings";

export const AUTONOMOUS_LABEL = "Let the orchestrator act on its own";
export const AUTONOMOUS_CONSEQUENCE =
  "When this is on, the orchestrator starts tasks, retries them, sends messages to waiting tasks and organizes workstreams by itself after your messages. Cancelling or revising a task, merging, removing a project and holding a decision are never done on its own. When this is off, every action waits for your approval, unless a standing approval covers it.";
export const AUTONOMOUS_SAVE = "Save acting on its own";

export function AutonomousActionsSection({ model }: { model: SettingsModel }) {
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

  // An older daemon does not send the setting: nothing to show.
  if (snapshot === null || snapshot.autonomousActions === undefined) return null;
  const saved = fresh ?? snapshot.autonomousActions;
  const checked = typed ?? saved;
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setResult(null);
    try {
      const receipt = await surfaces.settings.setGroups({ autonomousActions: checked });
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        setFresh(receipt.snapshot.autonomousActions ?? checked);
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
      <h2 id={headingId}>Acting on its own</h2>
      <p className="settings-intro">{AUTONOMOUS_CONSEQUENCE}</p>
      <form noValidate onSubmit={guard(reason, () => void save())}>
        <div className="settings-field">
          <input
            id={checkId}
            type="checkbox"
            role="switch"
            checked={checked}
            disabled={locked}
            onChange={(event) => setTyped(event.target.checked)}
          />{" "}
          <label htmlFor={checkId}>{AUTONOMOUS_LABEL}</label>
        </div>
        <div className="settings-actions">
          <SaveButton label={AUTONOMOUS_SAVE} reason={reason} saving={saving} tried={tried} />
          <div role="status" aria-live="polite" aria-label={`${AUTONOMOUS_SAVE} result`} className="settings-result">
            {result?.kind === "saved" && <p>Saved.</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
            {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
          </div>
        </div>
      </form>
    </section>
  );
}
