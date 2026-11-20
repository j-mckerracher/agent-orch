import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { QuestionWire } from "../../../daemon/types";
import { approval, makeClient, taskRecord } from "../../../review/testing";
import { populate } from "../../../sources/surfaces/testkit";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { holdRange } from "./holdDates";
import { ReviewDecisionsView } from "./ReviewDecisionsView";

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

const APPROVALS = [approval({ id: "approval-1", taskId: "task-1" }), approval({ id: "approval-2", taskId: "task-2", commit: "c".repeat(40) })];

function mount(services: Services) {
  const client = makeClient({
    "approvals.list": () => APPROVALS,
    "tasks.list": () => [taskRecord("task-1", "project-1"), taskRecord("task-2", "project-2")],
  });
  render(
    <ServicesProvider services={services}>
      <ReviewDecisionsView client={client} />
    </ServicesProvider>,
  );
}

async function connected(setup: (fake: FakeDaemon) => void = () => undefined, base: (fake: FakeDaemon) => FakeDaemon = (fake) => fake) {
  const fake = base(new FakeDaemon());
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

const status = () => screen.getByTestId("queue-status");
const rowOf = (type: string, text: string | RegExp) =>
  screen.getAllByRole("listitem").find((item) => item.getAttribute("data-type") === type && (typeof text === "string" ? item.textContent?.includes(text) : text.test(item.textContent ?? "")))!;

describe("Review queue in fixture mode", () => {
  it("is_one_list_with_a_chip_per_item_and_no_old_tabs", async () => {
    mount(createServices());
    expect(await screen.findByRole("heading", { level: 1, name: "Review" })).toBeTruthy();
    await screen.findByText(/Pull request 42/);
    const rows = screen.getAllByRole("listitem").filter((item) => item.hasAttribute("data-row-key"));
    const chips = new Set(rows.map((item) => item.getAttribute("data-type")));
    expect(chips).toEqual(new Set(["approval", "question", "candidate", "pr"]));
    for (const [type, chip] of [["approval", "Approval"], ["question", "Question"], ["candidate", "Candidate"], ["pr", "PR"]]) {
      expect(within(rowOf(type!, /./)).getByText(chip!)).toBeTruthy();
    }
    for (const old of ["Approvals", "Verification", "Candidates", "Delivery"]) expect(screen.queryByRole("tab", { name: old })).toBeNull();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent?.replace(/ \d+$/, ""))).toEqual(["Needs you", "Held", "Resolved"]);
  });

  it("filters_by_type_and_offers_all_projects", async () => {
    mount(createServices());
    await screen.findByText(/Pull request 42/);
    expect(within(screen.getByRole("combobox", { name: "Project" })).getByRole("option", { name: "All projects" })).toBeTruthy();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "Question");
    const rows = screen.getAllByRole("listitem").filter((item) => item.hasAttribute("data-row-key"));
    expect(rows.length).toBeGreaterThan(0);
    for (const item of rows) expect(item.getAttribute("data-type")).toBe("question");
  });

  it("decides_a_proposed_action_in_its_row_and_says_fixture_data_changed_nothing", async () => {
    mount(createServices());
    const row = await screen.findByText("Create workstream: Post-MVP work").then((el) => el.closest("li")!);
    await userEvent.click(within(row).getByRole("button", { name: /^Approve:/ }));
    await waitFor(() => expect(status()).toHaveTextContent("Fixture data: nothing was created."));
  });

  it("answers_a_question_in_its_row", async () => {
    mount(createServices());
    const row = await screen.findByText(/Should existing sessions be migrated/).then((el) => el.closest("li")!);
    await userEvent.click(within(row).getByRole("button", { name: "Let them expire" }));
    expect(within(row).getByRole("textbox")).toHaveValue("Let them expire");
    await userEvent.click(within(row).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(status()).toHaveTextContent(/Answered/));
  });

  it("opens_a_candidate_on_its_own_page_from_the_row", async () => {
    const services = createServices();
    mount(services);
    await userEvent.click(await screen.findByRole("button", { name: /^Review: Review changes: .*task-1/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review", taskId: "task-1" });
  });
});

describe("Review queue against a daemon", () => {
  const Q = (id: string, fields: Partial<QuestionWire> = {}): QuestionWire => ({
    id,
    task_id: `task-${id}`,
    task_title: `Task for ${id}`,
    project: "p-12345678",
    question: `Question ${id}?`,
    options: [],
    key: null,
    raised_by: "worker",
    state: "open",
    deferred_until: null,
    created_at: "2026-11-18T09:00:00Z",
    closing: null,
    ...fields,
  });
  const Q1 = Q("q1", { question: "Which database?", options: ["Postgres", "SQLite"] });
  const questions = (fake: FakeDaemon) => {
    fake.stub("decisions.list", { questions: [Q1, Q("q2", { deferred_until: "2026-11-21" })] });
    fake.stub("decisions.answer", { question: { ...Q1, state: "answered" } });
    fake.stub("decisions.defer", (p: Record<string, unknown>) => ({ question: { ...Q1, deferred_until: p.until } }));
  };

  it("counts_needs_you_like_the_shell_and_leaves_it_unnumbered_until_known", async () => {
    const fake = populate(new FakeDaemon());
    const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
    stops.push(() => {
      services.connection.stop();
      services.data.dispose();
    });
    mount(services);
    expect(screen.getByRole("tab", { name: "Needs you" })).toBeTruthy();
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    expect(await screen.findByRole("tab", { name: "Needs you 1" })).toBeTruthy();
    expect(screen.getByText("1 waiting")).toBeTruthy();
  });

  it("sends_the_exact_typed_answer_with_a_key_and_a_deferred_question_is_in_held", async () => {
    const { fake } = await connected(questions);
    const row = (await screen.findByText("Which database?")).closest("li")!;
    expect(row).toHaveTextContent("Task: Task for q1");
    await userEvent.type(within(row).getByRole("textbox"), "  Use SQLite{Enter}then move later  ");
    await userEvent.click(within(row).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(status()).toHaveTextContent("Answered: Task for q1."));
    const params = fake.callsTo("decisions.answer")[0]!.params as Record<string, unknown>;
    expect(params).toMatchObject({ question_id: "q1", answer: "  Use SQLite\nthen move later  " });
    expect(params.idempotency_key).toBeTruthy();
    await userEvent.click(screen.getByRole("tab", { name: /^Held/ }));
    expect(await screen.findByText("Question q2?")).toBeTruthy();
    expect(screen.getByText("Held until 21 November")).toBeTruthy();
  });

  it("does_not_send_a_blank_answer", async () => {
    const { fake } = await connected(questions);
    const row = (await screen.findByText("Which database?")).closest("li")!;
    await userEvent.type(within(row).getByRole("textbox"), "   ");
    expect(within(row).getByRole("button", { name: "Send answer" })).toBeDisabled();
    expect(fake.callsTo("decisions.answer")).toEqual([]);
  });

  it("defers_a_question_to_the_chosen_day", async () => {
    const { fake } = await connected(questions);
    const row = (await screen.findByText("Which database?")).closest("li")!;
    await userEvent.click(within(row).getAllByText("Defer")[0]!);
    const day = holdRange(new Date()).max;
    fireEvent.change(within(row).getByLabelText("Defer until"), { target: { value: day } });
    await userEvent.click(within(row).getByRole("button", { name: "Defer" }));
    await waitFor(() => expect(fake.callsTo("decisions.defer")).toHaveLength(1));
    expect(fake.callsTo("decisions.defer")[0]!.params).toMatchObject({ question_id: "q1", until: day });
  });

  it("keeps_the_typed_answer_when_the_daemon_refuses", async () => {
    await connected((f) => {
      questions(f);
      f.failNext("decisions.answer", { code: -32011, message: "closed" });
    });
    const row = (await screen.findByText("Which database?")).closest("li")!;
    await userEvent.type(within(row).getByRole("textbox"), "Postgres");
    await userEvent.click(within(row).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(status()).toHaveTextContent("That question is already closed. Nothing was changed."));
    expect(within(row).getByRole("textbox")).toHaveValue("Postgres");
  });

  it("turns_the_answer_off_with_the_reason_linked_without_the_approver_role", async () => {
    await connected((f) => {
      questions(f);
      f.approver = false;
    });
    const row = (await screen.findByText("Which database?")).closest("li")!;
    await userEvent.type(within(row).getByRole("textbox"), "Postgres");
    const send = within(row).getByRole("button", { name: "Send answer" });
    expect(send).toBeDisabled();
    expect(send).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
  });

  const holdWire = { id: "hold-1", subject_kind: "approval", subject_id: "task-1", until_date: "2026-11-21", state: "active" };
  const holds = (fake: FakeDaemon) => fake.stub("holds.list", { holds: [holdWire] });

  it("lists_a_held_candidate_under_held_and_releases_it_with_the_daemons_answer", async () => {
    const { fake } = await connected((f) => {
      holds(f);
      f.stub("holds.release", { hold: { ...holdWire, state: "released" } });
    });
    await userEvent.click(await screen.findByRole("tab", { name: /^Held/ }));
    expect(await screen.findByText("Held until 21 November")).toBeTruthy();
    await userEvent.click(await screen.findByRole("button", { name: "Release: task task-1" }));
    await waitFor(() => expect(status()).toHaveTextContent(/Released: task task-1/));
    expect(fake.callsTo("holds.release")[0]!.params).toMatchObject({ hold_id: "hold-1" });
  });

  it("turns_release_off_with_the_reason_linked_without_the_approver_role", async () => {
    const { fake } = await connected((f) => {
      holds(f);
      f.approver = false;
    });
    await userEvent.click(await screen.findByRole("tab", { name: /^Held/ }));
    const release = await screen.findByRole("button", { name: "Release: task task-1" });
    expect(release).toBeDisabled();
    expect(release).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(fake.callsTo("holds.release")).toEqual([]);
  });

  it("says_nothing_was_changed_and_keeps_the_row_when_the_release_is_refused", async () => {
    const { fake } = await connected(holds);
    fake.failNext("holds.release", { code: -32010, message: "not found" });
    await userEvent.click(await screen.findByRole("tab", { name: /^Held/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Release: task task-1" }));
    await waitFor(() => expect(status()).toHaveTextContent("That hold has already ended. Nothing was changed."));
    expect(screen.getByRole("button", { name: "Release: task task-1" })).toBeTruthy();
  });

  it("has_no_held_tab_and_never_reads_holds_when_the_daemon_has_no_holds_capability", async () => {
    const { fake } = await connected((f) => {
      holds(f);
      f.unknownCapabilities = ["holds"];
    });
    await screen.findByRole("tab", { name: /^Needs you/ });
    expect(screen.queryByRole("tab", { name: /^Held/ })).toBeNull();
    expect(fake.callsTo("holds.list")).toEqual([]);
  });

  const wire = (over: Record<string, unknown> = {}) => ({
    task_id: "t-1",
    number: 42,
    state: "open",
    head_sha: "abc123def4567890abc123def4567890abc12345",
    base_branch: "main",
    branch: "orchestrator/t-1",
    mergeable: "yes",
    required_total: 3,
    required_passed: 3,
    required_failed: 0,
    checks: [{ name: "build", result: "pass" }],
    title: "Retry the sync call",
    observed_age_secs: 40,
    merge: null,
    ...over,
  });
  const pullRow = (number: number) => screen.getByText(new RegExp(`^Pull request ${number}:`)).closest("li")!;

  it("lists_an_open_pull_request_under_needs_you_and_a_merged_one_under_resolved_without_refresh", async () => {
    await connected((f) => {
      f.stub("pulls.list", { pulls: [wire({ merge: { state: "issued", refusal: null } }), wire({ task_id: "t-2", number: 7, state: "merged", head_sha: "b".repeat(40) })] });
    });
    const open = await waitFor(() => pullRow(42));
    expect(within(open).getByText("PR")).toBeTruthy();
    expect(open).toHaveTextContent("Required checks 3 of 3 passed");
    expect(open).toHaveTextContent("Checking whether it merged");
    expect(screen.getByText(/^Pull request 7:/).closest("[role=tabpanel]")).toHaveAttribute("hidden");
    await userEvent.click(screen.getByRole("tab", { name: "Resolved" }));
    expect(screen.getByText(/^Pull request 7:/).closest("[role=tabpanel]")).not.toHaveAttribute("hidden");
    const merged = await waitFor(() => pullRow(7));
    expect(within(merged).getByText("Merged")).toBeTruthy();
    expect(within(merged).queryByRole("button", { name: /Refresh/ })).toBeNull();
  });

  it("renders_a_hostile_pull_request_title_and_check_name_as_plain_text", async () => {
    await connected((f) => {
      f.stub("pulls.list", {
        pulls: [wire({ title: '<a href="https://evil.example">click</a><img src=x onerror=alert(1)>', checks: [{ name: "<script>boom()</script>", result: "pass" }] })],
      });
    });
    const row = await waitFor(() => pullRow(42));
    expect(row).toHaveTextContent('<a href="https://evil.example">click</a>');
    expect(row).toHaveTextContent("<script>boom()</script>: Passed");
    expect(row.querySelector("a, img, script")).toBeNull();
  });

  it("refreshes_a_pull_request_once_and_waits_before_the_next_read", async () => {
    const { fake } = await connected((f) => {
      f.stub("pulls.list", { pulls: [wire({ required_passed: 1 })] });
      f.stub("pulls.refresh", wire({ required_passed: 3, observed_age_secs: 0 }));
    });
    await waitFor(() => pullRow(42));
    await userEvent.click(screen.getByRole("button", { name: "Refresh pull request 42" }));
    await waitFor(() => expect(status()).toHaveTextContent("Read pull request 42 again: Required checks 3 of 3 passed."));
    expect(fake.callsTo("pulls.refresh")).toHaveLength(1);
    const again = screen.getByRole("button", { name: "Refresh pull request 42" });
    expect(again).toBeDisabled();
    expect(again).toHaveAccessibleDescription(/available again in about 10 seconds/);
  });

  it("has_no_pull_requests_and_never_asks_when_the_daemon_has_no_landing_capability", async () => {
    const { fake } = await connected((f) => {
      f.unknownCapabilities = ["landing"];
    });
    await screen.findByRole("tab", { name: /^Needs you/ });
    expect(screen.queryByText(/^Pull request/)).toBeNull();
    expect(fake.callsTo("pulls.list")).toEqual([]);
  });
});
