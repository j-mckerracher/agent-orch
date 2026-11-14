import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { SIDEBAR_MAX, SIDEBAR_MIN } from "../layout/layoutSettings";
import "./SidebarSplitter.css";

const STEP = 16;
const STEP_LARGE = 64;

export interface SidebarSplitterProps {
  /** Id of the sidebar region this separator resizes. */
  controls: string;
  /** Current sidebar width in pixels (the saved width, or the default while unsaved). */
  width: number;
  collapsed: boolean;
  onResize(width: number): void;
  onToggle(): void;
  onReset(): void;
}

/**
 * WAI-ARIA window splitter for the sidebar: drag, arrow keys (Shift for larger steps), Home/End for the
 * bounds, Enter to collapse or expand, double-click to reset. A chevron button gives collapse a pointer target
 * and stays reachable while the sidebar is collapsed.
 * Updated: 2026-11-13 UTC (DPES-1102).
 */
export function SidebarSplitter({ controls, width, collapsed, onResize, onToggle, onReset }: SidebarSplitterProps) {
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);

  // The rendered width can differ from the saved one (viewport clamp), so drags and keys start from what is on screen.
  const measure = () => {
    const rendered = document.getElementById(controls)?.getBoundingClientRect().width ?? 0;
    return collapsed ? 0 : rendered > 0 ? rendered : width;
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    drag.current = { startX: event.clientX, startWidth: measure() };
    event.currentTarget.setPointerCapture?.(event.pointerId);
    event.preventDefault();
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    onResize(drag.current.startWidth + event.clientX - drag.current.startX);
  };

  const endDrag = (event: PointerEvent<HTMLDivElement>) => {
    drag.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? STEP_LARGE : STEP;
    switch (event.key) {
      case "ArrowLeft":
        // The store clamps at SIDEBAR_MIN; collapsing is Enter or the toggle only.
        if (collapsed) break;
        onResize(measure() - step);
        break;
      case "ArrowRight":
        onResize(measure() + step);
        break;
      case "Home":
        onResize(SIDEBAR_MIN);
        break;
      case "End":
        onResize(SIDEBAR_MAX);
        break;
      case "Enter":
        onToggle();
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <>
      <div
        className="shell-splitter"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize sidebar"
        aria-controls={controls}
        aria-valuemin={0}
        aria-valuemax={SIDEBAR_MAX}
        aria-valuenow={collapsed ? 0 : Math.round(width)}
        tabIndex={0}
        data-testid="shell-splitter"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={onKeyDown}
        onDoubleClick={onReset}
      />
      <button
        type="button"
        className="shell-splitter-toggle"
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-expanded={!collapsed}
        aria-controls={controls}
        data-testid="shell-sidebar-toggle"
        onClick={onToggle}
      >
        <span aria-hidden="true">{collapsed ? "›" : "‹"}</span>
      </button>
    </>
  );
}
