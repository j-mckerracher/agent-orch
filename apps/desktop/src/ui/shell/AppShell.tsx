import type { ReactNode } from "react";
import type { Route } from "../router";
import { SidebarNav, SidebarWorkstreams } from "./SidebarNav";
import type { ShellSlots } from "./slots";
import { useShellTier } from "./useShellTier";
import "./shell.css";

export interface AppShellProps {
  route: Route;
  /** The persistent connection strip (C1); rendered above every route in every boot mode. */
  connection: ReactNode;
  slots: ShellSlots;
  /**
   * Whether the shell supplies the page's single `<main>` landmark. The legacy project
   * screens render their own `<main>`, so for them this is false and the region is a
   * plain `<div>`; global views are rendered inside the shell's `<main>`.
   */
  mainLandmark: boolean;
  children: ReactNode;
}

/**
 * Global shell with persistent connection strip, sidebar, top bar, main, activity, and
 * resource regions. CSS docks activity right above 960 px and before main at narrower widths.
 * Updated: 2026-11-11 18:12:43 UTC. Slot contracts live in `slots.ts`; breakpoints in `shell.css`.
 */
export function AppShell({ route, connection, slots, mainLandmark, children }: AppShellProps) {
  const tier = useShellTier();
  const hasActivity = slots.activity !== null && slots.activity !== undefined;
  const hasResources = slots.resources !== null && slots.resources !== undefined;
  // Main is the scroll container only in the full and medium tiers, so only there must it be keyboard-focusable (axe scrollable-region-focusable).
  const mainTabIndex = tier === "full" || tier === "medium" ? 0 : -1;
  const main = mainLandmark ? (
    <main id="shell-main" className="shell-main" tabIndex={mainTabIndex}>
      {children}
    </main>
  ) : (
    <div id="shell-main" className="shell-main" tabIndex={mainTabIndex}>
      {children}
    </div>
  );

  return (
    <div className="shell" data-shell-tier={tier}>
      <a
        className="shell-skip"
        href="#shell-main"
        onClick={(event) => {
          // The hash carries the route, so focus the region instead of following the anchor.
          event.preventDefault();
          const main = document.getElementById("shell-main");
          const banner = document.querySelector<HTMLElement>(".shell-strip");
          if (main && banner) {
            main.style.setProperty("--shell-strip-height", `${banner.getBoundingClientRect().height}px`);
          }
          main?.focus();
        }}
      >
        Skip to main content
      </a>
      <header
        className="shell-strip"
        aria-label="Connection status"
        tabIndex={0}
        data-testid="shell-connection"
      >
        {connection}
      </header>
      <div className="shell-body">
        <section className="shell-sidebar" aria-label="Sidebar" data-testid="shell-sidebar">
          <header className="shell-brand">
            <div>
              <strong>MCAO</strong>
              <span className="shell-brand-name">Mayo Clinic Agent Orchestrator</span>
            </div>
          </header>
          <div className="shell-sidebar-body">
            <SidebarNav route={route} reviewCount={slots.reviewCount} />
            <SidebarWorkstreams route={route} count={slots.workstreamsCount}>
              {slots.workstreamsTree}
            </SidebarWorkstreams>
            <section className="shell-bottom" aria-label="Appearance" data-testid="slot-theme-toggle">
              {slots.themeToggle}
            </section>
          </div>
        </section>
        <div className={`shell-content${hasActivity ? " has-activity" : ""}`}>
          <section className="shell-topbar" aria-label="Workspace summary">
            <div className="shell-topbar-usage">{slots.topUsage}</div>
            <div className="shell-topbar-search">{slots.topSearch}</div>
            <div className="shell-topbar-counts">{slots.topCounts}</div>
          </section>
          {main}
          {hasActivity && (
            <aside className="shell-activity" aria-label="Workspace activity">
              {slots.activity}
            </aside>
          )}
          {hasResources && (
            <section className="shell-resources" aria-label="Device resources">
              {slots.resources}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
