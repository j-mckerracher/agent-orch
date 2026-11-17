import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import type { SurfaceSources } from "../../../sources/surfaces/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { GrantsSection } from "./GrantsSection";
import { MERGE_CONFIRM_TEXT, NEVER_GRANTED_TEXT, REVOKE_LOCKED_NOTE } from "./grantsModel";
import { projectCode } from "./projectCode";

const PROJECT = "/work/repo";
const CODE = projectCode(PROJECT);
const NOW = Date.now();

const POLICY = {
  grantable_kinds: ["workstream.update", "objective.add", "objective.revise", "task.propose", "task.retry", "workstream.link_task"],
  default_expires_in_secs: 28800,
  max_expires_in_secs: 604800,
  min_expires_in_secs: 60,
  default_max_uses: 20,
  max_max_uses: 200,
  max_token_budget: 1_000_000_000,
};
const GRANT = {
  id: "grant-9f2c1d7e3a",
  scope: { kind: "project", id: CODE, label: "repo" },
  kinds: ["task.propose", "task.retry"],
  created_at_ms: NOW,
  expires_at_ms: NOW + 4 * 3600_000,
  max_uses: 20,
  uses_used: 3,
  token_budget: 2_000_000,
  tokens_spent: 410_000,
  status: "active",
  revoked_at_ms: null,
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

interface Options {
  approver?: boolean | null;
  surfaces?: SurfaceSources;
  grants?: unknown[];
  configure?: (fake: FakeDaemon) => void;
}

async function section({ approver = true, surfaces, grants = [GRANT], configure }: Options = {}) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  fake.projects = [{ id: PROJECT }];
  fake.stub("grants.list", () => ({ grants, more: false, policy: POLICY }));
  fake.stub("grants.create", () => ({ grant: GRANT }));
  fake.stub("grants.revoke", () => ({ grant: { ...GRANT, status: "revoked", revoked_at_ms: NOW }, stopped: 2 }));
  configure?.(fake);
  const services = createServices({ connector: fake, surfaces, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  render(
    <ServicesProvider services={services}>
      <GrantsSection />
    </ServicesProvider>,
  );
  return { fake, user: userEvent.setup() };
}

const CREATE = "Create standing approval";
const create = () => screen.findByRole("button", { name: CREATE });

async function fill(user: ReturnType<typeof userEvent.setup>, kinds: string[] = ["Propose a task", "Retry a task"]) {
  await create();
  await waitFor(() => expect(screen.getAllByRole("option", { name: "repo" })).toHaveLength(1));
  await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), CODE);
  for (const kind of kinds) await user.click(screen.getByRole("checkbox", { name: kind }));
  await user.type(screen.getByRole("textbox", { name: "Token budget" }), "2000000");
}

