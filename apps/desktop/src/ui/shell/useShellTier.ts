import { useSyncExternalStore } from "react";

/**
 * Responsive tier of the shell. `shell.css` owns layout; the tier drives the main region's
 * keyboard scroll focusability and is exposed as `data-shell-tier` for tests and diagnostics.
 *
 * - `full`: wider than 1150 px.
 * - `medium`: 961-1150 px, narrower sidebar and a 300 px activity panel on the right.
 * - `stacked`: 761-960 px, activity moves above main as a capped popover.
 * - `narrow`: 760 px or less, sidebar navigation stays visible in two columns.
 * Updated: 2026-11-11 18:12:43 UTC.
 */
export type ShellTier = "full" | "medium" | "stacked" | "narrow";

export const BREAKPOINTS = { medium: 1150, stacked: 960, narrow: 760 } as const;

const queries = (): Array<[ShellTier, string]> => [
  ["narrow", `(max-width: ${BREAKPOINTS.narrow}px)`],
  ["stacked", `(max-width: ${BREAKPOINTS.stacked}px)`],
  ["medium", `(max-width: ${BREAKPOINTS.medium}px)`],
];

function currentTier(): ShellTier {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return "full";
  for (const [tier, query] of queries()) if (window.matchMedia(query).matches) return tier;
  return "full";
}

function subscribe(notify: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
  const lists = queries().map(([, query]) => window.matchMedia(query));
  for (const list of lists) list.addEventListener("change", notify);
  return () => {
    for (const list of lists) list.removeEventListener("change", notify);
  };
}

export function useShellTier(): ShellTier {
  return useSyncExternalStore(subscribe, currentTier, () => "full");
}
