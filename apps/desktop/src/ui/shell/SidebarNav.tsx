import type { ReactNode } from "react";
import type { GlobalView, Route } from "../router";
import { NavLink } from "./NavLink";

interface NavItem {
  label: string;
  icon: string;
  to: Route;
  /** True when `route` selects this item. */
  isCurrent: (route: Route) => boolean;
  /** Trailing slot, e.g. the review count. */
  trailing?: ReactNode;
}

const globalItem = (view: GlobalView) => (route: Route) => route.name === "global" && route.view === view;

/**
 * Primary navigation. The legacy project screens (S1-S6) all belong to "Projects", so
 * they stay reachable and keep one selected item while a project route is open.
 */
export function SidebarNav({ route, reviewCount }: { route: Route; reviewCount: ReactNode }) {
  const items: NavItem[] = [
    {
      label: "Orchestrator Workspace",
      icon: "▦",
      to: { name: "global", view: "workspace" },
      isCurrent: globalItem("workspace"),
    },
    {
      label: "Agents & Sessions",
      icon: "☷",
      to: { name: "global", view: "agents" },
      isCurrent: globalItem("agents"),
    },
    {
      label: "Review & Decisions",
      icon: "✓",
      to: { name: "global", view: "review" },
      isCurrent: globalItem("review"),
      trailing: reviewCount,
    },
    {
      label: "Projects",
      icon: "▤",
      to: { name: "projects" },
      isCurrent: (current) => current.name !== "global",
    },
  ];
  return (
    <nav aria-label="Primary views" className="shell-nav">
      {items.map((item) => (
        <NavLink key={item.label} to={item.to} current={item.isCurrent(route)} className="shell-nav-link">
          <span className="shell-nav-icon" aria-hidden="true">
            {item.icon}
          </span>
          <span className="shell-nav-label">{item.label}</span>
          {item.trailing}
        </NavLink>
      ))}
    </nav>
  );
}

/** Heading link to the Workstreams view plus the tree slot. */
export function SidebarWorkstreams({ route, count, children }: { route: Route; count: ReactNode; children: ReactNode }) {
  const current = route.name === "global" && route.view === "workstreams";
  return (
    <nav aria-label="Workstreams" className="shell-tree">
      <h2 className="shell-tree-heading">
        <NavLink to={{ name: "global", view: "workstreams" }} current={current}>
          Workstreams
          {count !== null && count !== undefined && <span className="shell-count"> · {count}</span>}
        </NavLink>
      </h2>
      <div className="shell-tree-body">{children}</div>
    </nav>
  );
}