describe("standing approvals section", () => {
  it("grants_section_hides_quietly_without_the_capability", async () => {
    await section({ configure: (daemon) => (daemon.unknownCapabilities = ["standing_approvals"]) });
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(screen.queryByRole("region", { name: "Standing approvals" })).not.toBeInTheDocument();
    expect(screen.queryByText(/could not be read/)).not.toBeInTheDocument();
  });

  it("grants_section_offers_only_the_grantable_kinds_and_never_makes_a_control_of_the_others", async () => {
    await section();
    const form = (await create()).closest("form")!;
    const labels = within(form).getAllByRole("checkbox").map((box) => box.getAttribute("value"));
    expect(labels).toEqual(POLICY.grantable_kinds);
    expect(within(form).getByRole("checkbox", { name: "Update a workstream (title and summary only)" })).toBeInTheDocument();
    expect(screen.getByText(NEVER_GRANTED_TEXT)).toBeInTheDocument();
    expect(NEVER_GRANTED_TEXT).toBe("These always need your approval: cancel a task, revise a task, remove a project.");
    for (const word of [/cancel/i, /remove/i, /merge/i, /revise a task/i]) {
      expect(within(form).queryByRole("checkbox", { name: word })).not.toBeInTheDocument();
      expect(within(form).queryByRole("radio", { name: word })).not.toBeInTheDocument();
    }
  });

  it("grants_section_is_locked_for_a_non_approver_with_the_notice_and_revoke_is_off", async () => {
    await section({ approver: false });
    const button = await create();
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(screen.getByRole("checkbox", { name: "Propose a task" })).toBeDisabled();
    const revoke = screen.getByRole("button", { name: /Revoke standing approval/ });
    expect(revoke).toBeDisabled();
    expect(revoke).toHaveAccessibleDescription(REVOKE_LOCKED_NOTE);
    // The list still reads.
    expect(screen.getByText("17 of 20 uses left", { exact: false })).toBeInTheDocument();
  });

  it("grants_section_create_stays_off_with_a_reason_until_the_form_is_valid", async () => {
    const { user } = await section();
    const button = await create();
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/Choose a project or workstream/);
    await user.type(screen.getByRole("textbox", { name: "Token budget" }), "0");
    expect(screen.getByRole("textbox", { name: "Token budget" })).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("textbox", { name: "Expires in (hours)" })).toHaveValue("8");
    expect(screen.getByRole("textbox", { name: "Uses" })).toHaveValue("20");
  });

  it("grants_section_create_sends_exactly_the_chosen_kinds_and_announces_it", async () => {
    const { fake, user } = await section();
    await fill(user);
    await user.click(await create());
    await waitFor(() => expect(fake.callsTo("grants.create")).toHaveLength(1));
    expect(fake.callsTo("grants.create")[0]!.params).toMatchObject({
      scope: { kind: "project", id: CODE },
      kinds: ["task.propose", "task.retry"],
      expires_in_secs: 28800,
      max_uses: 20,
      token_budget: 2_000_000,
    });
    const region = screen.getByRole("status", { name: "Standing approvals result" });
    await waitFor(() => expect(region).toHaveTextContent(/Standing approval created\. It covers Project repo until /));
    expect(region).toHaveAttribute("aria-live", "polite");
  });

  it("grants_section_create_shows_the_daemons_refusal_in_words", async () => {
    const { fake, user } = await section({ configure: (daemon) => daemon.failNext("grants.create", { code: -32025, message: "grant invalid" }) });
    await fill(user);
    await user.click(await create());
    const region = screen.getByRole("status", { name: "Standing approvals result" });
    await waitFor(() => expect(region).toHaveTextContent("The daemon refused these terms. Check the kinds and the limits."));
    expect(fake.callsTo("grants.create")).toHaveLength(1);
  });

  it("grants_section_lists_the_chip_uses_left_time_left_and_tokens_reported_so_far", async () => {
    await section({ grants: [GRANT, { ...GRANT, id: "grant-ended0001", status: "budget_exhausted" }, { ...GRANT, id: "grant-ended0002", status: "exhausted" }] });
    const list = await screen.findByRole("list", { name: "Standing approvals" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent("Active");
    expect(rows[0]).toHaveTextContent("17 of 20 uses left");
    expect(rows[0]).toHaveTextContent("4 hours left");
    expect(rows[0]).toHaveTextContent("410,000 of 2,000,000 tokens reported so far");
    expect(rows[1]).toHaveTextContent("Out of budget");
    expect(rows[2]).toHaveTextContent("Used up");
    // Only a grant that is still active can be revoked.
    expect(within(list).getAllByRole("button", { name: /Revoke standing approval/ })).toHaveLength(1);
  });

  it("grants_section_revoke_announces_in_a_polite_region_and_sends_the_id", async () => {
    const { fake, user } = await section();
    await user.click(await screen.findByRole("button", { name: /Revoke standing approval: Project repo/ }));
    await waitFor(() => expect(fake.callsTo("grants.revoke")).toHaveLength(1));
    expect(fake.callsTo("grants.revoke")[0]!.params).toMatchObject({ grant_id: "grant-9f2c1d7e3a" });
    const region = screen.getByRole("status", { name: "Standing approvals result" });
    await waitFor(() => expect(region).toHaveTextContent("Standing approval revoked. 2 pending actions stopped."));
    expect(region).toHaveAttribute("aria-live", "polite");
  });

  it("grants_section_in_fixture_mode_says_nothing_was_saved", async () => {
    const { fake, user } = await section({ surfaces: createFixtureSurfaces() });
    await user.click(await screen.findByRole("button", { name: /Revoke standing approval/ }));
    const region = screen.getByRole("status", { name: "Standing approvals result" });
    await waitFor(() => expect(region).toHaveTextContent("Fixture data: nothing was saved."));
    expect(fake.callsTo("grants.revoke")).toEqual([]);
    expect(fake.callsTo("grants.create")).toEqual([]);
  });
});

