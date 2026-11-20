import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { renderSettingsPage } from "../../../test/settingsPage";
import { createServices } from "../../createServices";
import { PAGES, SETTINGS, searchSettings } from "./settingsPages";

afterEach(cleanup);

const fixture = () => createServices({ surfaces: createFixtureSurfaces() });
const menu = () => within(screen.getByRole("navigation", { name: "Settings pages" }));

describe("Settings pages", () => {
  it("settings_has_seven_pages_in_a_left_menu", () => {
    renderSettingsPage(fixture());
    expect(menu().getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Agents",
      "Limits",
      "Autonomy",
      "Repositories",
      "Memory",
      "Notifications",
      "Appearance",
    ]);
    expect(PAGES.map((page) => page.id)).toEqual(["agents", "limits", "autonomy", "repositories", "memory", "notifications", "appearance"]);
  });

  it("settings_opens_the_first_page_for_a_bare_or_unknown_route", () => {
    for (const page of [undefined, "nonsense"]) {
      const view = renderSettingsPage(fixture(), page);
      expect(menu().getByRole("button", { name: "Agents" })).toHaveAttribute("aria-current", "page");
      expect(screen.getByRole("heading", { level: 2, name: "Agents" })).toBeInTheDocument();
      view.unmount();
    }
  });

  it("settings_shows_one_page_at_a_time_and_a_menu_click_changes_the_route", async () => {
    const user = userEvent.setup();
    const services = fixture();
    renderSettingsPage(services, "agents");
    expect(await screen.findByLabelText("Default provider")).toBeInTheDocument();
    expect(screen.queryByLabelText("CPU limit (percent)")).not.toBeInTheDocument();
    await user.click(menu().getByRole("button", { name: "Limits" }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "settings", settingsPage: "limits" });
    expect(await screen.findByLabelText("CPU limit (percent)")).toBeInTheDocument();
    expect(screen.queryByLabelText("Default provider")).not.toBeInTheDocument();
    expect(menu().getByRole("button", { name: "Limits" })).toHaveAttribute("aria-current", "page");
    expect(menu().getByRole("button", { name: "Agents" })).not.toHaveAttribute("aria-current");
  });

  it("settings_each_page_holds_what_the_wireframe_lists", async () => {
    const holds: Record<string, string[]> = {
      agents: ["Default provider"],
      limits: ["CPU limit (percent)", "Check interval with one task running (minutes)", "Replies per hour, per conversation"],
      autonomy: ["Let the orchestrator act on its own"],
      repositories: ["Home directory"],
    };
    for (const [page, labels] of Object.entries(holds)) {
      const view = renderSettingsPage(fixture(), page);
      for (const label of labels) expect(await screen.findByLabelText(label, { selector: "input, select" })).toBeInTheDocument();
      view.unmount();
    }
    renderSettingsPage(fixture(), "appearance");
    expect(await screen.findByRole("button", { name: /dark mode/i })).toBeInTheDocument();
  });

  it("settings_agents_keeps_adapters_in_a_collapsed_disclosure", async () => {
    renderSettingsPage(fixture(), "agents");
    expect(await screen.findByTestId("installed-adapters")).not.toHaveAttribute("open");
    expect(screen.getByText("Installed adapters")).toBeInTheDocument();
  });

  it("settings_agents_adapters_disclosure_holds_the_inventory_and_agent_detection", async () => {
    renderSettingsPage(fixture(), "agents");
    const adapters = await screen.findByTestId("installed-adapters");
    expect(within(adapters).getByRole("heading", { level: 3, name: "Agent detection" })).toBeInTheDocument();
    expect(await within(adapters).findByText(/Launch available: no\./)).toBeInTheDocument();
    expect(within(adapters).getByRole("region", { name: "Detected agents (placeholder)" })).toBeInTheDocument();
  });

  it("settings_notifications_page_holds_the_decision_notification_controls_without_a_save_bar", () => {
    renderSettingsPage(fixture(), "notifications");
    expect(screen.getByRole("heading", { level: 2, name: "Notifications" })).toBeInTheDocument();
    const section = screen.getByRole("region", { name: "Decision notifications" });
    expect(within(section).getByRole("heading", { level: 3, name: "Decision notifications" })).toBeInTheDocument();
    expect(screen.queryByTestId("settings-savebar")).not.toBeInTheDocument();
  });

  it("settings_has_one_save_bar_on_a_page_with_fields_and_none_on_a_page_of_switches", async () => {
    const limits = renderSettingsPage(fixture(), "limits");
    await screen.findByLabelText("CPU limit (percent)");
    await screen.findByRole("button", { name: /^Save/ });
    expect(screen.getAllByRole("button", { name: /^Save/ })).toHaveLength(1);
    expect(screen.getAllByTestId("settings-savebar")).toHaveLength(1);
    limits.unmount();
    const repos = renderSettingsPage(fixture(), "repositories");
    await screen.findByRole("textbox", { name: "Home directory" });
    await screen.findByRole("button", { name: /^Save/ });
    expect(screen.getAllByRole("button", { name: /^Save/ })).toHaveLength(1);
    repos.unmount();
    renderSettingsPage(fixture(), "autonomy");
    await screen.findByRole("switch", { name: "Let the orchestrator act on its own" });
    expect(screen.queryByRole("button", { name: /^Save/ })).not.toBeInTheDocument();
  });

  it("settings_save_on_the_agents_page_does_not_wait_for_another_pages_fields", async () => {
    const user = userEvent.setup();
    renderSettingsPage(fixture(), "agents");
    await screen.findByLabelText("Default provider");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByText("Fixture data: nothing was saved.")).toBeInTheDocument();
  });

  it("settings_add_opens_the_standing_approval_form_in_a_dialog_on_the_autonomy_page", async () => {
    const user = userEvent.setup();
    renderSettingsPage(fixture(), "autonomy");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Add" }));
    expect(screen.getByRole("dialog", { name: "Add a standing approval" })).toBeInTheDocument();
  });
});

