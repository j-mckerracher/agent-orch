/** The "Home directory" section of the Settings page (`home_directory`): where a task runs when no project is clear. */
import { useEffect, useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { useGate, useServices } from "../../services";
import { Notice } from "../../widgets";
import { FIXTURE_SAVE_TEXT, NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import { SaveButton, useSaveGuard } from "./SaveBar";
import type { SaveResult, SettingsModel } from "./useSettings";

export const HOME_DIRECTORY_LABEL = "Home directory";
export const HOME_DIRECTORY_EXAMPLE = "~/Code";
export const HOME_DIRECTORY_INTRO =
  "Where a task runs when it is not clear which project it belongs to, such as a task you start from the global orchestrator. An Implement task that runs here writes to this folder directly: it gets no separate working copy, and nothing is reviewed or landed afterwards.";
export const HOME_DIRECTORY_HINT = `Enter the full path of a folder on this machine, for example ${HOME_DIRECTORY_EXAMPLE}. Leave it empty to set none.`;
export const HOME_DIRECTORY_REFUSED_TEXT = "That folder was not found. Enter the full path of a folder that exists on this machine.";

export function HomeDirectorySection({ model }: { model: SettingsModel }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const inputId = useId();
  const hintId = useId();
  const headingId = useId();
  const snapshot = model.load.status === "ready" ? model.load.snapshot : null;
  // What the daemon answered a save with, until the page reads a newer snapshot.
  const [fresh, setFresh] = useState<{ path: string | null } | null>(null);
  const [typed, setTyped] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<SaveResult | null>(null);
  const { tried, guard } = useSaveGuard();
  useEffect(() => setFresh(null), [snapshot]);

  // An older daemon does not send the setting: nothing to show, and the load section already says what it can.
  if (snapshot === null || snapshot.homeDirectory === undefined) return null;
  const saved = fresh ? fresh.path : snapshot.homeDirectory;
  const value = typed ?? saved ?? "";
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setResult(null);
    try {
      // An empty field clears the setting.
      const homeDirectory = value.trim() === "" ? null : value.trim();
      const receipt = await surfaces.settings.setGroups({ homeDirectory });
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        // The daemon answers with the folder it resolved, so `~/Code` comes back as the full path.
        setFresh({ path: receipt.snapshot.homeDirectory ?? homeDirectory });
        setTyped(null);
        setResult({ kind: "saved" });
        model.reload();
      }
    } catch (error) {
      const daemonError = asDaemonError(error);
      setResult({
        kind: "failed",
        message: daemonError.kind === "invalid-params" ? HOME_DIRECTORY_REFUSED_TEXT : saveFailureText(daemonError),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>Home directory</h2>
      <p className="settings-intro">{HOME_DIRECTORY_INTRO}</p>
      <form noValidate onSubmit={guard(reason, () => void save())}>
        <div className="settings-row">
          <label htmlFor={inputId}>{HOME_DIRECTORY_LABEL}</label>
          <input
            id={inputId}
            type="text"
            className="settings-path"
            value={value}
            placeholder={HOME_DIRECTORY_EXAMPLE}
            disabled={locked}
            autoComplete="off"
            spellCheck={false}
            aria-describedby={hintId}
            onChange={(event) => setTyped(event.target.value)}
          />
          <p id={hintId} className="settings-hint">
            {HOME_DIRECTORY_HINT}
          </p>
        </div>
        <div className="settings-actions">
          <SaveButton label="Save home directory" reason={reason} saving={saving} tried={tried} />
          <div role="status" aria-live="polite" aria-label="Save home directory result" className="settings-result">
            {result?.kind === "saved" && <p>{saved === null ? "Cleared. A task with no clear project will ask where to run." : `Saved. A task with no clear project now runs in ${saved}.`}</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
            {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
          </div>
        </div>
      </form>
    </section>
  );
}
