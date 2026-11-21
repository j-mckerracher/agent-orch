import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FakeDaemon, mkSession, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";

/** Answering the agent's open question from the task window (DPES-1106). A refused answer is an alert with Retry (DPES-1170). */

const taskRoute = (taskId: string) => ({ name: "task", projectId: REPO, taskId }) as const;

const question = {
  id: "q1",
  task_id: "t-w",
  task_title: "Fix the login bug",
  project: "",
  question: "Migrate sessions now?",
  options: [],
  key: null,
  raised_by: "worker",
  state: "open",
  deferred_until: null,
  created_at: "2026-11-18T09:00:00Z",
};

describe("task answer", () => {
  it("a refused answer is an alert with Retry, not a status line, and Retry sends it again", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [mkTask("t-w", REPO, { state: "Waiting", sessions: [mkSession("s1", { state: "Running" })] })];
    fake.stub("decisions.list", { questions: [question] });
    fake.stub("decisions.answer", { question: { ...question, state: "answered" } });
    fake.failNext("decisions.answer", { code: -32010, message: "question not found" });
    const { user } = await mount(fake, { route: taskRoute("t-w") });
    const log = within(await screen.findByRole("log", { name: /^Conversation/ }));
    await log.findByText("Migrate sessions now?");

    await user.type(screen.getByRole("textbox", { name: "Your answer" }), "Migrate them now");
    await user.click(screen.getByRole("button", { name: "Send answer" }));

    const refusal = await screen.findByText(/That question is no longer known\. Nothing was changed\./);
    const alert = refusal.closest<HTMLElement>('[role="alert"]');
    expect(alert).not.toBeNull();
    expect(screen.queryAllByRole("status").some((region) => /no longer known/.test(region.textContent ?? ""))).toBe(false);

    await user.click(within(alert!).getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Answered: Fix the login bug.")).toBeInTheDocument();
    expect(screen.queryAllByText(/no longer known/)).toHaveLength(0);
    expect(fake.callsTo("decisions.answer")).toHaveLength(2);
  });
});
