import { useState } from "react";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { FullConfirmDialog } from "./FullConfirmDialog";
import { OLDER_DAEMON_NOTICE, PERMISSION_LEVELS, choiceBlock, permissionLabel, type PermissionSurface } from "./permissionLevel";
import "./PermissionLevelPicker.css";

const FULL_TITLE: Record<PermissionSurface, string> = {
  chat: "Run this chat at Full?",
  task: "Run this task at Full?",
};

/**
 * The level control in the model picker's panel (DPES-1105) and in the task form (DPES-1126). Choosing Full opens
 * a confirmation first, and the level changes only when the user confirms. Extended and, on an older daemon,
 * everything but Restricted stay listed with the reason they cannot be chosen.
 */
export function PermissionLevelPicker({
  level,
  daemonAdvertises,
  agent,
  surface,
  onChange,
  closePanel,
}: {
  level: PermissionLevel;
  daemonAdvertises: boolean;
  /** The agent the message names, or undefined when the daemon routes it. */
  agent: string | undefined;
  surface: PermissionSurface;
  onChange: (level: PermissionLevel) => void;
  /**
   * Closes the panel before the Full confirmation opens. The panel is hidden while the dialog is up, so the dialog
   * would otherwise have no visible control to return focus to. A surface with no panel leaves it out.
   */
  closePanel?: () => void;
}) {
  const [confirmingFull, setConfirmingFull] = useState(false);
  const pick = (next: PermissionLevel) => {
    if (next !== "full") {
      onChange(next);
      return;
    }
    closePanel?.();
    setConfirmingFull(true);
  };
  return (
    <>
      <label className="ws-field">
        Permission level
        <select value={level} onChange={(event) => pick(event.target.value as PermissionLevel)}>
          {PERMISSION_LEVELS.map((item) => {
            const block = choiceBlock(item, daemonAdvertises, surface);
            return (
              <option key={item} value={item} disabled={block !== null}>
                {permissionLabel(item)}
                {block === null ? "" : ` (${block})`}
              </option>
            );
          })}
        </select>
      </label>
      {confirmingFull && (
        <FullConfirmDialog
          title={FULL_TITLE[surface]}
          agent={agent}
          onCancel={() => setConfirmingFull(false)}
          onConfirm={() => {
            setConfirmingFull(false);
            onChange("full");
          }}
        />
      )}
    </>
  );
}

/** The daemon-reported level of the last turn, and the level this session sent it at, when it still knows. */
export interface LastTurnLevel {
  daemonLevel: PermissionLevel | null;
  sentLevel: PermissionLevel | null;
  /** `wake` for an automatic update the user did not send. Absent from an older daemon. */
  trigger?: "user" | "wake";
}

/**
 * Below the composer row: the older-daemon notice, and an error when the daemon reports a different level for the last
 * reply than the one sent. The routine "the daemon reports level X" line lives in the picker panel (`LastTurnNote`).
 */
export function PermissionLevelNotes({ olderDaemon, lastTurn }: { olderDaemon: boolean; lastTurn: LastTurnLevel | null }) {
  const mismatch =
    lastTurn !== null &&
    lastTurn.daemonLevel !== null &&
    lastTurn.trigger !== "wake" &&
    lastTurn.sentLevel !== null &&
    lastTurn.sentLevel !== lastTurn.daemonLevel
      ? { daemonLevel: lastTurn.daemonLevel, sentLevel: lastTurn.sentLevel }
      : null;
  if (!olderDaemon && mismatch === null) return null;
  return (
    <div className="ws-perm-notes">
      {olderDaemon && (
        <p role="status" className="ws-small">
          {OLDER_DAEMON_NOTICE}
        </p>
      )}
      {mismatch && (
        <p role="alert" className="ws-small">
          The daemon reports the last reply ran at {permissionLabel(mismatch.daemonLevel)}, not {permissionLabel(mismatch.sentLevel)}.
          Check that reply before you act on it.
        </p>
      )}
    </div>
  );
}

/** The level the daemon reports for the last turn, in the picker panel. Nothing for an automatic update or a mismatch (that is an alert). */
export function LastTurnNote({ lastTurn }: { lastTurn: LastTurnLevel | null }) {
  if (lastTurn === null) return null;
  const { daemonLevel, sentLevel, trigger } = lastTurn;
  if (daemonLevel === null) return <p className="ws-small">The daemon has not recorded a permission level for the last turn.</p>;
  if (trigger === "wake" || (sentLevel !== null && sentLevel !== daemonLevel)) return null;
  return <p className="ws-small">The daemon reports the last reply ran at {permissionLabel(daemonLevel)}.</p>;
}
