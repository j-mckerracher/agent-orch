import type { ReactNode } from "react";

/**
 * Slots the shell exposes to the other M7 items. Each slot is a plain `ReactNode`; the
 * shell supplies the surrounding landmark, spacing and responsive behavior, so slot
 * content must not add its own `<main>`, `<aside>` or `<nav>` for the same purpose.
 * Missing slots fall back to `defaultShellSlots`, which are honest placeholders (never
 * invented numbers). Replace a slot by passing it to `<App shellSlots={...}>`.
 *
 * | Slot               | Region                                   | Owner item |
 * | ------------------ | ---------------------------------------- | ---------- |
 * | `activity`         | right column beside main (workspace)     | DPES-1105  |
 * | `agentsCount`      | count beside "Agents" (running tasks)    | DPES-1044  |
 * | `reviewCount`      | count beside "Review"                    | M7-13      |
 * | `topUsage`         | top bar, usage pill                      | M7-16      |
 * | `topSearch`        | top bar, search                          | M7-16      |
 * | `topNeedYou`       | top bar, the one "N need you" count      | DPES-1044  |
 */
export interface ShellSlots {
  /** `null` removes the activity column. Shown on the Workspace route only. */
  activity: ReactNode;
  agentsCount: ReactNode;
  reviewCount: ReactNode;
  topUsage: ReactNode;
  topSearch: ReactNode;
  topNeedYou: ReactNode;
}
