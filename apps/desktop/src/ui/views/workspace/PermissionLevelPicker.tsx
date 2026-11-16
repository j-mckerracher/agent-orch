import { useState } from "react";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { Dialog } from "../../Dialog";
import { Button } from "../../theme";
import {
  FULL_AGENT_TEXT,
  FULL_REVIEW_TEXT,
  FULL_SAVE_TEXT,
  FULL_SUMMARY,
  OLDER_DAEMON_NOTICE,
  PERMISSION_LEVELS,
  agentTexts,
  chatChoiceBlock,
  permissionLabel,
} from "./permissionLevel";
import "./PermissionLevelPicker.css";

/**
 * The level control in the model picker's panel (DPES-1105). Choosing Full opens a confirmation first, and the
 * level changes only when the user confirms. Extended and, on an older daemon, everything but Restricted
 * stay listed with the reason they cannot be chosen.
 */
export function PermissionLevelPicker({
  level,
  daemonAdvertises,
  agent,
  onChange,
  closePanel,
}: {
  level: PermissionLevel;
  daemonAdvertises: boolean;
  /** The agent the message names, or undefined when the daemon routes it. */
  agent: string | undefined;
  onChange: (level: PermissionLevel) => void;
  /**
   * Closes the panel before the Full confirmation opens. The panel is hidden while the dialog is up, so the dialog
   * would otherwise have no visible control to return focus to.
   */
  closePanel: () => void;
}) {
  const [confirmingFull, setConfirmingFull] = useState(false);
  const pick = (next: PermissionLevel) => {
    if (next !== "full") {
      onChange(next);
      return;
    }
    closePanel();
    setConfirmingFull(true);
  };
  return (
    <>
      <label className="ws-field">
        Permission level
        <select value={level} onChange={(event) => pick(event.target.value as PermissionLevel)}>
          {PERMISSION_LEVELS.map((item) => {
            const block = chatChoiceBlock(item, daemonAdvertises);
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
        <Dialog
          title="Run this chat at Full?"
          onClose={() => setConfirmingFull(false)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setConfirmingFull(false)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                onClick={() => {
                  setConfirmingFull(false);
                  onChange("full");
                }}
              >
                Use Full
              </Button>
            </>
          }
        >
          <p>{FULL_SUMMARY}</p>
          {agentTexts(FULL_AGENT_TEXT, agent).map(({ agent: id, text }) => (
            <p key={id}>{text}</p>
          ))}
          <p>{FULL_REVIEW_TEXT}</p>
          <p>{FULL_SAVE_TEXT}</p>
        </Dialog>
      )}
    </>
  );
}

/** The daemon-reported level of the last turn, and the level this session sent it at, when it still knows. */
export interface LastTurnLevel {
  daemonLevel: PermissionLevel | null;
  sentLevel: PermissionLevel | null;
}

/**
 * Below the composer row: the older-daemon notice and the level the daemon reports for the last turn. A mismatch with
 * the level sent is an error.
 */
export function PermissionLevelNotes({ olderDaemon, lastTurn }: { olderDaemon: boolean; lastTurn: LastTurnLevel | null }) {
  return (
    <div className="ws-perm-notes">
      {olderDaemon && (
        <p role="status" className="ws-small">
          {OLDER_DAEMON_NOTICE}
        </p>
      )}
      {lastTurn && <LastTurn {...lastTurn} />}
    </div>
  );
}

function LastTurn({ daemonLevel, sentLevel }: LastTurnLevel) {
  if (daemonLevel === null) {
    return <p className="ws-small">The daemon has not recorded a permission level for the last turn.</p>;
  }
  if (sentLevel !== null && sentLevel !== daemonLevel) {
    return (
      <p role="alert" className="ws-small">
        The daemon reports the last reply ran at {permissionLabel(daemonLevel)}, not {permissionLabel(sentLevel)}. Check
        that reply before you act on it.
      </p>
    );
  }
  return <p className="ws-small">The daemon reports the last reply ran at {permissionLabel(daemonLevel)}.</p>;
}
