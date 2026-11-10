import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Approval } from "../../../daemon/types";
import { FakeDaemon, mkApproval, mkTask, until } from "../../../test/fakeDaemon";
import type { ChangeRequestSource } from "../../../review/sources";
import { createFixtureChangeRequestSource } from "../../../review/sources";
import { DIFF, approval, evidenceFor, evidenceSource, fail, makeClient, taskRecord, type TestClient } from "../../../review/testing";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { ReviewDecisionsView } from "./ReviewDecisionsView";
import { reviewGlobalViews } from "./register";

const policy = (id: string, taskId: string): Approval => ({
  ...approval({ id, taskId }),
  action: { ...approval({ id, taskId }).action, kind: { type: "policy_change" } },
});

function liveSetup(opts: { approvals?: Approval[]; fixtureEvidence?: boolean; changeRequestSource?: ChangeRequestSource } = {}) {
  const approvals = opts.approvals ?? [
    approval({ id: "approval-1", taskId: "task-1" }),
    approval({ id: "approval-2", taskId: "task-2", commit: "c".repeat(40) }),
  ];
  const state = { list: [...approvals] };
  const client: TestClient = makeClient({
    "approvals.list": () => state.list.filter((a) => a.state.state === "requested"),
    "tasks.list": () => [taskRecord("task-1", "project-1"), taskRecord("task-2", "project-2")],
    "tasks.get": (p) => taskRecord(String(p.task_id), p.task_id === "task-2" ? "project-2" : "project-1"),
    "workspaces.diff": (p) => ({ task_id: p.task_id, workspace_id: "ws-1", diff: DIFF }),
    "approvals.resolve": (p) => {
      const target = state.list.find((a) => a.id === p.approval_id)!;
      const next: Approval =
        p.decision === "approve"
          ? { ...target, state: { state: "approved", by: { kind: "user", id: "u" } } }
          : { ...target, state: { state: "rejected" } };
      state.list = state.list.map((a) => (a.id === next.id ? next : a));
      return next;
    },
    "delivery.request": (p) => ({ task_id: p.task_id, step_id: "step-1", outcome: "recorded" }),
  });
  const evidence = evidenceSource(
    opts.fixtureEvidence ? "fixture" : "daemon",
    (a) => ({ state: "available", evidence: evidenceFor(a, opts.fixtureEvidence ? { provenance: "fixture" } : {}) }),
    approvals,
  );
  let n = 0;
  const newKey = () => `key-${++n}`;
  const services = createServices();
  const changeRequestSource = opts.changeRequestSource ?? createFixtureChangeRequestSource();
  const tree = () => (
    <ServicesProvider services={services}>
      <ReviewDecisionsView
        client={client}
        evidenceSource={evidence}
        changeRequestSource={changeRequestSource}
        newKey={newKey}
      />
    </ServicesProvider>
  );
  const utils = render(tree());
  // Leaving the view and coming back: same daemon client, a fresh mount.
  const remount = () => {
    utils.unmount();
    return render(tree());
  };
  return { client, services, state, remount, ...utils };
}

const queue = () => screen.getByRole("region", { name: "Decision queue" });
const row = (taskId: string) => within(queue()).getByRole("button", { name: new RegExp(`^Task ${taskId}(?!\\d)`) });
const accept = () => screen.getByRole("button", { name: "Accept" }) as HTMLButtonElement;

