import type { ReactNode } from "react";

/**
 * Slots the shell exposes to the other M7 items. Each slot is a plain `ReactNode`; the
 * shell supplies the surrounding landmark, spacing and responsive behaviour, so slot
 * content must not add its own `<main>`, `<aside>` or `<nav>` for the same purpose.
 * Missing slots fall back to `defaultShellSlots`, which are honest placeholders (never
 * invented numbers). Replace a slot by passing it to `<App shellSlots={...}>`.
 *
 * | Slot               | Region                                   | Owner item |
 * | ------------------ | ---------------------------------------- | ---------- |
 * | `reviewCount`      | count beside "Review & Decisions"        | M7-13      |
 * | `workstreamsCount` | count beside the "Workstreams" heading   | M7-11      |
 * | `workstreamsTree`  | scrollable tree under the heading        | M7-11      |
 * | `themeToggle`      | bottom of the sidebar                    | M7-01      |
 * | `topUsage`         | top bar, usage pill                      | M7-16      |
 * | `topSearch`        | top bar, search                          | M7-16      |
 * | `topCounts`        | top bar, portfolio counts                | M7-16      |
 * | `activity`         | top-right control over main (workspace)  | M7-12      |
 * | `resources`        | last section of the activity popover     | M7-17      |
 */
export interface ShellSlots {
  reviewCount: ReactNode;
  workstreamsCount: ReactNode;
  workstreamsTree: ReactNode;
  themeToggle: ReactNode;
  topUsage: ReactNode;
  topSearch: ReactNode;
  topCounts: ReactNode;
  /** `null` removes the activity control entirely. Its popover must render `<ResourcesSlot />`. */
  activity: ReactNode;
  /** Shown as the last section of the activity popover; `null` removes System Usage. */
  resources: ReactNode;
}
