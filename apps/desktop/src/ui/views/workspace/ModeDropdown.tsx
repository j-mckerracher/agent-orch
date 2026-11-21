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
          <fieldset key={group.id} className="ws-mode-group" disabled={disabled !== null}>
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
