import { useState } from "react";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { Dialog } from "../../Dialog";
import { Button } from "../../theme";
import {
  AUTO_AGENT_TEXT,
  AUTO_TEXT,
  FULL_AGENT_TEXT,
  FULL_REVIEW_TEXT,
  FULL_SAVE_TEXT,
  FULL_SUMMARY,
  INDICATOR,
  OLDER_DAEMON_NOTICE,
  PERMISSION_LEVELS,
  agentTexts,
  chatChoiceBlock,
  permissionLabel,
} from "./permissionLevel";
import "./PermissionLevelPicker.css";

/**
 * The level dropdown in the composer row (DPES-1105). Choosing Full opens a confirmation first, and the
 * level changes only when the user confirms. Extended and, on an older daemon, everything but Restricted
 * stay listed with the reason they cannot be chosen.
 */
export function PermissionLevelPicker({
  level,
  daemonAdvertises,
  agent,
  onChange,
}: {
  level: PermissionLevel;
  daemonAdvertises: boolean;
  /** The agent the message names, or undefined when the daemon routes it. */
  agent: string | undefined;
  onChange: (level: PermissionLevel) => void;
}) {
  const [confirmingFull, setConfirmingFull] = useState(false);
  const pick = (next: PermissionLevel) => {
    if (next === "full") setConfirmingFull(true);
    else onChange(next);
  };
  return (
    <>
      <select
        className="ws-perm-select"
        aria-label="Permission level"
        value={level}
        onChange={(event) => pick(event.target.value as PermissionLevel)}
      >
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
 * Below the composer row: the always-visible indicator with its one-click switch to Restricted, the older-daemon
 * notice, and the level the daemon reports for the last turn. A mismatch with the level sent is an error.
 */
export function PermissionLevelNotes({
  level,
  olderDaemon,
  agent,
  lastTurn,
  onChange,
}: {
  level: PermissionLevel;
  olderDaemon: boolean;
  agent: string | undefined;
  lastTurn: LastTurnLevel | null;
  onChange: (level: PermissionLevel) => void;
}) {
  return (
    <div className="ws-perm-notes">
      {olderDaemon && (
        <p role="status" className="ws-small">
          {OLDER_DAEMON_NOTICE}
        </p>
      )}
      {level !== "restricted" && (
        <div className="ws-perm-indicator">
          <p className="ws-small">{INDICATOR[level]}</p>
          <Button variant="quiet" onClick={() => onChange("restricted")}>
            Switch to Restricted
          </Button>
          {level === "auto" && (
            <details className="ws-small">
              <summary>What automatic review does</summary>
              <p>{AUTO_TEXT}</p>
              {agentTexts(AUTO_AGENT_TEXT, agent).map(({ agent: id, text }) => (
                <p key={id}>{text}</p>
              ))}
            </details>
          )}
        </div>
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
