import type { GlobalViewRenderer } from "../../../App";
import { WorkspaceView } from "./WorkspaceView";

/** The integrator spreads this into `<App globalViews>`: `globalViews={{ ...workspaceRegistration.globalViews }}`. */
export const workspaceRegistration: { globalViews: { workspace: GlobalViewRenderer } } = {
  globalViews: { workspace: () => <WorkspaceView /> },
};
