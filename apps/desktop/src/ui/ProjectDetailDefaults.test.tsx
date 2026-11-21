import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE, DaemonError } from "../daemon/errors";
import { createFixtureSurfaces } from "../sources/surfaces/fixtures";
import { notOk, observation, okResult } from "../sources/surfaces/result";
import type { ProjectDefaults, ProjectDefaultsSource, SurfaceSources } from "../sources/surfaces/types";
import { FakeDaemon, until } from "../test/fakeDaemon";
import { createServices } from "./createServices";
import { ProjectDetail } from "./ProjectDetail";
import { ServicesProvider, type Services } from "./services";

const PROJECT = "/work/repo";
const DAEMON = { kind: "daemon", id: "D2" } as const;
const SAVED: ProjectDefaults = { mode: "local-only", baseBranch: null, branchPrefix: null, checks: [], mergeMethod: "squash" };
const SECTION = "Defaults for this project";
const SAVE = "Save project defaults";

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

/** A daemon-provenance source whose read answers `read` and whose save is `set`. */
function source(read: () => ReturnType<ProjectDefaultsSource["get"]>, set: ProjectDefaultsSource["set"]): ProjectDefaultsSource {
  return { provenance: DAEMON, get: read, set };
}
const answering = (defaults: ProjectDefaults) => () => Promise.resolve(okResult(DAEMON, defaults, observation(0)));

async function page(projectDefaults: ProjectDefaultsSource, approver: boolean | null = true) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  fake.projects = [{ id: PROJECT }];
  const surfaces: SurfaceSources = { ...createFixtureSurfaces(), projectDefaults };
  const services: Services = createServices({
    connector: fake,
    surfaces,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
  });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  render(
    <ServicesProvider services={services}>
      <ProjectDetail projectId={PROJECT} tab="defaults" />
    </ServicesProvider>,
  );
  return userEvent.setup();
}

const saveButton = () => screen.getByRole("button", { name: SAVE });

