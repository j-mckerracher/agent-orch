import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { FakeDaemon, mkApproval, until } from "../test/fakeDaemon";
import { createServices } from "../ui/createServices";
import { parseRoute } from "../ui/router";
import { createReviewClient } from "./client";
import { ReviewRoute } from "./ReviewRoute";

describe("S6 integration with the M5-03 daemon stack", () => {
  it("requests the workspaces and delivery capabilities and routes review methods through DaemonApi", async () => {
    const fake = new FakeDaemon();
    fake.approvals = [mkApproval("a1", "t1")];
    const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 } });
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    try {
      expect(services.connection.getSnapshot().capabilities).toEqual(expect.arrayContaining(["workspaces", "delivery"]));

      const client = createReviewClient(services.connection);
      expect(client.provenance).toBe("daemon");
      expect(await client.call("approvals.list")).toEqual(fake.approvals);
      await expect(client.call("workspaces.diff", { task_id: "t1", approval_id: "a1" })).rejects.toMatchObject({
        kind: "method-unavailable",
      });
      expect(fake.callsTo("workspaces.diff")).toHaveLength(1);
      await expect(client.call("sessions.list")).rejects.toMatchObject({ kind: "method-unavailable" });
    } finally {
      services.connection.stop();
    }
  });

  it("is reachable from the shell route and returns to the task or project route", async () => {
    const services = createServices({
      initialRoute: parseRoute("#/project/p1/review/fixture-task-1?fixture=1"),
      renderReview: (props) => <ReviewRoute {...props} />,
    });
    expect(services.router.getSnapshot()).toEqual({
      name: "review",
      projectId: "p1",
      taskId: "fixture-task-1",
      query: "fixture=1",
    });
    render(<App services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: /Back to task fixture-task-1/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: "p1", taskId: "fixture-task-1" });
  });
});
