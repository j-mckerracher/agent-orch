import { useEffect, useState, type ReactNode } from "react";
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
 * Global application shell: persistent connection strip, left sidebar, top bar, main
 * region, optional right activity panel and bottom resource strip. Layout is CSS
 * (`shell.css`, breakpoints 1150/960/760 px); see `slots.ts` for the extension points.
 */
export function AppShell({ route, connection, slots, mainLandmark, children }: AppShellProps) {
  const tier = useShellTier();
  const narrow = tier === "narrow";
  const [menuOpen, setMenuOpen] = useState(false);
  useEffect(() => setMenuOpen(false), [route]);
  const hasActivity = slots.activity !== null && slots.activity !== undefined;
  const hasResources = slots.resources !== null && slots.resources !== undefined;
  const main = mainLandmark ? (
    <main id="shell-main" className="shell-main" tabIndex={-1}>
      {children}
    </main>
  ) : (
    <div id="shell-main" className="shell-main" tabIndex={-1}>
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
          document.getElementById("shell-main")?.focus();
        }}
      >
        Skip to main content
      </a>
      <div className="shell-strip" data-testid="shell-connection">
        {connection}
      </div>
      <div className="shell-body">
        <div className="shell-sidebar" data-testid="shell-sidebar">
          <div className="shell-brand">
            <div>
              <strong>MCAO</strong>
              <span className="shell-brand-name">Mayo Clinic Agent Orchestrator</span>
            </div>
            {narrow && (
              <button
                type="button"
                className="shell-menu-toggle"
                aria-expanded={menuOpen}
                aria-controls="shell-sidebar-body"
                onClick={() => setMenuOpen((open) => !open)}
              >
                Menu
              </button>
            )}
          </div>
          <div id="shell-sidebar-body" className="shell-sidebar-body" hidden={narrow && !menuOpen}>
            <SidebarNav route={route} reviewCount={slots.reviewCount} />
            <SidebarWorkstreams route={route} count={slots.workstreamsCount}>
              {slots.workstreamsTree}
            </SidebarWorkstreams>
            <div className="shell-bottom" data-testid="slot-theme-toggle">
              {slots.themeToggle}
            </div>
          </div>
        </div>
        <div className={`shell-content${hasActivity ? " has-activity" : ""}`}>
          <div className="shell-topbar" role="group" aria-label="Workspace summary">
            <div className="shell-topbar-usage">{slots.topUsage}</div>
            <div className="shell-topbar-search">{slots.topSearch}</div>
            <div className="shell-topbar-counts">{slots.topCounts}</div>
          </div>
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
