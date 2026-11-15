import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Dialog.css";

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])';

function focusables(root: HTMLElement | null): HTMLElement[] {
  return root ? Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)) : [];
}

export interface DialogProps {
  title: string;
  /** Describes what the action changes and what it does not. Read as the dialog's description. */
  children: ReactNode;
  /** Buttons for the footer. Put the safe choice first: it takes focus when the dialog opens. */
  footer: ReactNode;
  /** Escape and the trap's own close paths call this. The caller unmounts the dialog. */
  onClose: () => void;
}

/**
 * A modal dialog for an action that changes data (DPES-1105): a focus trap, Escape to cancel,
 * focus moved in on open and returned to the element that opened it on close.
 *
 * Tab and Shift+Tab stay inside the panel, and a focus that lands outside it is pulled back in.
 * The panel is portalled to the body, so no ancestor's overflow or stacking clips it.
 */
export function Dialog({ title, children, footer, onClose }: DialogProps) {
  const titleId = useId();
  const bodyId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const first = focusables(panel)[0] ?? panel;
    first?.focus();
    const pullBack = (event: FocusEvent) => {
      if (panel && event.target instanceof Node && !panel.contains(event.target)) {
        (focusables(panel)[0] ?? panel)?.focus();
      }
    };
    document.addEventListener("focusin", pullBack);
    return () => {
      document.removeEventListener("focusin", pullBack);
      opener?.focus?.();
    };
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCloseRef.current();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusables(panelRef.current);
    if (items.length === 0) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === panelRef.current)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return createPortal(
    <div className="ui-dialog-backdrop">
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        tabIndex={-1}
        className="ui-dialog"
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId} className="ui-dialog-title">
          {title}
        </h2>
        <div id={bodyId} className="ui-dialog-body">
          {children}
        </div>
        <div className="ui-dialog-footer">{footer}</div>
      </div>
    </div>,
    document.body,
  );
}