describe("Review & Decisions view", () => {
  it("is a landmark-clean heading view inside the shell main: no own main, one h1, no select", async () => {
    liveSetup();
    expect(await screen.findByRole("heading", { level: 1, name: /Review & Decisions/ })).toBeTruthy();
    expect(screen.queryByRole("main")).toBeNull();
    expect(screen.queryByRole("combobox", { name: "Pending decision" })).toBeNull();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByRole("heading", { level: 2, name: "Decision queue" })).toBeTruthy();
    expect(screen.getByRole("heading", { level: 2, name: "Candidate and action panel" })).toBeTruthy();
  });

  it("lists every pending approval across projects and counts them", async () => {
    liveSetup();
    expect(await screen.findByText("2 independent decisions")).toBeTruthy();
    expect(row("task-1")).toBeTruthy();
    expect(row("task-2")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Actionable (2)" })).toBeTruthy();
  });

  it("lists non-delivery decisions as 'not reviewed here' entries and never offers a decision for them", async () => {
    const { client } = liveSetup({
      approvals: [approval({ id: "approval-1", taskId: "task-1" }), policy("approval-9", "task-9")],
    });
    const section = await screen.findByRole("region", { name: /Not reviewed here \(1\)/ });
    expect(within(section).getByText("1 non-delivery decision(s) are not reviewed here. This view cannot approve or reject them.")).toBeTruthy();
    const entry = within(section).getByRole("listitem");
    expect(entry).toHaveTextContent("Task task-9");
    expect(entry).toHaveTextContent("Policy change · approval approval-9");
    expect(within(entry).getByText("Not reviewed here")).toBeTruthy();
    expect(within(entry).queryByRole("button")).toBeNull();
    expect(screen.getByText("1 independent decision")).toBeTruthy();
    expect(client.callsOf("approvals.resolve")).toEqual([]);
  });

  it("states the non-delivery count once when nothing is reviewable, and lists the entries", async () => {
    liveSetup({ approvals: [policy("approval-9", "task-9")] });
    await screen.findByText("No pending decisions.");
    expect(screen.getAllByText(/non-delivery decision\(s\) are not reviewed here/)).toHaveLength(1);
    expect(screen.getByRole("region", { name: /Not reviewed here \(1\)/ })).toBeTruthy();
  });

  it("caps a long not-reviewed list and says how many are left", async () => {
    const many = Array.from({ length: 12 }, (_, i) => policy(`approval-p${i}`, `task-p${i}`));
    liveSetup({ approvals: [approval({ id: "approval-1", taskId: "task-1" }), ...many] });
    const section = await screen.findByRole("region", { name: /Not reviewed here \(12\)/ });
    expect(within(section).getAllByRole("listitem")).toHaveLength(10);
    expect(within(section).getByText("And 2 more not listed.")).toBeTruthy();
  });

  it("selecting a queue row loads that candidate's context", async () => {
    const { client } = liveSetup();
    await screen.findByText("2 independent decisions");
    await userEvent.click(row("task-2"));
    await waitFor(() => expect(screen.getByRole("region", { name: "Candidate identity" })).toHaveTextContent("approval-2"));
    expect(client.callsOf("tasks.get").map((p) => p.task_id)).toContain("task-2");
    expect(row("task-2")).toHaveAttribute("aria-current", "true");
  });
});

describe("tabs", () => {
  it("filters by tab, is keyboard operable, and explains the unsupported Verification tab", async () => {
    liveSetup();
    await screen.findByText("2 independent decisions");
    const tablist = screen.getByRole("tablist", { name: "Decision filters" });
    expect(within(tablist).getAllByRole("tab").map((t) => t.textContent?.replace(/ \(\d+\)/, ""))).toEqual([
      "Actionable",
      "Approvals",
      "Verification",
      "Candidates",
      "Delivery",
      "Resolved",
    ]);
    screen.getByRole("tab", { name: /Actionable/ }).focus();
    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    const verification = screen.getByRole("tab", { name: "Verification" });
    expect(verification).toHaveAttribute("aria-selected", "true");
    expect(verification).toHaveFocus();
    expect(screen.getByRole("tabpanel")).toHaveTextContent(/no separate verification decision/i);
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("tab", { name: /Resolved/ })).toHaveAttribute("aria-selected", "true");
    expect(within(screen.getByRole("tabpanel")).getByText("Nothing in this tab")).toBeTruthy();
  });

  it("moves an accepted approval to Resolved and Delivery without making a delivery request", async () => {
    const { client } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon: approval-1 is now approved/);
    expect(client.callsOf("delivery.request")).toEqual([]);

    await userEvent.click(screen.getByRole("tab", { name: /Delivery/ }));
    expect(within(screen.getByRole("tabpanel")).getByRole("button", { name: /Task task-1/ })).toBeTruthy();
    await userEvent.click(screen.getByRole("tab", { name: /Resolved/ }));
    expect(within(screen.getByRole("tabpanel")).getByRole("button", { name: /Task task-1/ })).toBeTruthy();
    await userEvent.click(screen.getByRole("tab", { name: /Actionable/ }));
    expect(within(screen.getByRole("tabpanel")).queryByRole("button", { name: /Task task-1/ })).toBeNull();
  });
});

