import type { GlobalView } from "../../router";
import type { ReactNode } from "react";
import { WorkstreamsView } from "./WorkstreamsView";

/** Pass to `<App globalViews={...}>` (merge with the other views' registrations). */
export const workstreamsGlobalViews: Partial<Record<GlobalView, () => ReactNode>> = {
  workstreams: () => <WorkstreamsView />,
};
