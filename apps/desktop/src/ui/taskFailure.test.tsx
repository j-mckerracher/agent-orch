import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FakeDaemon, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";

describe("task failure details", () => {
  it("shows each failure reason beside its attempt and labels missing reasons unavailable", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [
      mkTask("t-failed", REPO, {
        state: "Failed",
        attempts: [
          {
            id: "attempt-1",
            task_id: "t-failed",
            attempt_number: 1,
            state: "Failed",
            failure_reason: "The repository has no commit at HEAD. Create an initial commit, then retry.",
          },
          { id: "attempt-2", task_id: "t-failed", attempt_number: 2, state: "Failed" },
        ],
      }),
    ];

    await mount(fake, {
      route: { name: "task", projectId: REPO, taskId: "t-failed" },
      daemonSources: true,
    });

    expect(await screen.findByTestId("failure-reason-attempt-1")).toHaveTextContent(
      "Failure reason: The repository has no commit at HEAD. Create an initial commit, then retry.",
    );
    expect(screen.getByTestId("failure-reason-attempt-2")).toHaveTextContent("Failure reason: unavailable.");
    expect(screen.getByText("No recorded session.")).toBeInTheDocument();
  });

  it("explains a missing parent task and preserves the submitted values", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.failNext("tasks.submit", {
      code: -32020,
      message: "parent task was not found; provide an existing task ID or leave the parent blank",
    });
    const { user } = await mount(fake, {
      route: { name: "project", projectId: REPO },
      daemonSources: true,
    });

    await user.type(screen.getByLabelText("Objective"), "A child task");
    await user.type(screen.getByLabelText("Agent"), "Codex");
    await user.type(screen.getByLabelText("Parent task ID (optional)"), "not-a-task");
    await user.click(screen.getByRole("button", { name: "Submit" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The parent task does not exist.");
    expect(alert).toHaveTextContent("Enter an existing task ID or leave the parent blank.");
    await waitFor(() => expect(fake.callsTo("tasks.submit")).toHaveLength(1));
    expect((fake.callsTo("tasks.submit")[0]!.params as { contract: { agent: string; parent_id: string } }).contract)
      .toMatchObject({ agent: "codex", parent_id: "not-a-task" });
  });
});
