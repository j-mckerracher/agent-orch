import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App, { type AppProps } from "../../App";
import { DaemonError } from "../../daemon/errors";
import { createFixtureSources } from "../../sources/fixtures";
import { FakeDaemon, mkApproval, mkTask } from "../../test/fakeDaemon";
import { REPO, connected } from "../../test/render";
import { chooseAction } from "../../test/workstreamsUi";
import { createServices } from "../createServices";
import { DEFAULT_ROUTE, syncRouterToHash, type Route } from "../router";
import { workstreamsGlobalViews } from "../views/workstreams/register";
import { ResourcesSlot } from "./ResourcesSlot";

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

/** Name, route, the view's h1 and the label of its nav link. */
const GLOBAL_ROUTES: Array<[string, Route, string, string]> = [
  ["workspace", { name: "global", view: "workspace" }, "Orchestrator Workspace", "Workspace"],
  ["workstreams", { name: "global", view: "workstreams" }, "Workstreams", "Workstreams"],
  ["agents", { name: "global", view: "agents" }, "Agents & Sessions", "Agents"],
  ["review", { name: "global", view: "review" }, "Review & Decisions", "Review"],
  ["settings", { name: "global", view: "settings" }, "Settings", "Settings"],
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
  it.each(GLOBAL_ROUTES)("renders %s inside the main landmark with aria-current on its nav item", async (_name, route, title, label) => {
    await mount(daemon(), { route });
    const main = screen.getByRole("main");
    expect(within(main).getByRole("heading", { level: 1, name: title })).toBeInTheDocument();
    expect(screen.getAllByRole("main")).toHaveLength(1);
    const current = document.querySelectorAll('.shell [aria-current="page"]');
    expect(current).toHaveLength(1);
    expect(current[0]).toHaveTextContent(label);
  });

  it("defaults to the workspace and shows its activity control only there", async () => {
    await mount(daemon());
    expect(screen.getByRole("heading", { level: 1, name: "Orchestrator Workspace" })).toBeInTheDocument();
    expect(screen.getByTestId("shell-activity")).toBeInTheDocument();
    expect(screen.getByTestId("slot-activity")).toBeInTheDocument();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("omits the activity control outside the workspace view", async () => {
    await mount(daemon(), { route: { name: "global", view: "agents" } });
    expect(screen.queryByTestId("shell-activity")).not.toBeInTheDocument();
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
  it.each(LEGACY_ROUTES)("%s resolves with exactly one main landmark and Workstreams selected", async (_name, route) => {
    await mount(daemon(), { route });
    expect(screen.getAllByRole("main")).toHaveLength(1);
    const current = document.querySelectorAll('.shell [aria-current="page"]');
    expect(current.length).toBeGreaterThanOrEqual(1);
    const section = document.querySelector('.shell-sidebar [aria-current="page"]');
    expect(section).toHaveTextContent("Workstreams");
  });
});

describe("ConnectionStrip hosting", () => {
  it.each([...GLOBAL_ROUTES.map(([name, route]) => [`global ${name}`, route] as const), ...LEGACY_ROUTES.map(([name, route]) => [`legacy ${name}`, route] as const)])(
    "is hidden on %s while the connection is healthy",
    async (_name, route) => {
      await mount(daemon(), { route });
      expect(screen.getByTestId("shell-connection")).toBeEmptyDOMElement();
      expect(screen.queryByTestId("connection-status")).not.toBeInTheDocument();
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

  it("exposes the strip, while the connection is down, as a top-level named banner containing the live status", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    await mount(fake, { waitConnected: false, route: { name: "global", view: "agents" } });
    const banner = screen.getByRole("banner", { name: "Connection status" });
    expect(banner).toHaveAttribute("data-testid", "shell-connection");
    expect(banner.parentElement).toHaveClass("shell-body");
    const status = within(banner).getByTestId("connection-status");
    expect(status).toHaveAttribute("role", "status");
    expect(status).not.toHaveTextContent("Connected");
  });

  it("keeps the skip link working while the strip is shown", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    const { user } = await mount(fake, { waitConnected: false });
    await user.tab();
    expect(screen.getByRole("link", { name: "Skip to main content" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("main")).toHaveFocus();
  });
});

describe("navigation", () => {
  it("exposes landmarks: one named nav, one main, and no nested main", async () => {
    await mount(daemon());
    expect(screen.getAllByRole("navigation")).toHaveLength(1);
    expect(screen.getByRole("navigation", { name: "Primary views" })).toBeInTheDocument();
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
    const agents = within(nav).getByRole("link", { name: /^Agents/ });
    agents.focus();
    expect(agents).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
    expect(within(nav).getByRole("link", { name: /^Agents/ })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("link", { name: /^Workspace/ })).not.toHaveAttribute("aria-current");
  });

  it("tabs through every primary link in order", async () => {
    const { user } = await mount(daemon());
    const nav = screen.getByRole("navigation", { name: "Primary views" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual([
      expect.stringContaining("Workspace"),
      expect.stringContaining("Workstreams"),
      expect.stringContaining("Agents"),
      expect.stringContaining("Review"),
      expect.stringContaining("Settings"),
    ]);
    links[0].focus();
    for (const link of links.slice(1)) {
      await user.tab();
      expect(link).toHaveFocus();
    }
  });

  it("reaches the Workstreams nav link and, from its view, the legacy project list (S1)", async () => {
    const { user, services } = await mount(daemon(), { globalViews: workstreamsGlobalViews });
    await user.click(within(screen.getByRole("navigation", { name: "Primary views" })).getByRole("link", { name: /^Workstreams/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
    await chooseAction(user, "More actions", "Manage projects");
    expect(services.router.getSnapshot()).toEqual({ name: "projects" });
    expect(await screen.findByText("repo")).toBeInTheDocument();
  });

  it("links carry the hash form so deep links and new-window opens work", async () => {
    await mount(daemon());
    const nav = screen.getByRole("navigation", { name: "Primary views" });
    expect(within(nav).getByRole("link", { name: /^Review/ })).toHaveAttribute("href", "#/review");
    expect(within(nav).getByRole("link", { name: /^Agents/ })).toHaveAttribute("href", "#/agents");
    expect(within(nav).getByRole("link", { name: /^Settings/ })).toHaveAttribute("href", "#/settings");
    expect(within(nav).getByRole("link", { name: /^Workstreams/ })).toHaveAttribute("href", "#/workstreams");
  });

  it("has exactly five persistent destinations, with no workstream tree", async () => {
    await mount(daemon());
    const links = within(screen.getByRole("navigation", { name: "Primary views" })).getAllByRole("link");
    expect(links.map((link) => link.textContent?.trim())).toEqual([
      expect.stringContaining("Workspace"),
      expect.stringContaining("Workstreams"),
      expect.stringContaining("Agents"),
      expect.stringContaining("Review"),
      expect.stringContaining("Settings"),
    ]);
    expect(screen.queryByRole("link", { name: "Projects" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tree")).not.toBeInTheDocument();
    expect(screen.queryByTestId("workstream-tree")).not.toBeInTheDocument();
  });

  it("keeps projects reachable from the Workstreams view and not from any primary destination", async () => {
    const { user } = await mount(daemon(), { route: { name: "global", view: "workstreams" }, globalViews: workstreamsGlobalViews });
    expect(within(screen.getByRole("navigation", { name: "Primary views" })).queryByRole("link", { name: /Projects/ })).toBeNull();
    await chooseAction(user, "More actions", "Manage projects");
    expect(await screen.findByText("repo")).toBeInTheDocument();
  });

  it("follows the browser hash: deep link in, back and forward out", async () => {
    window.location.hash = "#/agents";
    const { services, user } = await mount(daemon(), { route: { name: "global", view: "agents" } });
    const off = syncRouterToHash(services.router);
    try {
      expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
      await user.click(within(screen.getByRole("navigation", { name: "Primary views" })).getByRole("link", { name: /^Review/ }));
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

describe("counts and placeholders", () => {
  it("derives the nav badges and the one need-you count from the same lists", async () => {
    await mount(daemon(), { route: { name: "global", view: "agents" } });
    await waitFor(() => expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1"));
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("1 pending decisions: 1 approvals, 0 proposed actions");
    expect(screen.getByTestId("nav-agents-count")).toHaveTextContent("1");
    expect(screen.getByTestId("nav-agents-count")).toHaveAccessibleName("1 running");
    expect(screen.getByTestId("top-need-you")).toHaveTextContent("1 need you");
  });

  it("shows one need-you count linking to Review and no four-count line", async () => {
    const { user, services } = await mount(daemon(), { route: { name: "global", view: "agents" } });
    expect(screen.queryByTestId("top-counts")).not.toBeInTheDocument();
    expect(screen.queryByText(/require attention/)).not.toBeInTheDocument();
    expect(screen.queryByTestId("top-counts-placeholder")).not.toBeInTheDocument();
    const top = screen.getByRole("region", { name: "Workspace summary" });
    await waitFor(() => expect(within(top).getByTestId("top-need-you")).toHaveTextContent("1 need you"));
    expect(within(top).getAllByRole("link")).toHaveLength(1);
    await user.click(within(top).getByRole("link", { name: /need you/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review" });
  });

  it("shows a placeholder, never 0, where no list has loaded; the running badge stays hidden", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    await mount(fake, { waitConnected: false });
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("—");
    expect(screen.getByTestId("top-need-you")).toHaveTextContent("— need you");
    expect(screen.getByTestId("top-need-you")).not.toHaveTextContent(/\b0\b/);
    expect(screen.queryByTestId("nav-agents-count")).not.toBeInTheDocument();
    expect(screen.getByTestId("slot-top-usage")).toHaveTextContent("Usage: not available");
    expect(screen.getByTestId("slot-resources")).toHaveTextContent("not available");
    expect(screen.getByTestId("slot-resources").closest('[data-testid="shell-activity"]')).not.toBeNull();
  });

  it("hides the running badge when no task runs", async () => {
    const fake = daemon();
    fake.tasks = [mkTask("t2", REPO, { state: "Queued" })];
    await mount(fake, { route: { name: "global", view: "agents" } });
    await waitFor(() => expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1"));
    expect(screen.queryByTestId("nav-agents-count")).not.toBeInTheDocument();
  });
});

describe("slots", () => {
  it("places supplied slot content in the matching regions", async () => {
    await mount(daemon(), {
      shellSlots: {
        agentsCount: <span data-testid="s-agents">G</span>,
        reviewCount: <span data-testid="s-review">R</span>,
        topUsage: <span data-testid="s-usage">U</span>,
        topSearch: <span data-testid="s-search">S</span>,
        topNeedYou: <span data-testid="s-need">C</span>,
        activity: (
          <div data-testid="s-activity">
            A<ResourcesSlot />
          </div>
        ),
        resources: <p data-testid="s-resources">X</p>,
      },
    });
    const primary = screen.getByRole("navigation", { name: "Primary views" });
    expect(within(primary).getByTestId("s-review")).toBeInTheDocument();
    expect(within(primary).getByTestId("s-agents")).toBeInTheDocument();
    const top = screen.getByRole("region", { name: "Workspace summary" });
    for (const id of ["s-usage", "s-search", "s-need"]) expect(within(top).getByTestId(id)).toBeInTheDocument();
    const activity = within(screen.getByTestId("shell-activity")).getByTestId("s-activity");
    expect(within(activity).getByTestId("s-resources")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Device resources" })).not.toBeInTheDocument();
  });

  it("has no theme toggle in the shell: it lives in Settings > Appearance", async () => {
    await mount(daemon());
    expect(screen.queryByRole("button", { name: /dark mode/i })).not.toBeInTheDocument();
    expect(screen.queryByTestId("slot-theme-toggle")).not.toBeInTheDocument();
  });

  it("removes the activity control and System Usage when their slots are null", async () => {
    await mount(daemon(), { shellSlots: { activity: null, resources: null } });
    expect(screen.queryByTestId("shell-activity")).not.toBeInTheDocument();
    expect(screen.queryByTestId("slot-resources")).not.toBeInTheDocument();
  });

  it("keeps the activity control, without System Usage, when resources is null", async () => {
    await mount(daemon(), { shellSlots: { resources: null } });
    expect(screen.getByTestId("shell-activity")).toBeInTheDocument();
    expect(screen.queryByTestId("slot-resources")).not.toBeInTheDocument();
  });
});

describe("responsive behavior", () => {
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
    expect(screen.getByTestId("shell-activity")).toBeInTheDocument();
    expect(screen.getByTestId("slot-resources")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Device resources" })).not.toBeInTheDocument();
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
    expect(within(nav).getAllByRole("link")).toHaveLength(5);

    const agents = within(nav).getByRole("link", { name: /^Agents/ });
    agents.focus();
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
    expect(screen.getByRole("navigation", { name: "Primary views" })).toBeVisible();
    expect(screen.getByTestId("shell-connection")).toBeInTheDocument();
  });
});
