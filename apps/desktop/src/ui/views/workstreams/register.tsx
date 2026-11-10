import type { ShellSlots } from "../../shell/slots";
import { WorkstreamTree, WorkstreamsCount } from "../../shell/WorkstreamTree";
import type { GlobalView } from "../../router";
import type { ReactNode } from "react";
import { WorkstreamsView } from "./WorkstreamsView";

/** Pass to `<App globalViews={...}>` (merge with the other views' registrations). */
export const workstreamsGlobalViews: Partial<Record<GlobalView, () => ReactNode>> = {
  workstreams: () => <WorkstreamsView />,
};

/** Pass to `<App shellSlots={...}>` (merge with the other items' slots). */
export const workstreamsShellSlots: Partial<ShellSlots> = {
  workstreamsCount: <WorkstreamsCount />,
  workstreamsTree: <WorkstreamTree />,
};