// As the daemon's `grant_policy()` sends it: the merge kind is not in `grantable_kinds`.
const MERGE_POLICY = {
  ...POLICY,
  never_grantable_kinds: ["project.remove", "task.cancel", "task.revise"],
  merge_kind_available: true,
  merge_default_max_uses: 5,
  merge_max_max_uses: 20,
  merge_max_expires_in_secs: 86400,
};
const MERGE_GRANT = { ...GRANT, id: "grant-merge00001", kinds: ["delivery.merge"], branch_prefix: "moa/", confirm_unattended_merge: true };

const withMerge = (daemon: FakeDaemon, grants: unknown[] = [GRANT]) => daemon.stub("grants.list", () => ({ grants, more: false, policy: MERGE_POLICY }));
const mergeForm = () => screen.findByRole("form", { name: "Allow unattended merges" });
const saveMerge = () => screen.findByRole("button", { name: "Save unattended merge approval" });

async function fillMerge(user: ReturnType<typeof userEvent.setup>) {
  await waitFor(() => expect(within(screen.getByRole("combobox", { name: "Project for unattended merges" })).getAllByRole("option")).toHaveLength(2));
  await user.selectOptions(screen.getByRole("combobox", { name: "Project for unattended merges" }), CODE);
  await user.type(screen.getByRole("textbox", { name: "Merge approval token budget" }), "2000000");
}

