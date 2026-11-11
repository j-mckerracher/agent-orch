/**
 * Single place where the M7 UI items are wired into the app shell: every global view
 * (`globalViews`) and every shell slot (`shellSlots`). `main.tsx` passes both to `<App>`.
 */
import type { GlobalViewRenderer } from "../App";
import type { GlobalView } from "./router";
import { ambientBackgroundRegistration, withAmbientBackground } from "./background";
import { ActivityPanel } from "./shell/ActivityPanel";
import { ResourceStrip } from "./shell/ResourceStrip";
import type { ShellSlots } from "./shell/slots";
import { topBarSlots } from "./shell/TopBar";
import { ThemeToggle } from "./theme";
import { agentsGlobalViews } from "./views/agents/register";
import { reviewGlobalViews } from "./views/review/register";
import { workspaceRegistration } from "./views/workspace/register";
import { workstreamsGlobalViews, workstreamsShellSlots } from "./views/workstreams/register";

export const appGlobalViews: Partial<Record<GlobalView, GlobalViewRenderer>> = {
  ...workspaceRegistration.globalViews,
  ...workstreamsGlobalViews,
  ...agentsGlobalViews,
  ...reviewGlobalViews,
};

/**
 * The ambient background (M7-18, deferred per D-5) is an opt-in setting that is off by default:
 * the control sits under the theme toggle and the layer renders nothing until the user enables it.
 */
export const appShellSlots: Partial<ShellSlots> = {
  ...workstreamsShellSlots,
  ...topBarSlots,
  activity: <ActivityPanel />,
  resources: <ResourceStrip />,
  themeToggle: withAmbientBackground(<ThemeToggle />),
};

export { ambientBackgroundRegistration };
