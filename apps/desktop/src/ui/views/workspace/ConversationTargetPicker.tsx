import { useId, useRef, useState, type KeyboardEvent } from "react";
import { Badge } from "../../theme";
import { filterTargets, type TargetOption } from "./targets";
import { usePopover } from "./usePopover";

export interface ConversationTargetPickerProps {
  options: readonly TargetOption[];
  selectedKey: string;
  onSelect: (option: TargetOption) => void;
}

/**
 * Searchable conversation-target dropdown that lives inside the composer.
 *
 * Keyboard: Enter/Space on the button toggles; opening moves focus to the search box;
 * ArrowDown from the search box enters the radio list; arrow keys move and select inside the
 * list (native radio behaviour); Escape closes and returns focus to the button. The result
 * count and the empty result are announced through one always-mounted status region, because a
 * live region that appears together with its text is not reliably announced.
 */
export function ConversationTargetPicker({ options, selectedKey, onSelect }: ConversationTargetPickerProps) {
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const popover = usePopover(() => searchRef.current?.focus());
  const ids = useId();
  const panelId = `${ids}-panel`;
  const searchId = `${ids}-search`;
  const statusId = `${ids}-status`;
  const shown = filterTargets(options, query);
  const current = options.find((option) => option.key === selectedKey);

  const enterList = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "ArrowDown") return;
    const radios = listRef.current?.querySelectorAll<HTMLInputElement>('input[type="radio"]');
    if (!radios || radios.length === 0) return;
    event.preventDefault();
    (Array.from(radios).find((radio) => radio.checked) ?? radios[0]).focus();
  };

  const status =
    query.trim() === ""
      ? ""
      : shown.length === 0
        ? "No matching conversations."
        : `${shown.length} matching ${shown.length === 1 ? "conversation" : "conversations"}.`;

  return (
    <div ref={popover.rootRef} className="ws-popover-root" onKeyDown={popover.onKeyDown} onBlur={popover.onBlur}>
      <button
        ref={popover.buttonRef}
        type="button"
        className="ws-chip-button"
        aria-expanded={popover.open}
        aria-controls={panelId}
        onClick={() => popover.setOpen((value) => !value)}
      >
        <span>Choose conversation target</span>
        <span className="ws-chip-current">· {current?.label ?? "none selected"}</span>
        <span aria-hidden="true">⌄</span>
      </button>
      <div id={panelId} className="ws-popover ws-target-panel" hidden={!popover.open}>
        <label className="ws-search-label" htmlFor={searchId}>
          Search conversations
        </label>
        <input
          ref={searchRef}
          id={searchId}
          type="search"
          autoComplete="off"
          placeholder="Search orchestrators and coordinators"
          value={query}
          aria-describedby={statusId}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={enterList}
        />
        <div ref={listRef} role="radiogroup" aria-label="Conversation targets" className="ws-target-options">
          {shown.map((option) => (
            <label key={option.key} className="ws-target-option">
              <input
                type="radio"
                name={`${ids}-target`}
                checked={option.key === selectedKey}
                onChange={() => onSelect(option)}
              />
              <span>
                {option.label}
                {option.placeholder && (
                  <>
                    {" "}
                    <Badge tone="placeholder">PLACEHOLDER DATA (D10)</Badge>
                  </>
                )}
                {option.target.kind === "project" && <span className="ws-target-note"> · notes, no reply</span>}
              </span>
            </label>
          ))}
        </div>
        <p id={statusId} role="status" className="ws-target-status">
          {status}
        </p>
        <p className="ws-small">Each target retains its own conversation and local draft.</p>
      </div>
    </div>
  );
}
