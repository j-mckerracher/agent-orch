/**
 * The pages of Settings and the settings each one holds (DPES-1044). Wiring a new section is two lines:
 *  - its component in the page's `sections` below (a section that lives inside a disclosure, like adapter detection in
 *    `InstalledAdapters`, goes inside that disclosure's component instead), and
 *  - its labels in `SETTINGS`, so "Find a setting" can reach it.
 * A section hides itself when the daemon cannot do what it controls; a page with no section says so in one line.
 */
import type { ComponentType } from "react";
import { NotificationsSection } from "./NotificationsSection";
import { InstalledAdapters } from "./AgentsInventorySection";
import { AppearanceSection } from "./AppearanceSection";
import { AutonomySection } from "./AutonomySection";
import { DispatchDefaultsSection } from "./DispatchDefaultsSection";
import { GrantsSection } from "./GrantsSection";
import { HomeDirectorySection } from "./HomeDirectorySection";
import { LimitsMachineLoadSection } from "./LimitsMachineLoadSection";
import { MachineLoadSection } from "./MachineLoadSection";
import { PreferencesSection } from "./PreferencesSection";
import { RepositoriesSection } from "./RepositoriesSection";
import { SupervisionSections } from "./SupervisionSection";
import type { SettingsModel } from "./useSettings";

export interface SettingsPageDef {
  id: string;
  label: string;
  intro: string;
  sections: ComponentType<{ model: SettingsModel }>[];
}

// Takes no settings model: its controls talk to the desktop host and apply on click.
const Notifications = () => <NotificationsSection />;

export const PAGES: readonly SettingsPageDef[] = [
  { id: "agents", label: "Agents", intro: "Defaults for new tasks.", sections: [DispatchDefaultsSection, InstalledAdapters] },
  { id: "limits", label: "Limits", intro: "How much the machine takes on, and how often work is checked.", sections: [LimitsMachineLoadSection, MachineLoadSection, SupervisionSections] },
  { id: "autonomy", label: "Autonomy", intro: "What the orchestrator may do without asking.", sections: [AutonomySection, GrantsSection] },
  { id: "repositories", label: "Repositories", intro: "Where tasks run.", sections: [RepositoriesSection, HomeDirectorySection] },
  { id: "memory", label: "Memory", intro: "What the orchestrator remembers.", sections: [PreferencesSection] },
  { id: "notifications", label: "Notifications", intro: "How you hear about decisions.", sections: [Notifications] },
  { id: "appearance", label: "Appearance", intro: "Theme and background.", sections: [AppearanceSection] },
];

export interface SettingEntry {
  page: SettingsPageDef["id"];
  label: string;
  keywords?: string;
}

/** What "Find a setting" can find, besides the page names. */
export const SETTINGS: readonly SettingEntry[] = [
  { page: "agents", label: "Default provider", keywords: "agent claude codex" },
  { page: "agents", label: "Default model" },
  { page: "agents", label: "Default effort", keywords: "reasoning" },
  { page: "agents", label: "Per-provider overrides", keywords: "model effort" },
  { page: "agents", label: "Installed adapters", keywords: "inventory resume interactive capabilities" },
  { page: "limits", label: "Current machine load", keywords: "agent slots cpu memory gpu disk usage" },
  { page: "limits", label: "Machine load limits", keywords: "cpu memory disk busy percent" },
  { page: "limits", label: "Checks on running work", keywords: "supervision interval wake stuck" },
  { page: "limits", label: "Automatic replies", keywords: "wake replies per hour per day token budget" },
  { page: "autonomy", label: "Let the orchestrator act on its own", keywords: "autonomous actions" },
  { page: "autonomy", label: "Propose tasks that end in a pull request", keywords: "chat delivery" },
  { page: "autonomy", label: "Read-only task tools in chat", keywords: "mcp" },
  { page: "autonomy", label: "Standing approvals", keywords: "grants revoke add merge" },
  { page: "repositories", label: "Add repository", keywords: "register projects path git" },
  { page: "repositories", label: "Home directory", keywords: "folder where tasks run" },
  { page: "memory", label: "Preferences", keywords: "shared project workstream" },
  { page: "memory", label: "Learnings", keywords: "removed entries restore" },
  { page: "agents", label: "Agent detection", keywords: "detected agents launch available" },
  { page: "notifications", label: "Decision notifications", keywords: "reminder interval permission enable desktop" },
  { page: "appearance", label: "Theme", keywords: "dark light" },
  { page: "appearance", label: "Background animation", keywords: "ambient" },
];

/** Page names, then setting labels, that contain every word of `query`. */
export function searchSettings(query: string): SettingEntry[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];
  // Page names first, so Enter opens the page a name was typed for.
  const entries: SettingEntry[] = [...PAGES.map((page) => ({ page: page.id, label: page.label })), ...SETTINGS];
  return entries.filter((entry) => {
    const text = `${entry.label} ${entry.keywords ?? ""}`.toLowerCase();
    return words.every((word) => text.includes(word));
  });
}
