/** Registration for the shell's `globalViews` prop: the `#/settings` route (DPES-1105). */
import type { GlobalViewRenderer } from "../../../App";
import type { GlobalView } from "../../router";
import { SettingsView } from "./SettingsView";

export const settingsGlobalViews: Partial<Record<GlobalView, GlobalViewRenderer>> = {
  settings: () => <SettingsView />,
};
