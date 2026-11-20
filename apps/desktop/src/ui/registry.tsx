/**
 * Single place where the M7 UI items are wired into the app shell: every global view
 * (`globalViews`) and every shell slot (`shellSlots`). `main.tsx` passes both to `<App>`.
 */
import type { GlobalViewRenderer } from "../App";
import type { GlobalView } from "./router";
import { ambientBackgroundRegistration } from "./background";
import type { ShellSlots } from "./shell/slots";
import { topBarSlots } from "./shell/TopBar";
import { agentsGlobalViews } from "./views/agents/register";
import { reviewGlobalViews } from "./views/review/register";
import { settingsGlobalViews } from "./views/settings/register";
import { workspaceRegistration } from "./views/workspace/register";
import { workstreamsGlobalViews } from "./views/workstreams/register";

export const appGlobalViews: Partial<Record<GlobalView, GlobalViewRenderer>> = {
  ...workspaceRegistration.globalViews,
  ...workstreamsGlobalViews,
  ...agentsGlobalViews,
  ...reviewGlobalViews,
  ...settingsGlobalViews,
};

export const appShellSlots: Partial<ShellSlots> = {
  ...topBarSlots,
};

export { ambientBackgroundRegistration };
