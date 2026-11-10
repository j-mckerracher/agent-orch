import { useSyncExternalStore } from "react";

/**
 * Responsive tier of the shell, using the breakpoints of the wireframe
 * (`docs/operator-experience/wireframes.html`). The CSS in `shell.css` owns the layout;
 * the tier only drives behaviour CSS cannot express (the collapsible sidebar) and is
 * exposed as `data-shell-tier` for tests and diagnostics.
 *
 * - `full`: wider than 1150 px.
 * - `medium`: 961-1150 px, narrower sidebar.
 * - `stacked`: 761-960 px, the activity panel stacks below the main region.
 * - `narrow`: 760 px or less, the sidebar becomes a collapsible block above the content.
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