describe("unattended merge grant form", () => {
  it("mergegrant_ordinary_form_never_offers_the_merge_kind", async () => {
    await section({ configure: (daemon) => withMerge(daemon) });
    const form = (await create()).closest("form")!;
    const values = within(form).getAllByRole("checkbox").map((box) => box.getAttribute("value"));
    expect(values).toEqual(POLICY.grantable_kinds);
    expect(values).not.toContain("delivery.merge");
    expect(NEVER_GRANTED_TEXT).toBe("These always need your approval: cancel a task, revise a task, remove a project.");
  });

  it("mergegrant_the_merge_form_appears_for_the_daemons_real_policy", async () => {
    await section({ configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    expect(screen.getByRole("textbox", { name: "Branch prefix" })).toHaveValue("moa/");
    expect(screen.getByText("A merge uses no tokens, so the budget never runs out; it is required for every grant.")).toBeInTheDocument();
  });

  it("mergegrant_save_is_disabled_while_the_prefix_is_empty", async () => {
    const { user } = await section({ configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    await fillMerge(user);
    await user.click(screen.getByRole("checkbox", { name: MERGE_CONFIRM_TEXT }));
    expect(await saveMerge()).toBeEnabled();
    await user.clear(screen.getByRole("textbox", { name: "Branch prefix" }));
    expect(await saveMerge()).toBeDisabled();
  });

  it("mergegrant_form_is_hidden_without_daemon_support", async () => {
    await section();
    await create();
    expect(screen.queryByRole("form", { name: "Allow unattended merges" })).not.toBeInTheDocument();
  });

  it("mergegrant_form_shows_its_defaults_and_limits", async () => {
    await section({ configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    expect(screen.getByRole("textbox", { name: "Merge approval expires in (hours)" })).toHaveValue("8");
    expect(screen.getByRole("textbox", { name: "Merge approval uses" })).toHaveValue("5");
    expect(screen.getByRole("checkbox", { name: MERGE_CONFIRM_TEXT })).not.toBeChecked();
  });

  it("mergegrant_validation_rejects_values_over_the_limits_and_a_bad_prefix", async () => {
    const { user } = await section({ configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    const hours = screen.getByRole("textbox", { name: "Merge approval expires in (hours)" });
    await user.clear(hours);
    await user.type(hours, "25");
    expect(hours).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Enter a whole number of hours from 1 to 24.")).toBeInTheDocument();
    const uses = screen.getByRole("textbox", { name: "Merge approval uses" });
    await user.clear(uses);
    await user.type(uses, "21");
    expect(screen.getByText("Enter a whole number of uses from 1 to 20.")).toBeInTheDocument();
    const prefix = screen.getByRole("textbox", { name: "Branch prefix" });
    await user.clear(prefix);
    await user.type(prefix, "orchestrator/");
    expect(prefix).toHaveAttribute("aria-invalid", "true");
    expect(await saveMerge()).toBeDisabled();
  });

  it("mergegrant_unconfirmed_blocks_save", async () => {
    const { fake, user } = await section({ configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    await fillMerge(user);
    const save = await saveMerge();
    expect(save).toBeDisabled();
    expect(save).toHaveAccessibleDescription(/tick the confirmation/);
    await user.click(screen.getByRole("checkbox", { name: MERGE_CONFIRM_TEXT }));
    expect(save).toBeEnabled();
    expect(fake.callsTo("grants.create")).toHaveLength(0);
  });

  it("mergegrant_create_sends_exactly_project_kind_confirmation_prefix_and_limits", async () => {
    const { fake, user } = await section({ configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    await fillMerge(user);
    await user.click(screen.getByRole("checkbox", { name: MERGE_CONFIRM_TEXT }));
    await user.click(await saveMerge());
    await waitFor(() => expect(fake.callsTo("grants.create")).toHaveLength(1));
    const params = fake.callsTo("grants.create")[0]!.params as Record<string, unknown>;
    expect(Object.keys(params).sort()).toEqual([
      "branch_prefix",
      "confirm_unattended_merge",
      "expires_in_secs",
      "idempotency_key",
      "kinds",
      "max_uses",
      "scope",
      "token_budget",
    ]);
    expect(params).toMatchObject({
      scope: { kind: "project", id: CODE },
      kinds: ["delivery.merge"],
      confirm_unattended_merge: true,
      branch_prefix: "moa/",
      expires_in_secs: 28800,
      max_uses: 5,
      token_budget: 2_000_000,
    });
  });

  it("mergegrant_form_is_locked_for_a_non_approver", async () => {
    await section({ approver: false, configure: (daemon) => withMerge(daemon) });
    await mergeForm();
    const save = await saveMerge();
    expect(save).toBeDisabled();
    expect(save).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(screen.getByRole("checkbox", { name: MERGE_CONFIRM_TEXT })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "Project for unattended merges" })).toBeDisabled();
  });

  it("mergegrant_list_shows_the_badge_and_the_branch_prefix", async () => {
    await section({ grants: [MERGE_GRANT, GRANT], configure: (daemon) => withMerge(daemon, [MERGE_GRANT, GRANT]) });
    const list = await screen.findByRole("list", { name: "Standing approvals" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("Unattended merge");
    expect(rows[0]).toHaveTextContent("Only branches starting with moa/");
    expect(rows[1]).not.toHaveTextContent("Unattended merge");
    expect(within(rows[0]!).getByRole("button", { name: /Revoke standing approval/ })).toBeEnabled();
  });

  it("mergegrant_refusal_is_shown_in_plain_words", async () => {
    const { fake, user } = await section({
      configure: (daemon) => {
        withMerge(daemon);
        daemon.failNext("grants.create", { code: -32025, message: "grant invalid" });
      },
    });
    await mergeForm();
    await fillMerge(user);
    await user.click(screen.getByRole("checkbox", { name: MERGE_CONFIRM_TEXT }));
    await user.click(await saveMerge());
    const region = screen.getByRole("status", { name: "Standing approvals result" });
    await waitFor(() => expect(region).toHaveTextContent("The daemon refused this unattended merge approval."));
    expect(fake.callsTo("grants.create")).toHaveLength(1);
  });
});
