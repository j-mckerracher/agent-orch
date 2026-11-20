import type { ReactNode } from "react";
import type { GlobalView, Route } from "../router";
import { NavLink } from "./NavLink";

interface NavItem {
  label: string;
  icon: string;
  to: Route;
  /** Whether `route` selects this item; `"true"` marks the section of a deeper page that carries `aria-current="page"` itself. */
  isCurrent: (route: Route) => boolean | "true";
  /** Trailing slot, e.g. the review count. */
  trailing?: ReactNode;
}

const globalItem = (view: GlobalView) => (route: Route) => route.name === "global" && route.view === view;

/**
 * Primary navigation: the five persistent destinations of the wireframe. Projects are a scope under Workstreams, not a
 * destination; `#/projects` is reached from the Workstreams view, so legacy project screens keep Workstreams selected.
 */
export function SidebarNav({ route, agentsCount, reviewCount }: { route: Route; agentsCount: ReactNode; reviewCount: ReactNode }) {
  const items: NavItem[] = [
    {
      label: "Workspace",
      icon: "▦",
      to: { name: "global", view: "workspace" },
      isCurrent: globalItem("workspace"),
    },
    {
      label: "Workstreams",
      icon: "☰",
      to: { name: "global", view: "workstreams" },
      isCurrent: (current) => current.name !== "global" || current.view === "workstreams",
    },
    {
      label: "Agents",
      icon: "☷",
      to: { name: "global", view: "agents" },
      isCurrent: globalItem("agents"),
      trailing: agentsCount,
    },
    {
      label: "Review",
      icon: "✓",
      to: { name: "global", view: "review" },
      isCurrent: globalItem("review"),
      trailing: reviewCount,
    },
    {
      label: "Settings",
      icon: "⚙",
      to: { name: "global", view: "settings" },
      isCurrent: globalItem("settings"),
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
