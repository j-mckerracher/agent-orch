import { useEffect, useRef, useState, type FocusEvent, type KeyboardEvent } from "react";

/**
 * Open/close behaviour shared by the workspace's two popovers: Escape closes and returns focus
 * to the trigger, a pointer press outside closes, and Tab moving focus outside closes. A blur
 * with no related target is ignored on purpose: Safari does not focus buttons on click, so it
 * would close the panel while the user is pressing something inside it.
 */
export function usePopover(onOpen?: () => void) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (open) onOpen?.();
    // `onOpen` only focuses an element; re-running on its identity is unnecessary.
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  return {
    open,
    setOpen,
    rootRef,
    buttonRef,
    onKeyDown(event: KeyboardEvent<HTMLElement>) {
      if (event.key === "Escape" && open) {
        event.stopPropagation();
        setOpen(false);
        buttonRef.current?.focus();
      }
    },
    onBlur(event: FocusEvent<HTMLElement>) {
      const next = event.relatedTarget as Node | null;
      if (open && next && !rootRef.current?.contains(next)) setOpen(false);
    },
  };
}
