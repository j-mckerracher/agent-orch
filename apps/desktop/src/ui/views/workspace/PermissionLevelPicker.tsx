import { useState } from "react";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { FullConfirmDialog } from "./FullConfirmDialog";
import { OLDER_DAEMON_NOTICE, PERMISSION_LEVELS, choiceBlock, permissionLabel } from "./permissionLevel";
import "./PermissionLevelPicker.css";

/**
 * The task form's level control (DPES-1126). Choosing Full opens a confirmation first, and the level changes only when
 * the user confirms. Extended and, on an older daemon, everything but Restricted stay listed with the reason they
 * cannot be chosen.
 */
export function PermissionLevelPicker({
  level,
  daemonAdvertises,
  agent,
  onChange,
}: {
  level: PermissionLevel;
  daemonAdvertises: boolean;
  /** The agent the task names, or undefined when the daemon routes it. */
  agent: string | undefined;
  onChange: (level: PermissionLevel) => void;
}) {
  const [confirmingFull, setConfirmingFull] = useState(false);
  const pick = (next: PermissionLevel) => {
    if (next !== "full") {
      onChange(next);
      return;
    }
    setConfirmingFull(true);
  };
  return (
    <>
      <label className="ws-field">
        Permission level
        <select value={level} onChange={(event) => pick(event.target.value as PermissionLevel)}>
          {PERMISSION_LEVELS.map((item) => {
            const block = choiceBlock(item, daemonAdvertises);
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
          title="Run this task at Full?"
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

/** Below the task form's controls: the older-daemon notice, when the daemon predates permission levels. */
export function PermissionLevelNotes({ olderDaemon }: { olderDaemon: boolean }) {
  if (!olderDaemon) return null;
  return (
    <div className="ws-perm-notes">
      <p role="status" className="ws-small">
        {OLDER_DAEMON_NOTICE}
      </p>
    </div>
  );
}
