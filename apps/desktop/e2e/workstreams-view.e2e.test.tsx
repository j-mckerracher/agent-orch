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
import { chooseAction, openTab } from "../src/test/workstreamsUi";
import { WorkstreamsView } from "../src/ui/views/workstreams/WorkstreamsView";
import { NodeConnector, noAgentsEnv, startDaemon } from "./harness";

afterEach(cleanup);

// The Workstreams view over the real daemon: add an objective, link a task, set a gate and
// prioritize through the real workstreams.* methods, then confirm the daemon holds exactly that.
// Workstreams are created by the orchestrator, so the test creates them directly.
describe("WorkstreamsView over the real daemon", () => {
  it("adds an objective, links a task, verifies a gate and prioritizes, all persisted by the daemon", async () => {
    const daemon = await startDaemon(noAgentsEnv());
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
        agent: "codex",
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

      const other = await api.workstreamsCreate("Other stream", undefined, "ws-view-create-0");
      const created = await api.workstreamsCreate("Real stream", undefined, "ws-view-create-1");
      await chooseAction(user, "More actions", "Refresh");
      const rows = await screen.findAllByTestId("ws-row", undefined, { timeout: 15000 });
      expect(rows.map((row) => row.querySelector(".ws-row-name")?.textContent)).toEqual(["Other stream", "Real stream"]);
      await user.click(rows[1]!);
      await screen.findByRole("heading", { level: 1, name: "Real stream" }, { timeout: 15000 });
      expect(screen.getByTestId("ws-detail").getAttribute("aria-label")).toBe("Workstream Real stream");

      await chooseAction(user, "Workstream actions", "Prioritize");
      await waitFor(async () => expect((await api.workstreamsList()).map((item) => item.id)).toEqual([created.id, other.id]), {
        timeout: 15000,
      });

      await user.click(screen.getByRole("button", { name: "+ Add objective" }));
      const addForm = await screen.findByRole("form", { name: "Add objective" });
      await user.type(within(addForm).getByLabelText("New objective title"), "Ship");
      await user.type(within(addForm).getByLabelText("Acceptance gates, one per line"), "It works");
      await user.click(within(addForm).getByRole("button", { name: "Add objective" }));
      await openTab(user, "Objectives");
      await screen.findByRole("region", { name: "Objective Ship" }, { timeout: 15000 });

      await openTab(user, "Tasks");
      const linkForm = screen.getByRole("form", { name: "Link task" });
      await user.selectOptions(within(linkForm).getByLabelText("Task to link"), task.contract.id);
      await user.click(within(linkForm).getByRole("button", { name: "Link task" }));
      await screen.findByRole("button", { name: /Unlink Prove the view/ }, { timeout: 15000 });

      await openTab(user, "Objectives");
      await user.selectOptions(screen.getByLabelText("Evidence task for gate It works"), task.contract.id);
      await user.click(screen.getByRole("button", { name: "Apply to It works" }));
      await waitFor(
        () => expect(within(screen.getByTestId("ws-gate")).getByText("verified", { exact: true })).toBeTruthy(),
        { timeout: 15000 },
      );

      const detail = await api.workstreamsGet(created.id);
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
