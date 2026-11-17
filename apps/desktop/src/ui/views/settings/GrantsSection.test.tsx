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
import { NEVER_GRANTED_TEXT, REVOKE_LOCKED_NOTE } from "./grantsModel";
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
    expect(NEVER_GRANTED_TEXT).toBe("These always need your approval: cancel a task, revise a task, remove a project, merge.");
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
