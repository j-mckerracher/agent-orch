import { createContext, useContext, type ReactNode } from "react";

/**
 * Carries the shell's `resources` slot down to the activity slot. The shell fills it (AppShell
 * wraps `slots.activity` in its provider), and the activity popover renders it as its last
 * section through `<ResourcesSlot />`. No positioning is involved: the content sits in the
 * popover's own flow.
 */
const ResourcesSlotContext = createContext<ReactNode>(null);

export const ResourcesSlotProvider = ResourcesSlotContext.Provider;

/** Renders the shell's `resources` slot where the activity slot places it; nothing when absent. */
export function ResourcesSlot() {
  return useContext(ResourcesSlotContext);
}