describe("Find a setting", () => {
  it("search_lists_matching_settings_with_their_page_and_opens_the_page_on_click", async () => {
    const user = userEvent.setup();
    const services = fixture();
    renderSettingsPage(services, "agents");
    await user.type(screen.getByRole("searchbox", { name: "Find a setting" }), "cpu");
    const results = screen.getByRole("list", { name: "Search results" });
    expect(within(results).getByRole("button", { name: /Machine load limits/ })).toHaveTextContent("Limits");
    expect(screen.queryByRole("navigation", { name: "Settings pages" })).not.toBeInTheDocument();
    await user.click(within(results).getByRole("button", { name: /Machine load limits/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "settings", settingsPage: "limits" });
    expect(screen.getByRole("searchbox", { name: "Find a setting" })).toHaveValue("");
    expect(await screen.findByLabelText("CPU limit (percent)")).toBeInTheDocument();
    expect(menu().getAllByRole("button")).toHaveLength(7);
  });

  it("search_matches_page_names_and_opens_the_first_result_on_enter", async () => {
    const user = userEvent.setup();
    const services = fixture();
    renderSettingsPage(services, "agents");
    await user.type(screen.getByRole("searchbox", { name: "Find a setting" }), "memory{Enter}");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "settings", settingsPage: "memory" });
  });

  it("search_says_so_when_nothing_matches", async () => {
    const user = userEvent.setup();
    renderSettingsPage(fixture());
    await user.type(screen.getByRole("searchbox", { name: "Find a setting" }), "zzzz");
    expect(screen.getByText("No setting matches.")).toBeInTheDocument();
  });

  it("search_registry_points_only_at_real_pages_and_matches_every_word", () => {
    const ids = PAGES.map((page) => page.id);
    for (const entry of SETTINGS) expect(ids).toContain(entry.page);
    expect(searchSettings("")).toEqual([]);
    expect(searchSettings("home directory").map((entry) => entry.page)).toEqual(["repositories"]);
    expect(searchSettings("DARK").map((entry) => entry.label)).toEqual(["Theme"]);
    expect(searchSettings("limits").map((entry) => entry.label)).toContain("Limits");
  });

  it("search_escape_clears_the_box", async () => {
    const user = userEvent.setup();
    renderSettingsPage(fixture());
    const box = screen.getByRole("searchbox", { name: "Find a setting" });
    await user.type(box, "cpu{Escape}");
    await waitFor(() => expect(box).toHaveValue(""));
  });
});