describe("remounting the view (F2)", () => {
  const delivery = () => screen.getByRole("button", { name: "Request delivery" }) as HTMLButtonElement;
  const inTab = async (tab: RegExp, taskId: string) => {
    await userEvent.click(screen.getByRole("tab", { name: tab }));
    return within(screen.getByRole("tabpanel")).getByRole("button", { name: new RegExp(`^Task ${taskId}(?!\\d)`) });
  };

  it("keeps an approved-but-undelivered candidate reachable and deliverable after leaving and returning", async () => {
    const { client, remount } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon: approval-1 is now approved/);

    remount();
    // The daemon lists pending approvals only, so approval-1 is no longer in its list.
    await screen.findByText("1 independent decision");
    expect(client.callsOf("approvals.list").length).toBeGreaterThan(1);
    await userEvent.click(await inTab(/Delivery/, "task-1"));
    await waitFor(() => expect(screen.getByRole("region", { name: "Candidate identity" })).toHaveTextContent("approval-1"));
    await waitFor(() => expect(delivery().disabled).toBe(false));
    expect(client.callsOf("delivery.request")).toEqual([]);
    expect(client.callsOf("approvals.resolve")).toHaveLength(1);

    await userEvent.click(delivery());
    await screen.findByText(/Delivery request receipt from daemon/);
    expect(client.callsOf("delivery.request")).toEqual([
      expect.objectContaining({ approval_id: "approval-1", task_id: "task-1", action: "local_review", idempotency_key: expect.any(String) }),
    ]);
  });

  it("keeps a rejected decision in Resolved", async () => {
    const { remount } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Reject" }));
    await screen.findByText(/is now rejected/);

    remount();
    await screen.findByText("1 independent decision");
    const resolved = await inTab(/Resolved/, "task-1");
    expect(resolved).toHaveTextContent("Rejected");
  });

  it("keeps the delivery receipt, so a second mount shows the recorded request instead of offering nothing", async () => {
    const { client, remount } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon: approval-1 is now approved/);
    await userEvent.click(await screen.findByRole("button", { name: "Request delivery" }));
    await screen.findByText(/Delivery request receipt from daemon/);
    const firstKey = client.callsOf("delivery.request")[0].idempotency_key;

    remount();
    await screen.findByText("1 independent decision");
    await userEvent.click(await inTab(/Resolved/, "task-1"));
    const facts = await screen.findByRole("region", { name: /Six separate facts/ });
    await within(facts).findByText("Request recorded");
    expect(screen.getByText(/Delivery request receipt from daemon/)).toBeTruthy();

    // A repeat request after the remount replays the same idempotency key.
    const again = delivery();
    if (!again.disabled) {
      await userEvent.click(again);
      await waitFor(() => expect(client.callsOf("delivery.request")).toHaveLength(2));
      expect(client.callsOf("delivery.request")[1].idempotency_key).toBe(firstKey);
    }
  });

  it("retries an indeterminate decision with the original idempotency key after a remount", async () => {
    const { client, remount } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    let attempts = 0;
    client.setHandler("approvals.resolve", (p) => {
      attempts += 1;
      if (attempts === 1) fail("timeout");
      return { ...approval({ id: String(p.approval_id) }), state: { state: "approved", by: { kind: "user", id: "u" } } };
    });
    await userEvent.click(accept());
    await screen.findByText(/Decision failed \(timeout\)/);

    remount();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    expect(client.callsOf("approvals.resolve").map((c) => c.idempotency_key)).toEqual(["key-1", "key-1"]);
  });

  it("does not leak resolved decisions between different daemon clients", async () => {
    const first = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    first.unmount();

    liveSetup();
    await screen.findByText("2 independent decisions");
    expect(screen.getByRole("tab", { name: "Resolved (0)" })).toBeTruthy();
  });
});

