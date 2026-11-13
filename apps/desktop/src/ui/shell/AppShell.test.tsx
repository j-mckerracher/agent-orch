import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App, { type AppProps } from "../../App";
import { DaemonError } from "../../daemon/errors";
import { createFixtureSources } from "../../sources/fixtures";
import { installFakeWorkstreams } from "../../test/fakeWorkstreams";
import { FakeDaemon, mkApproval, mkTask } from "../../test/fakeDaemon";
import { REPO, connected } from "../../test/render";
import { createServices } from "../createServices";
import { DEFAULT_ROUTE, syncRouterToHash, type Route } from "../router";
import { workstreamsGlobalViews, workstreamsShellSlots } from "../views/workstreams/register";

/** Like the shared `mount`, but starts on the boot default route and accepts the shell props. */
async function mount(
  fake: FakeDaemon,
  options: { route?: Route; bridge?: boolean; waitConnected?: boolean } & Pick<AppProps, "shellSlots" | "globalViews"> = {},
) {
  const { route = DEFAULT_ROUTE, bridge = true, waitConnected = true, shellSlots, globalViews } = options;
  const services = createServices({
    connector: bridge ? fake : undefined,
    initialRoute: route,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
  const user = userEvent.setup();
  const view = render(<App services={services} shellSlots={shellSlots} globalViews={globalViews} />);
  if (bridge && waitConnected) await connected();
  return { services, user, view };
}

const original = window.matchMedia;

/** Emulates the CSS media queries the shell uses for a given window width. */
function setViewport(width: number) {
  window.matchMedia = ((query: string) => {
    const max = /max-width:\s*(\d+)px/.exec(query);
    return {
      matches: max ? width <= Number(max[1]) : false,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    };
  }) as unknown as typeof window.matchMedia;
}

afterEach(() => {
  window.matchMedia = original;
  window.location.hash = "";
});

const GLOBAL_ROUTES: Array<[string, Route, string]> = [
  ["workspace", { name: "global", view: "workspace" }, "Orchestrator Workspace"],
  ["workstreams", { name: "global", view: "workstreams" }, "Workstreams"],
  ["agents", { name: "global", view: "agents" }, "Agents & Sessions"],
  ["review", { name: "global", view: "review" }, "Review & Decisions"],
];

const LEGACY_ROUTES: Array<[string, Route]> = [
  ["projects", { name: "projects" }],
  ["project", { name: "project", projectId: REPO }],
  ["tasks", { name: "tasks", projectId: REPO, filter: "all" }],
  ["task", { name: "task", projectId: REPO, taskId: "t1" }],
  ["coordinator", { name: "coordinator", projectId: REPO }],
  ["review", { name: "review", projectId: REPO, taskId: null }],
];

function daemon() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [mkTask("t1", REPO, { state: "Running" }), mkTask("t2", REPO, { state: "Queued" })];
  fake.approvals = [mkApproval("a1", "t1"), mkApproval("a2", "t2", "approved")];
  return fake;
}

describe("global routes in the shell", () => {
  it.each(GLOBAL_ROUTES)("renders %s inside the main landmark with aria-current on its nav item", async (_name, route, title) => {
    await mount(daemon(), { route });
    const main = screen.getByRole("main");
    expect(within(main).getByRole("heading", { level: 1, name: title })).toBeInTheDocument();
    expect(screen.getAllByRole("main")).toHaveLength(1);
    const current = document.querySelectorAll('.shell [aria-current="page"]');
    expect(current).toHaveLength(1);
    expect(current[0]).toHaveTextContent(title);
  });

  it("defaults to the workspace and shows its activity aside only there", async () => {
    await mount(daemon());
    expect(screen.getByRole("heading", { level: 1, name: "Orchestrator Workspace" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Workspace activity" })).toBeInTheDocument();
    expect(screen.getByTestId("slot-activity")).toBeInTheDocument();
  });

  it("omits the activity aside outside the workspace view", async () => {
    await mount(daemon(), { route: { name: "global", view: "agents" } });
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("renders supplied global view content inside the shell's main", async () => {
    await mount(daemon(), {
      route: { name: "global", view: "agents" },
      globalViews: { agents: () => <h1>Agents view</h1> },
    });
    expect(within(screen.getByRole("main")).getByRole("heading", { name: "Agents view" })).toBeInTheDocument();
  });

  it("shows a placeholder for a global view with no content yet", async () => {
    await mount(daemon(), { route: { name: "global", view: "agents" } });
    expect(within(screen.getByRole("main")).getByTestId("global-view-agents")).toBeInTheDocument();
  });
});

describe("legacy project routes in the shell", () => {
  it.each(LEGACY_ROUTES)("%s resolves with exactly one main landmark and Projects selected", async (_name, route) => {
    await mount(daemon(), { route });
    expect(screen.getAllByRole("main")).toHaveLength(1);
    const current = document.querySelectorAll('.shell [aria-current="page"]');
    expect(current.length).toBeGreaterThanOrEqual(1);
    const section = document.querySelector('.shell-nav [aria-current]');
    expect(section).toHaveAttribute("aria-current", "page");
    expect(section).toHaveTextContent("Projects");
  });
});

describe("ConnectionStrip hosting", () => {
  it.each([...GLOBAL_ROUTES.map(([name, route]) => [`global ${name}`, route] as const), ...LEGACY_ROUTES.map(([name, route]) => [`legacy ${name}`, route] as const)])(
    "is visible on %s in a connected native boot",
    async (_name, route) => {
      await mount(daemon(), { route });
      expect(within(screen.getByTestId("shell-connection")).getByTestId("connection-status")).toHaveTextContent("Connected");
    },
  );

  it("is visible in fixture mode on a global route", async () => {
    await mount(new FakeDaemon(), { bridge: false, route: { name: "global", view: "review" } });
    expect(within(screen.getByTestId("shell-connection")).getByTestId("fixture-banner")).toBeInTheDocument();
  });

  it("is visible when the daemon is unavailable (no-bridge style failure)", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    await mount(fake, { waitConnected: false, route: { name: "global", view: "agents" } });
    await waitFor(() =>
      expect(within(screen.getByTestId("shell-connection")).getByTestId("connection-status")).not.toHaveTextContent(
        "Connected",
      ),
    );
  });

  it("exposes the connection strip as a top-level named banner containing the live status", async () => {
    await mount(daemon(), { route: { name: "global", view: "agents" } });
    const banner = screen.getByRole("banner", { name: "Connection status" });
    expect(banner).toHaveAttribute("data-testid", "shell-connection");
    expect(banner.parentElement).toHaveClass("shell");
    const status = within(banner).getByTestId("connection-status");
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveTextContent("Connected");
  });
});

describe("navigation", () => {
  it("exposes landmarks: two named navs, one main, and no nested main", async () => {
    await mount(daemon());
    expect(screen.getByRole("navigation", { name: "Primary views" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Workstreams" })).toBeInTheDocument();
    expect(screen.getAllByRole("main")).toHaveLength(1);
  });

  it("offers a skip link that moves focus to main without changing the route", async () => {
    const { user, services } = await mount(daemon());
    const route = services.router.getSnapshot();
    await user.tab();
    const skip = screen.getByRole("link", { name: "Skip to main content" });
    expect(skip).toHaveFocus();
    await user.keyboard("{Enter}");
    const main = screen.getByRole("main");
    expect(main).toHaveFocus();
    expect(window.getComputedStyle(main).outlineWidth).toBe("3px");
    expect(services.router.getSnapshot()).toEqual(route);
  });

  it("is keyboard operable: focus a link and press Enter", async () => {
    const { user, services } = await mount(daemon());
    const nav = screen.getByRole("navigation", { name: "Primary views" });
    const agents = within(nav).getByRole("link", { name: /Agents & Sessions/ });
    agents.focus();
    expect(agents).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
    expect(within(nav).getByRole("link", { name: /Agents & Sessions/ })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("link", { name: /Orchestrator Workspace/ })).not.toHaveAttribute("aria-current");
  });

  it("tabs through every primary link in order", async () => {
    const { user } = await mount(daemon());
    const nav = screen.getByRole("navigation", { name: "Primary views" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual([
      expect.stringContaining("Orchestrator Workspace"),
      expect.stringContaining("Agents & Sessions"),
      expect.stringContaining("Review & Decisions"),
      expect.stringContaining("Projects"),
    ]);
    links[0].focus();
    for (const link of links.slice(1)) {
      await user.tab();
      expect(link).toHaveFocus();
    }
  });

  it("reaches the Workstreams heading link and the legacy project list (S1)", async () => {
    const { user, services } = await mount(daemon());
    await user.click(within(screen.getByRole("navigation", { name: "Workstreams" })).getByRole("link", { name: /Workstreams/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
    await user.click(within(screen.getByRole("navigation", { name: "Primary views" })).getByRole("link", { name: /Projects/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "projects" });
    expect(await screen.findByText("repo")).toBeInTheDocument();
  });

  it("links carry the hash form so deep links and new-window opens work", async () => {
    await mount(daemon());
    const nav = screen.getByRole("navigation", { name: "Primary views" });
    expect(within(nav).getByRole("link", { name: /Review & Decisions/ })).toHaveAttribute("href", "#/review");
    expect(within(nav).getByRole("link", { name: /Projects/ })).toHaveAttribute("href", "#/projects");
  });

  it("follows the browser hash: deep link in, back and forward out", async () => {
    window.location.hash = "#/agents";
    const { services, user } = await mount(daemon(), { route: { name: "global", view: "agents" } });
    const off = syncRouterToHash(services.router);
    try {
      expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
      await user.click(within(screen.getByRole("navigation", { name: "Primary views" })).getByRole("link", { name: /Review & Decisions/ }));
      expect(window.location.hash).toBe("#/review");
      await act(async () => {
        window.history.back();
        await new Promise((resolve) => setTimeout(resolve, 30));
      });
      expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
      await act(async () => {
        window.history.forward();
        await new Promise((resolve) => setTimeout(resolve, 30));
      });
      expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review" });
    } finally {
      off();
    }
  });
});

describe("persistent WorkstreamTree task freshness", () => {
  it("shows retained AppData task state warnings after navigating away from Workstreams", async () => {
    const fake = daemon();
    installFakeWorkstreams(fake, {
      workstreams: [{ id: "ws-1", title: "Sessions", summary: "", state: "active", revision: 1 }],
      links: [{ task_id: "t1", workstream_id: "ws-1", objective_id: null }],
    });
    const { user, services } = await mount(fake, {
      route: { name: "global", view: "workstreams" },
      shellSlots: workstreamsShellSlots,
      globalViews: workstreamsGlobalViews,
    });
    const tree = await screen.findByTestId("workstream-tree");
    await within(tree).findByRole("treeitem", { name: /objective of t1/ });
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    const retained = services.data.getSnapshot().tasks.data;
    expect(retained?.find((task) => task.contract.id === "t1")?.state).toBe("Running");

    fake.tasks = fake.tasks.map((task) => (task.contract.id === "t1" ? { ...task, state: "Failed" } : task));
    fake.failNext("tasks.list", { code: -32603, message: "task refresh failed" });
    await act(async () => services.data.refreshAll());
    expect(services.data.getSnapshot().tasks).toMatchObject({ status: "error", data: retained });

    await user.click(within(screen.getByRole("navigation", { name: "Primary views" })).getByRole("link", { name: /Agents & Sessions/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Agents & Sessions" })).toBeInTheDocument();
    const persistentTree = screen.getByTestId("workstream-tree");
    expect(within(persistentTree).getByRole("treeitem", { name: /objective of t1/ })).toHaveTextContent("Running");
    expect(within(persistentTree).getByTestId("workstream-tree-task-list-status")).toHaveAttribute("role", "alert");
    expect(within(persistentTree).getByTestId("workstream-tree-task-list-status")).toHaveTextContent(/retained data that may be stale/);
  });
});
describe("counts and placeholders", () => {
  it("derives the review count and top-bar counts from the same lists", async () => {
    await mount(daemon(), { route: { name: "global", view: "agents" } });
    await waitFor(() => expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1"));
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("1 pending approvals");
    const counts = screen.getByTestId("top-counts");
    expect(counts).toHaveTextContent("2 tasks");
    expect(counts).toHaveTextContent("1 running");
    expect(counts).toHaveTextContent("1 require attention");
  });

  it("shows a placeholder, never 0, where no list has loaded", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    await mount(fake, { waitConnected: false });
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("—");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("— tasks");
    expect(screen.getByTestId("slot-top-usage")).toHaveTextContent("Usage: not available");
    expect(screen.getByTestId("slot-resources")).toHaveTextContent("not available");
  });
});

describe("slots", () => {
  it("places supplied slot content in the matching regions", async () => {
    await mount(daemon(), {
      shellSlots: {
        reviewCount: <span data-testid="s-review">R</span>,
        workstreamsCount: <span data-testid="s-wcount">W</span>,
        workstreamsTree: <ul data-testid="s-tree" />,
        themeToggle: <button type="button">Dark mode</button>,
        topUsage: <span data-testid="s-usage">U</span>,
        topSearch: <span data-testid="s-search">S</span>,
        topCounts: <span data-testid="s-counts">C</span>,
        activity: <p data-testid="s-activity">A</p>,
        resources: <p data-testid="s-resources">X</p>,
      },
    });
    const primary = screen.getByRole("navigation", { name: "Primary views" });
    expect(within(primary).getByTestId("s-review")).toBeInTheDocument();
    const tree = screen.getByRole("navigation", { name: "Workstreams" });
    expect(within(tree).getByTestId("s-wcount")).toBeInTheDocument();
    expect(within(tree).getByTestId("s-tree")).toBeInTheDocument();
    expect(within(screen.getByTestId("slot-theme-toggle")).getByRole("button", { name: "Dark mode" })).toBeInTheDocument();
    const top = screen.getByRole("region", { name: "Workspace summary" });
    for (const id of ["s-usage", "s-search", "s-counts"]) expect(within(top).getByTestId(id)).toBeInTheDocument();
    expect(within(screen.getByRole("complementary", { name: "Workspace activity" })).getByTestId("s-activity")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Device resources" })).getByTestId("s-resources")).toBeInTheDocument();
  });

  it("renders the M7-01 theme toggle in the shell by default and switches the theme", async () => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    const { user } = await mount(daemon());
    const toggle = within(screen.getByTestId("slot-theme-toggle")).getByRole("button", { name: /dark mode/i });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
  });

  it("removes the activity and resource regions when their slots are null", async () => {
    await mount(daemon(), { shellSlots: { activity: null, resources: null } });
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Device resources" })).not.toBeInTheDocument();
  });
});

describe("responsive behaviour", () => {
  it.each([
    [1280, "full"],
    [1150, "medium"],
    [960, "stacked"],
    [760, "narrow"],
    [480, "narrow"],
  ])("at %i px (tier %s) every region is present", async (width, tier) => {
    setViewport(width);
    await mount(daemon());
    expect(document.querySelector(".shell")).toHaveAttribute("data-shell-tier", tier);
    expect(screen.getByTestId("shell-connection")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Workspace summary" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Workspace activity" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Device resources" })).toBeInTheDocument();
    expect(screen.getByTestId("shell-sidebar")).toBeInTheDocument();
  });

  it.each([
    [1280, 0],
    [1150, 0],
    [960, -1],
    [760, -1],
    [390, -1],
  ])("at %i px the main region has tabindex %i (focusable only where it is the scroller)", async (width, tabIndex) => {
    setViewport(width);
    await mount(daemon());
    expect(screen.getByRole("main")).toHaveAttribute("tabindex", String(tabIndex));
  });

  it("keeps the sidebar open and has no menu button above the narrow breakpoint", async () => {
    setViewport(961);
    await mount(daemon());
    expect(screen.queryByRole("button", { name: "Menu" })).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Primary views" })).toBeVisible();
  });

  it.each([760, 480])("keeps primary navigation visible and keyboard operable at %i px", async (width) => {
    setViewport(width);
    const { user, services } = await mount(daemon());
    const nav = screen.getByRole("navigation", { name: "Primary views" });
    expect(nav).toBeVisible();
    expect(screen.queryByRole("button", { name: "Menu" })).not.toBeInTheDocument();
    expect(within(nav).getAllByRole("link")).toHaveLength(4);
    expect(screen.getByRole("navigation", { name: "Workstreams" })).toBeVisible();

    const agents = within(nav).getByRole("link", { name: /Agents & Sessions/ });
    agents.focus();
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
    expect(screen.getByRole("navigation", { name: "Primary views" })).toBeVisible();
    expect(screen.getByTestId("shell-connection")).toBeInTheDocument();
  });
});
