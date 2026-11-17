import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { PullRequests } from "./PullRequests";

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

const wire = (over: Record<string, unknown> = {}) => ({
  task_id: "t-1",
  number: 42,
  state: "open",
  head_sha: "abc123def4567890abc123def4567890abc12345",
  base_branch: "main",
  branch: "orchestrator/t-1",
  mergeable: true,
  required_total: 3,
  required_passed: 3,
  required_failed: 0,
  checks: [
    { name: "build", result: "pass" },
    { name: "lint", result: "pass" },
    { name: "tests", result: "pass" },
  ],
  title: "Retry the sync call",
  observed_age_secs: 40,
  merge: null,
  ...over,
});

function mount(services: Services) {
  render(
    <ServicesProvider services={services}>
      <PullRequests />
    </ServicesProvider>,
  );
}

async function live(setup: (fake: FakeDaemon) => void) {
  const fake = new FakeDaemon();
  setup(fake);
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  mount(services);
  return { fake, services };
}

describe("pull request list", () => {
  it("landing_list_fixture_rows_say_fixture_data_with_state_and_checks_in_words", async () => {
    mount(createServices());
    const row = await screen.findByTestId("pull-42");
    expect(row).toHaveTextContent("Pull request 42: Sample: tidy the docs");
    expect(within(row).getByText("Open")).toBeTruthy();
    expect(within(row).getByText("Fixture data")).toBeTruthy();
    expect(row).toHaveTextContent("Required checks 2 of 2 passed");
    expect(screen.getByTestId("pull-43")).toHaveTextContent("Required checks 1 of 2 passed, 1 failed");
    expect(screen.getByTestId("pull-43")).toHaveTextContent("tests: Failed");
    expect(within(row).getByLabelText("Commit abc123def456")).toHaveTextContent("abc123def456");
  });

  it("landing_list_empty_says_so_and_how_a_row_appears", async () => {
    const services = createServices({ surfaces: createFixtureSurfaces({ scenario: "empty" }) });
    mount(services);
    expect(await screen.findByText("No pull requests are being watched. Open one from a task branch and it appears here.")).toBeTruthy();
  });

  it("landing_list_unavailable_says_why_and_loading_says_loading", async () => {
    mount(createServices({ surfaces: createFixtureSurfaces({ scenario: "unavailable" }) }));
    expect(await screen.findByText("Pull requests are not available")).toBeTruthy();
    cleanup();
    mount(createServices({ surfaces: createFixtureSurfaces({ scenario: "loading" }) }));
    expect(screen.getByText("Loading pull requests…")).toBeTruthy();
  });

  it("landing_list_unsupported_fixture_scenario_renders_nothing", async () => {
    mount(createServices({ surfaces: createFixtureSurfaces({ scenario: "unsupported" }) }));
    await waitFor(() => expect(screen.queryByText("Loading pull requests…")).toBeNull());
    expect(screen.queryByRole("heading", { name: "Pull requests" })).toBeNull();
  });

  it("landing_list_hides_quietly_and_never_asks_when_the_daemon_has_no_landing_capability", async () => {
    const { fake } = await live((f) => {
      f.unknownCapabilities = ["landing"];
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByRole("heading", { name: "Pull requests" })).toBeNull();
    expect(screen.queryByText(/not available/)).toBeNull();
    expect(fake.callsTo("pulls.list")).toEqual([]);
  });

  it("landing_list_live_row_shows_merge_state_and_a_merged_row_has_no_refresh", async () => {
    await live((f) => {
      f.stub("pulls.list", {
        pulls: [
          wire({ merge: { state: "issued", refusal: null } }),
          wire({ task_id: "t-2", number: 7, state: "merged", head_sha: "b".repeat(40) }),
        ],
      });
    });
    const row = await screen.findByTestId("pull-42");
    expect(row).toHaveTextContent("Checking whether it merged");
    expect(within(row).queryByText("Fixture data")).toBeNull();
    expect(within(screen.getByTestId("pull-7")).getByText("Merged")).toBeTruthy();
    expect(within(screen.getByTestId("pull-7")).queryByRole("button")).toBeNull();
  });

  it("landing_list_hostile_title_branch_and_check_names_render_as_text_with_no_link_or_markup", async () => {
    await live((f) => {
      f.stub("pulls.list", {
        pulls: [
          wire({
            title: '<a href="https://evil.example">click</a><img src=x onerror=alert(1)>',
            branch: "<b>bold</b>",
            checks: [{ name: "<script>boom()</script>", result: "pass" }],
          }),
        ],
      });
    });
    const row = await screen.findByTestId("pull-42");
    expect(row).toHaveTextContent('<a href="https://evil.example">click</a>');
    expect(row).toHaveTextContent("<script>boom()</script>: Passed");
    expect(row.querySelector("a, img, script, b")).toBeNull();
  });
});

describe("refresh", () => {
  it("landing_refresh_asks_the_daemon_once_updates_the_row_says_so_politely_and_waits_before_the_next", async () => {
    const { fake } = await live((f) => {
      f.stub("pulls.list", { pulls: [wire({ required_passed: 1, required_failed: 0 })] });
      f.stub("pulls.refresh", wire({ required_passed: 3, observed_age_secs: 0 }));
    });
    const user = userEvent.setup();
    await screen.findByTestId("pull-42");
    const button = screen.getByRole("button", { name: "Refresh pull request 42" });
    await user.click(button);
    await waitFor(() => expect(screen.getByTestId("pulls-status")).toHaveTextContent("Read pull request 42 again: Required checks 3 of 3 passed."));
    expect(screen.getByTestId("pulls-status")).toHaveAttribute("aria-live", "polite");
    expect(fake.callsTo("pulls.refresh")).toEqual([expect.objectContaining({ params: { task_id: "t-1" } })]);
    const after = screen.getByRole("button", { name: "Refresh pull request 42" });
    expect(after).toBeDisabled();
    expect(after).toHaveAccessibleDescription(/available again in about 10 seconds/);
    await user.click(after);
    expect(fake.callsTo("pulls.refresh")).toHaveLength(1);
  });

  it("landing_refresh_rate_limit_answer_says_wait_and_keeps_the_row", async () => {
    const { fake } = await live((f) => {
      f.stub("pulls.list", { pulls: [wire()] });
    });
    fake.failNext("pulls.refresh", { code: -32011, message: "rate limited" });
    await screen.findByTestId("pull-42");
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: "Refresh pull request 42" }));
    });
    await waitFor(() => expect(screen.getByTestId("pulls-status")).toHaveTextContent("Wait about 10 seconds and try again."));
    expect(screen.getByTestId("pull-42")).toHaveTextContent("Required checks 3 of 3 passed");
  });
});
