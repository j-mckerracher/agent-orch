/** DPES-1105 Phase 4: the "Pull requests from chat" section of the Settings page (`chat_pr_delivery`). */
import { useEffect, useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { useGate, useServices } from "../../services";
import { Button } from "../../theme";
import { Notice } from "../../widgets";
import { FIXTURE_SAVE_TEXT, NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import type { SaveResult, SettingsModel } from "./useSettings";

export const CHAT_PR_LABEL = "Let the orchestrator propose tasks that end in a pull request. You still approve each task.";
export const CHAT_PR_CONSEQUENCE =
  "When this is off, chat can only propose a report or a review on this machine. When it is on, a task you approve from chat can end in a pull request, in a project set to a pull request mode.";
export const CHAT_PR_SAVED_TEXT = "Saved. New proposals follow this setting; proposals already waiting are checked again when you approve them.";
export const CHAT_PR_UNCHANGED_REASON = "Change the setting to save.";

export function ChatPullRequestSection({ model }: { model: SettingsModel }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const checkId = useId();
  const reasonId = useId();
  const headingId = useId();
  const snapshot = model.load.status === "ready" ? model.load.snapshot : null;
  // What the daemon answered a save with, until the page reads a newer snapshot.
  const [fresh, setFresh] = useState<boolean | null>(null);
  const [typed, setTyped] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<SaveResult | null>(null);
  useEffect(() => setFresh(null), [snapshot]);

  // An older daemon does not send the setting: nothing to show, and the load section already says what it can.
  if (snapshot === null || snapshot.chatPrDelivery === undefined) return null;
  const saved = fresh ?? snapshot.chatPrDelivery;
  const checked = typed ?? saved;
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason =
    model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : checked === saved ? CHAT_PR_UNCHANGED_REASON : null;

  const save = async () => {
    if (reason !== null || saving) return;
    setSaving(true);
    setResult(null);
    try {
      const receipt = await surfaces.settings.setGroups({ chatPrDelivery: checked });
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        setFresh(receipt.snapshot.chatPrDelivery ?? checked);
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
      <h2 id={headingId}>Pull requests from chat</h2>
      <p className="settings-intro">{CHAT_PR_CONSEQUENCE}</p>
      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <div className="settings-field">
          <input
            id={checkId}
            type="checkbox"
            checked={checked}
            disabled={locked}
            aria-describedby={locked ? reasonId : undefined}
            onChange={(event) => setTyped(event.target.checked)}
          />{" "}
          <label htmlFor={checkId}>{CHAT_PR_LABEL}</label>
        </div>
        <div className="settings-actions">
          <Button
            type="submit"
            variant="primary"
            disabled={reason !== null}
            aria-busy={saving}
            aria-describedby={reason === null ? undefined : reasonId}
          >
            Save pull request setting
          </Button>
          {reason !== null && (
            <p id={reasonId} className="settings-reason">
              {reason}
            </p>
          )}
          <div role="status" aria-live="polite" aria-label="Save pull request setting result" className="settings-result">
            {result?.kind === "saved" && <p>{CHAT_PR_SAVED_TEXT}</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
            {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
          </div>
        </div>
      </form>
    </section>
  );
}
