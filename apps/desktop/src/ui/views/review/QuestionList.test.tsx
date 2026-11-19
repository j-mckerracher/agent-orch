import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { QuestionWire } from "../../../daemon/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { holdRange } from "./holdDates";
import { QuestionList } from "./QuestionList";

const question = (id: string, fields: Partial<QuestionWire> = {}): QuestionWire => ({
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

const Q1 = question("q1", { question: "Which database?", options: ["Postgres", "SQLite"] });
const Q2 = question("q2", { deferred_until: "2026-11-21" });

async function connected(setup: (fake: FakeDaemon) => void = () => undefined) {
  const fake = new FakeDaemon();
  fake.stub("decisions.list", { questions: [Q1, Q2] });
  fake.stub("decisions.answer", { question: { ...Q1, state: "answered" } });
  fake.stub("decisions.defer", (p: Record<string, unknown>) => ({ question: { ...Q1, deferred_until: p.until } }));
  setup(fake);
  const services = createServices({ connector: fake });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  render(
    <ServicesProvider services={services}>
      <QuestionList />
    </ServicesProvider>,
  );
  const rows = await screen.findAllByRole("listitem");
  return { fake, services, rows };
}

const firstRow = () => screen.getAllByRole("listitem")[0]!;
const status = () => screen.getByTestId("questions-status");

describe("QuestionList", () => {
  it("lists_only_the_records_from_decisions_list_with_task_options_and_deferral", async () => {
    const { services, rows } = await connected();
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Which database?");
    expect(rows[0]).toHaveTextContent("Task: Task for q1");
    expect(within(rows[0]!).getByRole("button", { name: "Postgres" })).toBeTruthy();
    expect(within(rows[0]!).getByRole("button", { name: "SQLite" })).toBeTruthy();
    expect(rows[0]).not.toHaveTextContent("Deferred until");
    expect(rows[1]).toHaveTextContent("Deferred until 21 November");
    services.connection.stop();
  });

  it("an_option_fills_the_answer_box_and_the_user_may_edit_it", async () => {
    const { services } = await connected();
    const box = within(firstRow()).getByLabelText("Your answer");
    await userEvent.click(within(firstRow()).getByRole("button", { name: "SQLite" }));
    expect(box).toHaveValue("SQLite");
    await userEvent.type(box, " for now");
    expect(box).toHaveValue("SQLite for now");
    services.connection.stop();
  });

  it("sends_the_exact_typed_words_with_an_idempotency_key_and_reads_the_list_again", async () => {
    const { fake, services } = await connected();
    const reads = fake.callsTo("decisions.list").length;
    await userEvent.type(within(firstRow()).getByLabelText("Your answer"), "  Use SQLite{Enter}then move later  ");
    await userEvent.click(within(firstRow()).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(status()).toHaveTextContent("Answered: Task for q1."));
    const params = fake.callsTo("decisions.answer")[0]!.params as Record<string, unknown>;
    expect(params).toMatchObject({ question_id: "q1", answer: "  Use SQLite\nthen move later  " });
    expect(typeof params.idempotency_key).toBe("string");
    expect(params.idempotency_key).not.toBe("");
    await waitFor(() => expect(fake.callsTo("decisions.list").length).toBeGreaterThan(reads));
    services.connection.stop();
  });

  it("does_not_send_an_empty_or_blank_answer", async () => {
    const { fake, services } = await connected();
    const box = within(firstRow()).getByLabelText("Your answer");
    const send = within(firstRow()).getByRole("button", { name: "Send answer" });
    expect(send).toBeDisabled();
    await userEvent.type(box, "   {Enter}  ");
    expect(send).toBeDisabled();
    fireEvent.submit(box.closest("form")!);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(fake.callsTo("decisions.answer")).toEqual([]);
    services.connection.stop();
  });

  it("defer_sends_the_chosen_date", async () => {
    const { fake, services } = await connected();
    const day = holdRange(new Date()).max;
    const input = within(firstRow()).getByLabelText("Defer until");
    expect(input).toHaveAttribute("min", holdRange(new Date()).min);
    fireEvent.change(input, { target: { value: day } });
    await userEvent.click(within(firstRow()).getByRole("button", { name: "Defer" }));
    await waitFor(() => expect(fake.callsTo("decisions.defer")).toHaveLength(1));
    expect(fake.callsTo("decisions.defer")[0]!.params).toMatchObject({ question_id: "q1", until: day });
    expect(typeof (fake.callsTo("decisions.defer")[0]!.params as Record<string, unknown>).idempotency_key).toBe("string");
    await waitFor(() => expect(status()).toHaveTextContent("Deferred: Task for q1 until"));
    services.connection.stop();
  });

  it("shows_a_refusal_inline_and_keeps_the_typed_answer", async () => {
    const { fake, services } = await connected((f) => f.failNext("decisions.answer", { code: -32011, message: "closed" }));
    const box = within(firstRow()).getByLabelText("Your answer");
    await userEvent.type(box, "Postgres");
    await userEvent.click(within(firstRow()).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(status()).toHaveTextContent("That question is already closed. Nothing was changed."));
    expect(box).toHaveValue("Postgres");
    expect(fake.callsTo("decisions.answer")).toHaveLength(1);
    services.connection.stop();
  });

  it("without_the_approver_role_the_controls_are_off_with_the_reason_linked", async () => {
    const { fake, services } = await connected((f) => {
      f.approver = false;
    });
    await userEvent.type(within(firstRow()).getByLabelText("Your answer"), "Postgres");
    const send = within(firstRow()).getByRole("button", { name: "Send answer" });
    expect(send).toBeDisabled();
    expect(send).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(fake.callsTo("decisions.answer")).toEqual([]);
    services.connection.stop();
  });
});
