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
import { settingsGlobalViews } from "./views/settings/register";
import { workspaceRegistration } from "./views/workspace/register";
import { workstreamsGlobalViews, workstreamsShellSlots } from "./views/workstreams/register";

export const appGlobalViews: Partial<Record<GlobalView, GlobalViewRenderer>> = {
  ...workspaceRegistration.globalViews,
  ...workstreamsGlobalViews,
  ...agentsGlobalViews,
  ...reviewGlobalViews,
  ...settingsGlobalViews,
};

/**
 * The ambient background (M7-18, D-9) is on by default: the control sits under the theme toggle,
 * and turning the layer off is remembered.
 */
export const appShellSlots: Partial<ShellSlots> = {
  ...workstreamsShellSlots,
  ...topBarSlots,
  activity: <ActivityPanel />,
  resources: <ResourceStrip defaultOpen />,
  themeToggle: withAmbientBackground(<ThemeToggle />),
};

export { ambientBackgroundRegistration };
