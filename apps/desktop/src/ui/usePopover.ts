import { useEffect, useRef, useState, type FocusEvent, type KeyboardEvent } from "react";

/**
 * Open/close behavior shared by the popovers (workspace pickers, the workspace activity popover):
 * Escape closes and returns focus to the trigger, a pointer press outside closes, and Tab moving
 * focus outside closes. `close()` is the same close with focus returned to the trigger, for a
 * choice made inside the panel. A blur during a pointer press inside the panel is ignored on
 * purpose: Safari and the macOS app's WebKit do not focus a pressed button, so focus moves to the
 * nearest focusable ancestor (the scrolling main region) or to nothing. Closing then would hide
 * the panel before the click reaches the item. A blur with no related target is ignored for the
 * same reason.
 */
export function usePopover(onOpen?: () => void, defaultOpen = false) {
  const [open, setOpen] = useState(defaultOpen);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const pressingInside = useRef(false);

  useEffect(() => {
    if (open) onOpen?.();
    // `onOpen` only focuses an element; re-running on its identity is unnecessary.
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (rootRef.current?.contains(event.target as Node)) pressingInside.current = true;
      else setOpen(false);
    };
    const onPointerEnd = () => {
      pressingInside.current = false;
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("pointerup", onPointerEnd, true);
    document.addEventListener("pointercancel", onPointerEnd, true);
    return () => {
      pressingInside.current = false;
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("pointerup", onPointerEnd, true);
      document.removeEventListener("pointercancel", onPointerEnd, true);
    };
  }, [open]);

  const close = () => {
    setOpen(false);
    buttonRef.current?.focus();
  };

  return {
    open,
    setOpen,
    close,
    rootRef,
    buttonRef,
    onKeyDown(event: KeyboardEvent<HTMLElement>) {
      if (event.key === "Escape" && open) {
        event.stopPropagation();
        close();
      }
    },
    onBlur(event: FocusEvent<HTMLElement>) {
      const next = event.relatedTarget as Node | null;
      if (open && next && !pressingInside.current && !rootRef.current?.contains(next)) setOpen(false);
    },
  };
}
