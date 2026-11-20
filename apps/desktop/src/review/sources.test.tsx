import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../App";
import { DaemonError } from "../daemon/errors";
import { createServices } from "../ui/createServices";
import { reviewGlobalViews } from "../ui/views/review/register";
import { createFixtureChangeRequestSource, createFixtureReviewEvidenceSource } from "./sources";

const request = { taskId: "t", approvalId: "a", candidateCommit: "c", digestHex: "d" };

describe("fixture adapters", () => {
  it("force fixture provenance even for the 'success' scenario", async () => {
    const source = createFixtureReviewEvidenceSource("success", {
      default: "success",
      scenarios: { success: { result: { state: "available", evidence: { provenance: "daemon" } as never } } },
    });
    const result = await source.load(request);
    expect(source.provenance).toBe("fixture");
    expect(result.state === "available" && result.evidence.provenance).toBe("fixture");
  });

  it.each(["success", "missing", "error", "unavailable", "loading"])("evidence scenario %s resolves", async (scenario) => {
    const result = await createFixtureReviewEvidenceSource(scenario).load(request);
    const provenance = result.state === "available" ? result.evidence.provenance : result.provenance;
    expect(provenance).toBe("fixture");
  });

  it.each(["success", "empty", "error", "unavailable"])("change-request scenario %s works", async (scenario) => {
    const source = createFixtureChangeRequestSource(scenario);
    const history = await source.loadHistory("t");
    expect(["available", "empty", "error", "unavailable"]).toContain(history.state);
    expect(source.provenance).toBe("fixture");
  });
});

describe("daemon error mapping", () => {
  it.each([
    [-32011, "conflict"],
    [-32012, "forbidden"],
    [-32013, "idempotency-conflict"],
    [-32016, "result-too-large"],
    [-32017, "replay-gap"],
    [-32004, "capability-not-negotiated"],
    [-12345, "protocol"],
  ])("maps %i to %s", (code, kind) => expect(DaemonError.fromRpc(code, "m").kind).toBe(kind));

  it("treats timeouts and internal errors as indeterminate, conflicts as definite", () => {
    expect(new DaemonError("timeout", "m").indeterminate).toBe(true);
    expect(new DaemonError("internal", "m").indeterminate).toBe(true);
    expect(new DaemonError("conflict", "m").indeterminate).toBe(false);
  });
});

describe("S6 mounted in the app shell", () => {
  const mountReview = (search = "") => {
    window.history.replaceState({}, "", `/${search}`);
    const services = createServices({ initialRoute: { name: "global", view: "review", taskId: "fixture-task-1" } });
    return render(<App services={services} globalViews={reviewGlobalViews} />);
  };
  afterEach(() => window.history.replaceState({}, "", "/"));

  it("defaults to the real client: Offline strip, error state and no fixture data", async () => {
    mountReview();
    expect((await screen.findAllByRole("alert")).some((alert) => /Pending decisions are unavailable/.test(alert.textContent ?? ""))).toBe(true);
    expect(screen.getByTestId("connection-status")).toHaveTextContent("Offline");
    expect(screen.queryByText(/fixture-approval-1/)).toBeNull();
  });

  it("uses the labeled fixture daemon only when explicitly requested", async () => {
    mountReview("?fixture=1");
    await userEvent.click(await screen.findByText("Technical details"));
    expect(await screen.findByTestId("fixture-note")).toHaveTextContent(/placeholder data/);
    expect(await screen.findByRole("button", { name: "Accept" })).toBeTruthy();
  });
});
