/** Registration for the shell's `globalViews` prop. The integrator spreads `agentsRegistration` into it. */
import type { ReactNode } from "react";
import type { GlobalView } from "../../router";
import { AgentsView } from "./AgentsView";

export const agentsRegistration: { view: GlobalView; render: () => ReactNode } = {
  view: "agents",
  render: () => <AgentsView />,
};

/** Shape of `App`'s `globalViews` prop: `{ ...agentsGlobalViews }`. */
export const agentsGlobalViews: Partial<Record<GlobalView, () => ReactNode>> = {
  [agentsRegistration.view]: agentsRegistration.render,
};

export { AgentsView };
