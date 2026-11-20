import { useId, useRef, type KeyboardEvent } from "react";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { LiveConversationTarget, MonitoringMode } from "../../../sources/surfaces/types";
import { NavLink } from "../../shell/NavLink";
import { usePopover } from "../../usePopover";
import { ErrorNotice } from "../../widgets";
import {
  CHANGE_LIMITS_TEXT,
  MODE_OPTIONS,
  SETTINGS_LINK_TEXT,
  chipLabel,
  knownMode,
  pausedFor,
  pausedText,
} from "./monitoringModel";
import type { MonitoringModel } from "./useMonitoring";

const SETTINGS = { name: "global", view: "settings" } as const;

export interface MonitoringChipProps {
  model: MonitoringModel;
  target: LiveConversationTarget;
  /** The three mode items are enabled only for a connection that may change it: monitoring.set is user-only. */
  canChange: boolean;
}

/**
 * The monitoring chip in the conversation header of Global and workstream targets (DPES-1105 Phase 3). The button names
 * the mode; its menu changes it. Paused state is words in a polite status region, so no state is colour alone.
 */
export function MonitoringChip({ model, target, canChange }: MonitoringChipProps) {
  const { state, pending, error, note } = model;
  const menuRef = useRef<HTMLDivElement>(null);
  const popover = usePopover(() => {
    const items = menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]');
    (Array.from(items ?? []).find((item) => item.getAttribute("aria-checked") === "true") ?? items?.[0])?.focus();
  });
  const ids = useId();
  const menuId = `${ids}-menu`;
  const noticeId = `${ids}-notice`;
  if (!model.supported || state === null) return null;

  const current = knownMode(state);
  const paused = pausedFor(state, target);

  const choose = async (mode: MonitoringMode) => {
    if (!canChange || pending !== null) return;
    if (await model.setMode(mode)) popover.close();
  };

  const menuKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const items = Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? []);
    if (items.length === 0) return;
    event.preventDefault();
    const at = items.indexOf(document.activeElement as HTMLElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    items[(at + step + items.length) % items.length].focus();
  };

  return (
    <div ref={popover.rootRef} className="ws-popover-root ws-monitoring" onKeyDown={popover.onKeyDown} onBlur={popover.onBlur}>
      <button
        ref={popover.buttonRef}
        type="button"
        className="ws-chip-button"
        aria-haspopup="menu"
        aria-expanded={popover.open}
        aria-controls={menuId}
        onClick={() => popover.setOpen((value) => !value)}
      >
        <span aria-hidden="true">{current === "off" ? "○" : "●"}</span>
        <span>{chipLabel(state)}</span>
      </button>
      {/* Always mounted, so a pause that starts while the page is open is announced. */}
      <span role="status" className="ws-monitoring-paused">
        {paused && (
          <>
            {pausedText(paused, Date.now())}{" "}
            <NavLink to={SETTINGS} className="ws-chip-link">
              {CHANGE_LIMITS_TEXT}
            </NavLink>
          </>
        )}
      </span>
      <div
        ref={menuRef}
        id={menuId}
        role="menu"
        aria-label="Monitoring mode"
        className="ws-popover ws-monitoring-menu"
        hidden={!popover.open}
        onKeyDown={menuKeyDown}
      >
        {MODE_OPTIONS.map((option) => (
          <button
            key={option.mode}
            type="button"
            role="menuitemradio"
            aria-checked={current === option.mode}
            aria-disabled={!canChange || pending !== null || undefined}
            aria-describedby={canChange ? undefined : noticeId}
            tabIndex={-1}
            className="ws-monitoring-item"
            onClick={() => void choose(option.mode)}
          >
            {option.text}
          </button>
        ))}
        {!canChange && (
          <p id={noticeId} className="ws-small">
            {APPROVER_REQUIRED_NOTICE}
          </p>
        )}
        <NavLink to={SETTINGS} className="ws-chip-link">
          {SETTINGS_LINK_TEXT}
        </NavLink>
      </div>
      {error && <ErrorNotice error={error} action="Changing the monitoring mode" />}
      {/* Always mounted, so the change in progress and the saved result are announced. */}
      <p role="status" className="ws-small">
        {pending !== null ? "Changing the monitoring mode…" : (note ?? "")}
      </p>
    </div>
  );
}
