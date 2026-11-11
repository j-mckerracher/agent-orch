// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { realpathSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";
import { DaemonApi } from "../src/daemon/api";
import { DaemonSession } from "../src/daemon/session";
import { M5_03_CAPABILITIES } from "../src/daemon/types";
import { createServices } from "../src/ui/createServices";
import { ServicesProvider } from "../src/ui/services";
import { WorkstreamsView } from "../src/ui/views/workstreams/WorkstreamsView";
import { NodeConnector, startDaemon } from "./harness";

afterEach(cleanup);

// The Workstreams view over the real daemon: create, add an objective, link a task and set a
// gate through the real workstreams.* methods, then confirm the daemon holds exactly that.
describe("WorkstreamsView over the real daemon", () => {
  it("creates, adds an objective, links a task and verifies a gate, all persisted by the daemon", async () => {
    const daemon = await startDaemon();
    const connector = new NodeConnector(daemon.dataDir);
    const direct = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    const api = new DaemonApi(direct);
    const services = createServices({ connector, connection: { backoffMs: [50], requestTimeoutMs: 8000 } });
    try {
      await api.projectsRegister(daemon.repo);
      const task = await api.tasksSubmit("ws-view-submit", {
        project_id: realpathSync(daemon.repo),
        parent_id: null,
        objective: "Prove the view",
        kind: "Implement",
        agent: "e2e-agent",
        dependencies: [],
        verification: "None",
        delivery: "LocalReview",
        autonomy: "ApprovalRequired",
        max_retries: 1,
        max_repairs: 1,
      });
      services.connection.start();
      await waitFor(() => expect(services.connection.getSnapshot().status).toBe("connected"), { timeout: 15000 });
      const user = userEvent.setup();
      render(
        <ServicesProvider services={services}>
          <WorkstreamsView />
        </ServicesProvider>,
      );

      await screen.findByText(/No workstreams exist yet/, undefined, { timeout: 15000 });
      expect(screen.getByTestId("workstreams-view").textContent).not.toMatch(/PLACEHOLDER/);

      await user.type(screen.getByLabelText("New workstream title"), "Real stream");
      await user.click(screen.getByRole("button", { name: "Create workstream" }));
      expect((await screen.findByTestId("ws-detail", undefined, { timeout: 15000 })).getAttribute("aria-label")).toBe("Workstream Real stream");

      const addForm = screen.getByRole("form", { name: "Add objective" });
      await user.type(within(addForm).getByLabelText("New objective title"), "Ship");
      await user.type(within(addForm).getByLabelText("Acceptance gates, one per line"), "It works");
      await user.click(within(addForm).getByRole("button", { name: "Add objective" }));
      await screen.findByRole("region", { name: "Objective Ship" }, { timeout: 15000 });

      const linkForm = screen.getByRole("form", { name: "Link task" });
      await user.selectOptions(within(linkForm).getByLabelText("Task to link"), task.contract.id);
      await user.click(within(linkForm).getByRole("button", { name: "Link task" }));
      await screen.findByRole("button", { name: /Unlink Prove the view/ }, { timeout: 15000 });

      await user.selectOptions(screen.getByLabelText("Evidence task for gate It works"), task.contract.id);
      await user.click(screen.getByRole("button", { name: "Apply to It works" }));
      await waitFor(() => expect(screen.getByTestId("ws-gate").textContent).toContain("verified"), { timeout: 15000 });

      const [stored] = await api.workstreamsList();
      const detail = await api.workstreamsGet(stored!.id);
      expect(detail.workstream.title).toBe("Real stream");
      expect(detail.tasks.map((link) => link.task_id)).toEqual([task.contract.id]);
      expect(detail.objectives[0]!.gates[0]).toMatchObject({ description: "It works", state: "verified", evidence_task_id: task.contract.id });
    } finally {
      cleanup();
      services.connection.stop();
      direct.close();
      await daemon.stop();
    }
  });
});