describe("Request changes in the view", () => {
  it("goes through the change-request source for the selected approval and is never an approvals.resolve", async () => {
    const submitted: Array<Record<string, string>> = [];
    const changeRequestSource: ChangeRequestSource = {
      provenance: "daemon",
      labels: { description: "Ask for changes.", submitAction: "Send feedback", receiptNote: "" },
      loadHistory: async () => ({ state: "available", items: [] }),
      submit: async (input) => {
        submitted.push({ ...input });
        return {
          provenance: "daemon",
          requestId: "cr-1",
          taskId: input.taskId,
          candidateCommit: input.candidateCommit,
          feedback: input.feedback,
          resultingState: "changes requested",
          submittedAt: "now",
        };
      },
    };
    const { client } = liveSetup({ changeRequestSource });
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    const panel = screen.getByRole("region", { name: "Request changes" });
    await userEvent.click(within(panel).getByRole("button", { name: "Request changes" }));
    await userEvent.type(within(panel).getByLabelText(/Feedback/), "please add tests");
    await userEvent.click(within(panel).getByRole("button", { name: "Send feedback" }));
    await within(panel).findByText(/Receipt cr-1: changes requested/);
    expect(submitted).toEqual([
      { taskId: "task-1", approvalId: "approval-1", candidateCommit: "a".repeat(40), feedback: "please add tests" },
    ]);
    expect(client.callsOf("approvals.resolve")).toEqual([]);
    expect(client.callsOf("delivery.request")).toEqual([]);
  });
});

describe("real approval flow (release gate)", () => {
  it("Accept sends one approvals.resolve with the approval id and an idempotency key", async () => {
    const { client } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    expect(client.callsOf("approvals.resolve")).toEqual([
      { approval_id: "approval-1", decision: "approve", idempotency_key: "key-1" },
    ]);
  });

  it("Reject uses approvals.resolve reject", async () => {
    const { client } = liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(screen.getByRole("button", { name: "Reject" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Reject" }));
    await screen.findByText(/is now rejected/);
    expect(client.callsOf("approvals.resolve")[0]).toMatchObject({ approval_id: "approval-1", decision: "reject" });
  });

  it("fixture evidence never enables Accept and no mutation is sent", async () => {
    const { client } = liveSetup({ fixtureEvidence: true });
    await screen.findAllByText(/cannot authorize approval or delivery/);
    expect(accept().disabled).toBe(true);
    await userEvent.click(accept());
    expect(client.callsOf("approvals.resolve")).toEqual([]);
    expect(client.callsOf("delivery.request")).toEqual([]);
  });

  it("a fixture-prefixed id reaching a live daemon client is refused before any RPC", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("fixture:task-1", "/repos/alpha", { state: "Succeeded" })];
    fake.approvals = [mkApproval("fixture:approval-1", "fixture:task-1")];
    const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 } });
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    try {
      render(
        <ServicesProvider services={services}>
          <ReviewDecisionsView />
        </ServicesProvider>,
      );
      await screen.findByText("1 independent decision");
      const reject = await screen.findByRole("button", { name: "Reject" });
      await waitFor(() => expect(reject).toBeEnabled());
      await userEvent.click(reject);
      expect((await screen.findAllByText(/placeholder ids cannot be sent to the daemon/)).length).toBeGreaterThan(0);
      expect(fake.callsTo("approvals.resolve")).toEqual([]);
      expect(fake.callsTo("review.request_changes")).toEqual([]);
      expect(fake.callsTo("delivery.request")).toEqual([]);
    } finally {
      services.connection.stop();
    }
  });
});

describe("six separate facts", () => {
  it("shows facts the daemon does not carry as Not available, never as success", async () => {
    liveSetup();
    const facts = await screen.findByRole("region", { name: /Six separate facts/ });
    await within(facts).findByText("Passed");
    const item = (id: string) => facts.querySelector(`[data-fact="${id}"]`) as HTMLElement;
    expect(within(item("claim")).getByText("Not available")).toBeTruthy();
    expect(within(item("review")).getByText("Not available")).toBeTruthy();
    expect(within(item("integrated")).getByText("Not available")).toBeTruthy();
    expect(within(item("execution")).getByText("Finished")).toBeTruthy();
    expect(within(item("approved")).getByText("Awaiting your decision")).toBeTruthy();
    expect(within(item("checks")).getByText("Passed")).toBeTruthy();
    expect(within(facts).getAllByRole("listitem")).toHaveLength(6);
  });

  it("reports unavailable evidence as Not available for automated checks", async () => {
    const approvals = [approval()];
    const client = makeClient({
      "approvals.list": () => approvals,
      "tasks.get": () => taskRecord("task-1"),
      "workspaces.diff": () => ({ task_id: "task-1", workspace_id: "ws-1", diff: DIFF }),
    });
    render(
      <ServicesProvider services={createServices()}>
        <ReviewDecisionsView
          client={client}
          evidenceSource={evidenceSource("daemon", () => ({ state: "missing", provenance: "daemon", reason: "no run" }), approvals)}
          changeRequestSource={createFixtureChangeRequestSource()}
        />
      </ServicesProvider>,
    );
    const facts = await screen.findByRole("region", { name: /Six separate facts/ });
    const checks = await waitFor(() => {
      const el = facts.querySelector('[data-fact="checks"]') as HTMLElement;
      expect(within(el).getByText("Not available")).toBeTruthy();
      return el;
    });
    expect(checks).toHaveTextContent("no run");
  });

  it("shows a delivery receipt as a recorded request, not as integration", async () => {
    liveSetup();
    await screen.findByText("2 independent decisions");
    await waitFor(() => expect(accept().disabled).toBe(false));
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    await userEvent.click(await screen.findByRole("button", { name: "Request delivery" }));
    const facts = screen.getByRole("region", { name: /Six separate facts/ });
    await within(facts).findByText("Request recorded");
    expect(facts.querySelector('[data-fact="integrated"]')).toHaveTextContent(/not proof of integration/);
  });
});