describe("project defaults section", () => {
  it("project_detail_defaults_shows_the_modes_with_consequences_and_the_hints", async () => {
    await page(source(answering(SAVED), vi.fn()));
    const region = await screen.findByRole("region", { name: SECTION });
    expect(screen.getByRole("tab", { name: "Defaults" })).toHaveAttribute("aria-selected", "true");
    expect(within(region).queryByRole("heading", { level: 2 })).not.toBeInTheDocument();
    const radios = within(region).getAllByRole("radio");
    expect(radios.map((radio) => radio.getAttribute("value"))).toEqual(["local-only", "direct-pr", "no-mistakes"]);
    expect(within(region).getByRole("radio", { name: "Review only (local-only)" })).toBeChecked();
    expect(within(region).getByRole("radio", { name: "Review only (local-only)" })).toHaveAccessibleDescription(/Nothing leaves it/);
    expect(within(region).getByRole("textbox", { name: "Base branch" })).toHaveAccessibleDescription(/Leave empty to branch from the current checkout/);
    expect(within(region).getByRole("group", { name: "Checks" })).toHaveAccessibleDescription(/Each command runs without a shell/);
    expect(within(region).getByRole("combobox", { name: "Merge method" })).toHaveValue("squash");
  });

  it("project_detail_defaults_hides_the_section_quietly_for_a_daemon_without_the_method", async () => {
    await page(source(() => Promise.resolve(notOk(DAEMON, "unsupported", "no such method")), vi.fn()));
    await screen.findByRole("tab", { name: "Tasks" });
    expect(screen.queryByRole("tab", { name: "Defaults" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: SECTION })).not.toBeInTheDocument();
    expect(screen.queryByText(/could not be read/)).not.toBeInTheDocument();
  });

  it("project_detail_defaults_marks_a_bad_prefix_with_linked_text_and_explains_why_save_is_blocked", async () => {
    const user = await page(source(answering(SAVED), vi.fn()));
    const prefix = await screen.findByRole("textbox", { name: "Branch prefix" });
    await user.type(prefix, "orchestrator/");
    expect(prefix).toHaveAttribute("aria-invalid", "true");
    expect(prefix).toHaveAccessibleDescription(/Do not start with orchestrator\//);
    expect(saveButton()).toBeEnabled();
    expect(saveButton()).toHaveAccessibleDescription("Fix the fields marked as invalid before saving.");
  });

  it("project_detail_defaults_refuses_full_verification_without_a_check_before_sending", async () => {
    const set = vi.fn();
    const user = await page(source(answering(SAVED), set));
    await user.click(await screen.findByRole("radio", { name: "Pull request, full verification (no-mistakes)" }));
    expect(saveButton()).toBeEnabled();
    expect(screen.getByRole("group", { name: "Checks" })).toHaveAccessibleDescription(/Full verification needs at least one check command/);
    await user.click(screen.getByRole("button", { name: "Add check command" }));
    await user.type(screen.getByRole("textbox", { name: "Check command 1" }), "cargo test");
    expect(saveButton()).toBeEnabled();
    expect(set).not.toHaveBeenCalled();
  });

  it("project_detail_defaults_save_sends_split_arguments_and_announces_the_result_in_a_polite_region", async () => {
    const set = vi.fn((_id: string, defaults: ProjectDefaults) => Promise.resolve({ simulated: false, provenance: DAEMON, defaults }));
    const user = await page(source(answering(SAVED), set));
    await user.click(await screen.findByRole("radio", { name: "Pull request, run checks (direct-pr)" }));
    await user.type(screen.getByRole("textbox", { name: "Base branch" }), "main");
    await user.click(screen.getByRole("button", { name: "Add check command" }));
    await user.type(screen.getByRole("textbox", { name: "Check command 1" }), "cargo test --all");
    await user.click(saveButton());
    expect(set).toHaveBeenCalledWith(PROJECT, {
      mode: "direct-pr",
      baseBranch: "main",
      branchPrefix: null,
      checks: [["cargo", "test", "--all"]],
      mergeMethod: "squash",
    });
    const live = screen.getByRole("status", { name: "Project defaults save result" });
    expect(live).toHaveAttribute("aria-live", "polite");
    await waitFor(() => expect(live).toHaveTextContent("Saved."));
    // The section keeps focus, so the result is read where the user was.
    expect(screen.getByRole("region", { name: SECTION })).toHaveFocus();
    expect(saveButton()).toBeEnabled();
  });

  it("project_detail_defaults_shows_the_daemons_refusal_in_plain_words_and_keeps_what_was_typed", async () => {
    const set = vi.fn(() => Promise.reject(new DaemonError("invalid-params", "base branch not found")));
    const user = await page(source(answering(SAVED), set));
    await user.type(await screen.findByRole("textbox", { name: "Base branch" }), "nope");
    await user.click(saveButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(/The daemon refused these values/);
    expect(screen.getByRole("textbox", { name: "Base branch" })).toHaveValue("nope");
  });

  it("project_detail_defaults_save_shows_pending_on_the_button_until_the_daemon_answers", async () => {
    type SetResult = Awaited<ReturnType<ProjectDefaultsSource["set"]>>;
    let finish: (() => void) | undefined;
    const set = vi.fn(
      (_id: string, defaults: ProjectDefaults) =>
        new Promise<SetResult>((resolve) => {
          finish = () => resolve({ simulated: false, provenance: DAEMON, defaults });
        }),
    );
    const user = await page(source(answering(SAVED), set));
    await user.type(await screen.findByRole("textbox", { name: "Base branch" }), "main");
    await user.click(saveButton());
    expect(saveButton()).toHaveAttribute("aria-busy", "true");
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    await act(async () => finish?.());
    await waitFor(() => expect(screen.getByRole("status", { name: "Project defaults save result" })).toHaveTextContent("Saved."));
    expect(saveButton()).not.toHaveAttribute("aria-busy");
  });

  it("project_detail_defaults_locks_every_control_with_the_approver_notice_for_a_non_approver", async () => {
    await page(source(answering(SAVED), vi.fn()), false);
    const region = await screen.findByRole("region", { name: SECTION });
    expect(saveButton()).toBeEnabled();
    expect(saveButton()).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    for (const control of [
      ...within(region).getAllByRole("radio"),
      within(region).getByRole("textbox", { name: "Base branch" }),
      within(region).getByRole("textbox", { name: "Branch prefix" }),
      within(region).getByRole("combobox", { name: "Merge method" }),
      within(region).getByRole("button", { name: "Add check command" }),
    ]) {
      expect(control).toBeDisabled();
      expect(control).toHaveAccessibleDescription(expect.stringContaining(APPROVER_REQUIRED_NOTICE));
    }
  });

  it("project_detail_defaults_fixture_save_says_nothing_was_changed", async () => {
    const user = userEvent.setup();
    const services = createServices({ surfaces: createFixtureSurfaces() });
    render(
      <ServicesProvider services={services}>
        <ProjectDetail projectId="fixture:project-beta" tab="defaults" />
      </ServicesProvider>,
    );
    await user.selectOptions(await screen.findByRole("combobox", { name: "Merge method" }), "rebase");
    await user.click(saveButton());
    expect(await screen.findByText("Fixture data: nothing was changed.")).toBeInTheDocument();
  });

  it("project_detail_defaults_has_no_remove_project_control", async () => {
    await page(source(answering(SAVED), vi.fn()));
    await screen.findByRole("region", { name: SECTION });
    expect(screen.queryByRole("button", { name: /remove project/i })).not.toBeInTheDocument();
  });
});
