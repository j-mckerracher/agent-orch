/** The "Home directory" section of Settings > Repositories (`home_directory`): where a task runs when no project is clear. */
import { useEffect, useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { useGate, useServices } from "../../services";
import { Disclosure } from "../../theme";
import { outcomeOf, usePageSave } from "./pageSave";
import { NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import type { SettingsModel } from "./useSettings";

export const HOME_DIRECTORY_LABEL = "Home directory";
export const HOME_DIRECTORY_EXAMPLE = "~/Code";
export const HOME_DIRECTORY_INTRO = "Where a task runs when it is not clear which project it belongs to.";
export const HOME_DIRECTORY_DETAIL =
  "For example a task you start from the global orchestrator. An Implement task that runs here writes to this folder directly: it gets no separate working copy, and nothing is reviewed or landed afterwards.";
export const HOME_DIRECTORY_HINT = `Enter the full path of a folder on this machine, for example ${HOME_DIRECTORY_EXAMPLE}. Leave it empty to set none.`;
export const HOME_DIRECTORY_REFUSED_TEXT = "That folder was not found. Enter the full path of a folder that exists on this machine.";

export function HomeDirectorySection({ model }: { model: SettingsModel }) {
  const snapshot = model.load.status === "ready" ? model.load.snapshot : null;
  // An older daemon does not send the setting: nothing to show, and the load section already says what it can.
  if (snapshot === null || snapshot.homeDirectory === undefined) return null;
  return <HomeDirectoryForm model={model} snapshotPath={snapshot.homeDirectory} snapshotKey={snapshot} />;
}

function HomeDirectoryForm({ model, snapshotPath, snapshotKey }: { model: SettingsModel; snapshotPath: string | null; snapshotKey: unknown }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const inputId = useId();
  const hintId = useId();
  const headingId = useId();
  // What the daemon answered a save with, until the page reads a newer snapshot.
  const [fresh, setFresh] = useState<{ path: string | null } | null>(null);
  const [typed, setTyped] = useState<string | null>(null);
  useEffect(() => setFresh(null), [snapshotKey]);

  const saved = fresh ? fresh.path : snapshotPath;
  const value = typed ?? saved ?? "";
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const reason = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;

  usePageSave(reason, async () => {
    try {
      // An empty field clears the setting.
      const homeDirectory = value.trim() === "" ? null : value.trim();
      const receipt = await surfaces.settings.setGroups({ homeDirectory });
      if (receipt.simulated) return outcomeOf({ kind: "fixture" }, "");
      // The daemon answers with the folder it resolved, so `~/Code` comes back as the full path.
      const path = receipt.snapshot.homeDirectory ?? homeDirectory;
      setFresh({ path });
      setTyped(null);
      model.reload();
      return outcomeOf(
        { kind: "saved" },
        path === null ? "Cleared. A task with no clear project will ask where to run." : `Saved. A task with no clear project now runs in ${path}.`,
      );
    } catch (error) {
      const daemonError = asDaemonError(error);
      return outcomeOf(
        { kind: "failed", message: daemonError.kind === "invalid-params" ? HOME_DIRECTORY_REFUSED_TEXT : saveFailureText(daemonError) },
        "",
      );
    }
  });

  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h3 id={headingId}>Home directory</h3>
      <p className="settings-intro">{HOME_DIRECTORY_INTRO}</p>
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
      <Disclosure summary="More about the home directory">
        <p className="settings-hint">{HOME_DIRECTORY_DETAIL}</p>
      </Disclosure>
    </section>
  );
}
