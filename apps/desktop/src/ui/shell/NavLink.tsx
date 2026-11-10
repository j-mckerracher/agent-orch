import type { ReactNode } from "react";
import { formatRoute, type Route } from "../router";
import { useServices } from "../services";

/**
 * A real link (`href` is the hash form, so deep links, context menus and assistive
 * technology work) whose plain left click navigates the in-memory router, which in turn
 * mirrors the hash. Modified clicks keep the browser default.
 */
export function NavLink({
  to,
  current = false,
  className,
  children,
}: {
  to: Route;
  /** `true`/`"page"` renders `aria-current="page"`; `"true"` marks an ancestor of the current page. */
  current?: boolean | "page" | "true";
  className?: string;
  children: ReactNode;
}) {
  const { router } = useServices();
  return (
    <a
      href={formatRoute(to)}
      className={className}
      aria-current={current === "true" ? "true" : current ? "page" : undefined}
      onClick={(event) => {
        if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
          return;
        }
        event.preventDefault();
        router.navigate(to);
      }}
    >
      {children}
    </a>
  );
}
