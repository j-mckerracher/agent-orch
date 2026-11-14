// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { execFileSync } from "node:child_process";
import { afterEach, describe, expect, it } from "vitest";
import { DaemonApi } from "../src/daemon/api";
import { DaemonSession } from "../src/daemon/session";
import { M5_03_CAPABILITIES } from "../src/daemon/types";
import { createServices } from "../src/ui/createServices";
import { ServicesProvider } from "../src/ui/services";
import { ReviewDecisionsView } from "../src/ui/views/review/ReviewDecisionsView";
import { NodeConnector, startDaemon } from "./harness";

afterEach(cleanup);

async function until<T>(read: () => T | undefined | false | Promise<T | undefined | false>, label: string, ms = 15000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await read();
    if (v) return v as T;
    if (Date.now() > deadline) throw new Error(`timeout ${label}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

// The ReviewDecisionsView over the real daemon: the seeded review candidate, the real
// review.request_approval / approvals.resolve / delivery.request / review.request_changes flow.
async function prepare() {
  const daemon = await startDaemon();
  const seeded = await daemon.seedReview();
  const connector = new NodeConnector(daemon.dataDir);
  const direct = async <T,>(fn: (api: DaemonApi) => Promise<T>): Promise<T> => {
    const s = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    try {
      return await fn(new DaemonApi(s));
    } finally {
      s.close();
    }
  };
  const targetRef = `refs/heads/orchestrator/review/${seeded.task_id}`;
  const requested = await direct((api) =>
    api.reviewRequestApproval(
      {
        task_id: seeded.task_id,
        workspace_id: seeded.workspace_id,
        verification_run_id: seeded.verification_run_id,
        action: "local_review",
        target_ref: targetRef,
      },
      "e2e-key-request",
    ),
  );
  const services = createServices({ connector, connection: { backoffMs: [50], requestTimeoutMs: 8000 } });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  const tree = () => (
    <ServicesProvider services={services}>
      <ReviewDecisionsView />
    </ServicesProvider>
  );
  const view = render(tree());
  const stop = async () => {
    cleanup();
    services.connection.stop();
    await daemon.stop();
  };
  return { daemon, seeded, direct, services, targetRef, approval: requested.approval, view, tree, stop };
}

const accept = () => screen.getByRole("button", { name: "Accept" }) as HTMLButtonElement;
const waitEnabled = (read: () => HTMLButtonElement) => waitFor(() => expect(read().disabled).toBe(false), { timeout: 15000 });

describe("ReviewDecisionsView over the real daemon", () => {
  it("lists the real approval, Accept resolves it, the approved candidate survives a remount, delivery creates the ref", async () => {
    const t = await prepare();
    try {
      const queue = await screen.findByRole("region", { name: "Decision queue" });
      const row = await within(queue).findByRole("button", { name: new RegExp(`Task ${t.seeded.task_id}`) });
      expect(row.textContent).toContain(t.approval.id);
      await waitEnabled(accept);
      await userEvent.click(accept());
      await screen.findByText(/receipt from daemon/, undefined, { timeout: 15000 });
      const pending = await t.direct((a) => a.approvalsList());
      expect(pending.find((a) => a.id === t.approval.id)).toBeUndefined();

      // Leave and come back: the daemon lists pending approvals only, so the view must keep the approved candidate.
      t.view.unmount();
      render(t.tree());
      const delivery = await screen.findByRole("tab", { name: /Delivery/ }, { timeout: 15000 });
      await userEvent.click(delivery);
      const panel = await screen.findByRole("tabpanel");
      await userEvent.click(await within(panel).findByRole("button", { name: new RegExp(`Task ${t.seeded.task_id}`) }));

      const deliver = await screen.findByRole("button", { name: "Request delivery" });
      await waitEnabled(() => deliver as HTMLButtonElement);
      await userEvent.click(deliver);
      await screen.findAllByText(/review_ref_created/, undefined, { timeout: 15000 });
      expect(execFileSync("git", ["rev-parse", t.targetRef], { cwd: t.daemon.repo, encoding: "utf8" }).trim()).toBe(
        t.seeded.candidate_commit,
      );
      const integrated = document.querySelector('[data-fact="integrated"]');
      expect(integrated?.textContent).toContain("Request recorded");
    } finally {
      await t.stop();
    }
  }, 90000);

  it("Request changes goes through review.request_changes and never approvals.resolve", async () => {
    const t = await prepare();
    try {
      await screen.findByRole("region", { name: "Decision queue" });
      await waitEnabled(accept);
      const panel = screen.getByRole("region", { name: "Request changes" });
      await userEvent.click(within(panel).getByRole("button", { name: "Request changes" }));
      await userEvent.type(within(panel).getByLabelText(/Feedback/), "please add tests");
      await userEvent.click(within(panel).getByRole("button", { name: "Send feedback" }));
      await waitFor(
        async () => {
          const evidence = await t.direct((a) => a.reviewEvidence(t.seeded.task_id, t.approval.id));
          expect(evidence.review_state).not.toBe("awaiting_decision");
        },
        { timeout: 15000 },
      );
    } finally {
      await t.stop();
    }
  }, 90000);
});
