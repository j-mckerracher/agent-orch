import { useId, useState } from "react";
import type { ModeGroup, ModeOption, ModeSelection } from "../../../sources/surfaces/types";
import { usePopover } from "../../usePopover";
import { FullConfirmDialog } from "./FullConfirmDialog";
import { effectiveMode, selectedOptions, type ModeDisabled } from "./modeChoice";
import "./ModeDropdown.css";

const FULL_TITLE = "Run this chat at Full?";

/**
 * The chat's mode control, first in the composer row (DPES-1105). It lists each group under the agent's own names, with
 * one choice per group. An option that needs confirmation opens the Full dialog, and it applies only when the user
 * confirms. The trigger stays usable when nothing can be picked, so its reason can still be read.
 */
export function ModeDropdown({
  agent,
  groups,
  selection,
  disabled,
  onChange,
}: {
  /** The agent the message names, or undefined when the daemon routes it. */
  agent: string | undefined;
  groups: ModeGroup[];
  /** The option the user chose per group. A group left out takes its default. */
  selection: ModeSelection;
  disabled: ModeDisabled | null;
  onChange: (next: ModeSelection) => void;
}) {
  const popover = usePopover();
  const ids = useId();
  const [confirming, setConfirming] = useState<{ groupId: string; option: ModeOption } | null>(null);
  const effective = effectiveMode(groups, selection);
  const text = disabled?.label ?? (selectedOptions(groups, effective).map((option) => option.label).join(" · ") || "Default mode");

  const choose = (group: ModeGroup, option: ModeOption) => {
    if (!option.needsConfirmation) {
      onChange({ ...selection, [group.id]: option.id });
      return;
    }
    popover.close();
    setConfirming({ groupId: group.id, option });
  };

  return (
    <div ref={popover.rootRef} className="ws-popover-root ws-mode-root" onKeyDown={popover.onKeyDown} onBlur={popover.onBlur}>
      <button
        ref={popover.buttonRef}
        type="button"
        className="ws-chip-button"
        aria-expanded={popover.open}
        aria-controls={`${ids}-menu`}
        aria-label={`Chat mode: ${text}`}
        title={text}
        onClick={() => popover.setOpen((value) => !value)}
      >
        <span>{text}</span>
        <span aria-hidden="true">⌄</span>
      </button>
      <div id={`${ids}-menu`} className="ws-popover ws-mode-panel" hidden={!popover.open}>
        <p className="ws-menu-title">Chat mode</p>
        {disabled && (
          <p role="status" className="ws-small">
            {disabled.reason}
          </p>
        )}
        {groups.map((group) => (
          <fieldset key={group.id} className="ws-mode-group">
            <legend>{group.label}</legend>
            {group.options.map((option) => (
              <label key={option.id} className="ws-mode-option">
                <input
                  type="radio"
                  name={`${ids}-${group.id}`}
                  checked={effective[group.id] === option.id}
                  disabled={!option.available}
                  onChange={() => choose(group, option)}
                />
                <span>
                  <span>{option.label}</span>
                  <span className="ws-small">{option.available ? option.description : option.unavailableReason}</span>
                </span>
              </label>
            ))}
          </fieldset>
        ))}
      </div>
      {confirming && (
        <FullConfirmDialog
          title={FULL_TITLE}
          agent={agent}
          onCancel={() => setConfirming(null)}
          onConfirm={() => {
            onChange({ ...selection, [confirming.groupId]: confirming.option.id });
            setConfirming(null);
          }}
        />
      )}
    </div>
  );
}

/** The last reply's mode, as the daemon reports it and as this session sent it. Labels are null when the catalog cannot name them. */
export interface LastReplyMode {
  reported: string;
  sent: string | null;
  differs: boolean;
}

/** Below the composer row: what the last reply ran in, or an alert when the daemon reports a different mode than was sent. */
export function ModeNotes({ last }: { last: LastReplyMode | null }) {
  if (last === null) return null;
  return (
    <div className="ws-perm-notes">
      {last.differs ? (
        <p role="alert" className="ws-small">
          The daemon reports the last reply ran in {last.reported}, not {last.sent}. Check that reply before you act on it.
        </p>
      ) : (
        <p className="ws-small">Last reply ran in {last.reported}.</p>
      )}
    </div>
  );
}