describe("unsupported actions", () => {
  it("are disabled with a visible reason, and never call the daemon", async () => {
    const { client } = liveSetup();
    await screen.findByText("2 independent decisions");
    const cases: Array<[string, RegExp]> = [
      ["Acknowledge", /no acknowledge decision/],
      ["Postpone", /cannot postpone/],
      ["Dismiss", /cannot dismiss/],
      ["Escalate", /not defined/],
      ["Request additional checks", /no method to request more checks/],
      ["Review PR creation authorization", /never creates a pull request/],
      ["Export audit records", /no audit export/],
      ["Inspect integration history", /no integration history/],
      ["Plan recovery", /not supported/],
    ];
    const before = client.calls.length;
    for (const [name, reason] of cases) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription(reason);
      await userEvent.click(button);
    }
    expect(client.calls.length).toBe(before);
  });

  it("scope row marks owner, response-time sort and silences as unavailable", async () => {
    liveSetup();
    await screen.findByText("2 independent decisions");
    expect(screen.getByRole("button", { name: "All owners" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "By useful response time" })).toBeDisabled();
    expect(screen.getByText("Active silences: not available")).toBeTruthy();
  });
});

describe("live mode carries no illustrative values", () => {
  it("shows no PLACEHOLDER badge and no priority or response time from the wireframe", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("t-1", "/repos/alpha", { state: "Succeeded" })];
    fake.approvals = [mkApproval("a-1", "t-1")];
    const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 } });
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    try {
      render(
        <ServicesProvider services={services}>
          <ReviewDecisionsView />
        </ServicesProvider>,
      );
      await screen.findByText("1 independent decision");
      await waitFor(() => expect(within(queue()).getByText(/Priority and latest response time: not available/)).toBeTruthy());
      const view = screen.getByTestId("review-decisions");
      expect(view.textContent).not.toMatch(/PLACEHOLDER/);
      expect(view.textContent).not.toMatch(/\bP[12]\b/);
      expect(view.textContent).not.toMatch(/14:10|17:00|E1/);
    } finally {
      services.connection.stop();
    }
  });
});

describe("fixture mode", () => {
  it("labels fixture-backed values PLACEHOLDER and keeps Accept disabled", async () => {
    render(
      <ServicesProvider services={createServices()}>
        <ReviewDecisionsView fixtureMode />
      </ServicesProvider>,
    );
    await screen.findByText("1 independent decision");
    expect(screen.getAllByText("FIXTURE DAEMON — PLACEHOLDER DATA").length).toBe(1);
    expect(await within(queue()).findByText(/Priority: P1/)).toBeTruthy();
    expect(within(queue()).getAllByText("PLACEHOLDER").length).toBeGreaterThan(0);
    const facts = screen.getByRole("region", { name: /Six separate facts/ });
    expect(within(facts).getAllByText("PLACEHOLDER DATA").length).toBeGreaterThan(0);
    await screen.findAllByText(/cannot authorize approval or delivery/);
    expect(accept().disabled).toBe(true);
  });
});

describe("registration", () => {
  it("registers only the #/review global view", () => {
    expect(Object.keys(reviewGlobalViews)).toEqual(["review"]);
  });
});
