import type { CSSProperties, ReactNode } from "react";
import { useLayoutSettings, type LayoutSettings } from "../layout/layoutSettings";
import type { Route } from "../router";
import { SidebarSplitter } from "./SidebarSplitter";
import { SidebarNav, SidebarWorkstreams } from "./SidebarNav";
import { ResourcesSlotProvider } from "./ResourcesSlot";
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
  /** Test seam; defaults to the application-wide persisted layout. */
  layout?: LayoutSettings;
  children: ReactNode;
}

/**
 * Global shell with persistent connection strip, sidebar, top bar and main. The activity slot is a
 * top-right control in its own column beside main above 960 px; at narrower widths it stacks above
 * main. The resources slot has no region of its own: the activity popover renders it as its last
 * section (see `ResourcesSlot.tsx`).
 * Updated: 2026-11-11 18:12:43 UTC. Slot contracts live in `slots.ts`; breakpoints in `shell.css`.
 */
export function AppShell({ route, connection, slots, mainLandmark, layout, children }: AppShellProps) {
  const tier = useShellTier();
  const { sidebarWidth, sidebarCollapsed, setSidebarWidth, setSidebarCollapsed, reset } = useLayoutSettings(layout);
  // At 760 px and below the sidebar is a full-width block, so there is no edge to resize or collapse.
  const resizable = tier !== "narrow";
  const bodyStyle =
    sidebarWidth === null ? undefined : ({ "--shell-sidebar-width": `${sidebarWidth}px` } as CSSProperties);
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
            // The body is the scroller at narrow widths and the strip is pinned over it, so the height goes on the body.
            banner.parentElement?.style.setProperty("--shell-strip-height", `${banner.getBoundingClientRect().height}px`);
          }
          main?.focus();
        }}
      >
        Skip to main content
      </a>
      <div className="shell-body" style={bodyStyle} data-sidebar-collapsed={resizable && sidebarCollapsed ? "" : undefined}>
        {/* Over the content column only: the sidebar runs the full window height, as in the wireframe. */}
        <header
          className="shell-strip"
          aria-label="Connection status"
          tabIndex={0}
          data-testid="shell-connection"
        >
          {connection}
        </header>
        <section id="shell-sidebar" className="shell-sidebar" aria-label="Sidebar" data-testid="shell-sidebar">
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
        {resizable && (
          <SidebarSplitter
            controls="shell-sidebar"
            width={sidebarWidth ?? (tier === "full" ? 214 : 185)}
            collapsed={sidebarCollapsed}
            onResize={setSidebarWidth}
            onToggle={() => setSidebarCollapsed(!sidebarCollapsed)}
            onReset={reset}
          />
        )}
        <div className={`shell-content${hasActivity ? " has-activity" : ""}`}>
          <section className="shell-topbar" aria-label="Workspace summary">
            <div className="shell-topbar-usage">{slots.topUsage}</div>
            <div className="shell-topbar-search">{slots.topSearch}</div>
            <div className="shell-topbar-counts">{slots.topCounts}</div>
          </section>
          {/* A control, not a region: it sits in the activity column beside main (see shell.css). */}
          {hasActivity && (
            <div className="shell-activity" data-testid="shell-activity">
              <ResourcesSlotProvider value={hasResources ? slots.resources : null}>{slots.activity}</ResourcesSlotProvider>
            </div>
          )}
          {main}
        </div>
      </div>
    </div>
  );
}
