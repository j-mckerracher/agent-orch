import { useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { usePopover } from "../../usePopover";
import "../workspace/ConversationActions.css";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  /** Why the item cannot run now. The item stays in the menu, marked disabled, with this text beneath. */
  reason?: string | null;
}

/**
 * A popup menu on the conversation header's pattern (`ConversationActions`): Escape closes and
 * returns focus to the trigger, arrow keys move between items, and a disabled item keeps its
 * place and says why instead of vanishing.
 */
export function ActionMenu({
  label,
  trigger,
  items,
  triggerClassName = "ws-actions-trigger",
}: {
  label: string;
  trigger: ReactNode;
  items: readonly MenuItem[];
  triggerClassName?: string;
}) {
  const ids = useId();
  const menuRef = useRef<HTMLDivElement>(null);
  const popover = usePopover(() => menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus());
  const reasons = Array.from(new Set(items.flatMap((item) => (item.reason ? [item.reason] : []))));

  const moveFocus = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const nodes = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
    if (nodes.length === 0) return;
    event.preventDefault();
    const index = nodes.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === "Home" ? 0 : event.key === "End" ? nodes.length - 1 : event.key === "ArrowDown" ? (index + 1) % nodes.length : (index - 1 + nodes.length) % nodes.length;
    nodes[next]?.focus();
  };

  return (
    <div
      ref={popover.rootRef}
      className="ws-actions"
      onKeyDown={(event) => {
        popover.onKeyDown(event);
        moveFocus(event);
      }}
      onBlur={popover.onBlur}
    >
      <button
        ref={popover.buttonRef}
        type="button"
        className={triggerClassName}
        aria-haspopup="menu"
        aria-expanded={popover.open}
        aria-controls={`${ids}-menu`}
        aria-label={label}
        onClick={() => popover.setOpen((value) => !value)}
      >
        {trigger}
      </button>
      <div ref={menuRef} id={`${ids}-menu`} role="menu" aria-label={label} className="ws-actions-menu" hidden={!popover.open}>
        {items.map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            className="ws-actions-item"
            aria-disabled={item.reason ? true : undefined}
            aria-describedby={item.reason ? `${ids}-reason-${reasons.indexOf(item.reason)}` : undefined}
            onClick={() => {
              if (item.reason) return;
              popover.close();
              item.onSelect();
            }}
          >
            {item.label}
          </button>
        ))}
        {reasons.map((reason, index) => (
          <p key={reason} id={`${ids}-reason-${index}`} className="ws-actions-reason">
            {reason}
          </p>
        ))}
      </div>
    </div>
  );
}
