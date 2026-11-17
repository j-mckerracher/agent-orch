import { useId, useRef, useState, type KeyboardEvent } from "react";
import { Badge } from "../../theme";
import { badgeLabel, targetNameWithBadge } from "./monitoringModel";
import { filterTargets, type TargetOption } from "./targets";
import { usePopover } from "../../usePopover";

export interface ConversationTargetPickerProps {
  options: readonly TargetOption[];
  selectedKey: string;
  onSelect: (option: TargetOption) => void;
  /**
   * The "Add coordinator…" entry (DPES-1105). `reason` explains why it is unavailable; while it is
   * set the entry is shown disabled and does nothing.
   */
  addCoordinator?: { onAdd: () => void; reason: string | null };
  /** Unseen automatic updates in the Global conversation (DPES-1105 Phase 3); the Global option shows a badge when above 0. */
  globalUnseen?: number;
}

const ARROW_KEYS = new Set(["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"]);

/**
 * Searchable conversation-target dropdown that lives inside the composer.
 *
 * Keyboard: Enter/Space on the button toggles; opening moves focus to the search box;
 * Enter in the search box does nothing (it must not submit the composer form);
 * ArrowDown from the search box enters the radio list; arrow keys move and select inside the
 * list (native radio behavior) and keep the panel open; Enter or Space on a radio, or a pointer
 * choice, selects it and closes the panel; Escape closes without changing the choice. Every close
 * returns focus to the button. The result count and the empty result are announced through one
 * always-mounted status region, because a live region that appears together with its text is not
 * reliably announced.
 */
export function ConversationTargetPicker({ options, selectedKey, onSelect, addCoordinator, globalUnseen = 0 }: ConversationTargetPickerProps) {
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  // True from an arrow key press until its release. The radio the arrow moves to is then selected
  // by the browser, and the panel stays open; a pointer press or any other key clears it.
  const arrowing = useRef(false);
  const popover = usePopover(() => searchRef.current?.focus());
  const ids = useId();
  const panelId = `${ids}-panel`;
  const searchId = `${ids}-search`;
  const statusId = `${ids}-status`;
  const shown = filterTargets(options, query);
  const current = options.find((option) => option.key === selectedKey);

  const select = (option: TargetOption) => {
    if (option.key !== selectedKey) onSelect(option);
  };

  const searchKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // This input sits inside the composer form, so Enter left alone would submit the form and send the draft.
    // Enter that confirms an IME composition is left to the IME.
    if (event.key === "Enter" && !event.nativeEvent.isComposing && event.keyCode !== 229) {
      event.preventDefault();
      return;
    }
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
          onKeyDown={searchKeyDown}
        />
        <div
          ref={listRef}
          role="radiogroup"
          aria-label="Conversation targets"
          className="ws-target-options"
          onPointerDown={() => {
            arrowing.current = false;
          }}
          onKeyDown={(event) => {
            arrowing.current = ARROW_KEYS.has(event.key);
          }}
          onKeyUp={() => {
            arrowing.current = false;
          }}
        >
          {shown.map((option) => {
            const unseen = option.target.kind === "global" ? globalUnseen : 0;
            return (
            <label
              key={option.key}
              className="ws-target-option"
              onMouseDown={(event) => {
                // Keep focus here until the label activates its radio; focusing the scrollable
                // workspace ancestor would blur and close the popover before the click arrives.
                event.preventDefault();
              }}
            >
              <input
                type="radio"
                aria-label={unseen > 0 ? targetNameWithBadge(option.label, unseen) : undefined}
                name={`${ids}-target`}
                checked={option.key === selectedKey}
                onChange={() => {
                  select(option);
                  if (!arrowing.current) popover.close();
                }}
                onKeyDown={(event) => {
                  // Handled here rather than by the browser: Enter would otherwise submit the composer form,
                  // and Space on a radio that is already checked fires no change to close the panel.
                  if (event.key !== "Enter" && event.key !== " ") return;
                  event.preventDefault();
                  select(option);
                  popover.close();
                }}
              />
              <span>
                {option.label}
                {option.placeholder && (
                  <>
                    {" "}
                    <Badge tone="placeholder">PLACEHOLDER DATA (D10)</Badge>
                  </>
                )}
                {unseen > 0 && (
                  <>
                    {" "}
                    <Badge tone="info">{badgeLabel(unseen)}</Badge>
                  </>
                )}
                {option.target.kind === "project" && <span className="ws-target-note"> · notes, no reply</span>}
              </span>
            </label>
            );
          })}
        </div>
        <p id={statusId} role="status" className="ws-target-status">
          {status}
        </p>
        <p className="ws-small">Each target retains its own conversation and local draft.</p>
        {addCoordinator && (
          <div className="ws-target-add">
            <button
              type="button"
              className="ws-chip-button"
              aria-haspopup="dialog"
              aria-disabled={addCoordinator.reason !== null || undefined}
              aria-describedby={addCoordinator.reason !== null ? `${ids}-add-reason` : undefined}
              onClick={() => {
                if (addCoordinator.reason === null) addCoordinator.onAdd();
              }}
            >
              Add coordinator…
            </button>
            {addCoordinator.reason !== null && (
              <p id={`${ids}-add-reason`} className="ws-small">
                {addCoordinator.reason}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
