import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CopilotSignInRecord } from "../../../daemon/types";
import type { SurfaceResult } from "../../../sources/surfaces/types";
import { CopilotSignInCheck } from "./CopilotSignInCheck";

// Only the fields the control reads matter here; the provenance and observation fields are not used.
const ok = (data: CopilotSignInRecord) => ({ status: "ok", data }) as unknown as SurfaceResult<CopilotSignInRecord>;
const refused = (reason: string) => ({ status: "unavailable", reason }) as unknown as SurfaceResult<CopilotSignInRecord>;

const READY: CopilotSignInRecord = {
  state: "ready",
  reason: null,
  checked_at_ms: 1,
  premium_requests: 0.33,
  ai_credits: null,
  warning: null,
};

describe("CopilotSignInCheck (DPES-1105)", () => {
  it("shows the cost before any call, makes one call on the button, and shows the state as text", async () => {
    const check = vi.fn(async () => ok(READY));
    render(<CopilotSignInCheck check={check} />);
    expect(screen.getByText(/one billable Copilot call on claude-haiku-4\.5/)).toBeInTheDocument();
    expect(check).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/Ready: GitHub Copilot is signed in/));
    expect(check).toHaveBeenCalledTimes(1);
    expect(check).toHaveBeenCalledWith(false);
    expect(screen.getByText("Reported by Copilot: 0.33 premium requests.")).toBeInTheDocument();
  });

  it("shows a not-signed-in result with the daemon's fix as text", async () => {
    const check = vi.fn(async () =>
      ok({ state: "not_signed_in", reason: "Not signed in to GitHub Copilot. Run `copilot login`, then retry.", checked_at_ms: 2, premium_requests: null, ai_credits: null, warning: null }),
    );
    render(<CopilotSignInCheck check={check} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Run `copilot login`, then retry."));
  });

  it("makes a new call after a failed sign-in, so a fixed sign-in is seen, and keeps a ready answer", async () => {
    const check = vi.fn(async (force: boolean) =>
      force
        ? ok(READY)
        : ok({ state: "not_signed_in", reason: "Not signed in to GitHub Copilot. Run `copilot login`, then retry.", checked_at_ms: 2, premium_requests: null, ai_credits: null, warning: null }),
    );
    render(<CopilotSignInCheck check={check} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/Not signed in to GitHub Copilot/));
    expect(check).toHaveBeenLastCalledWith(false);

    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/Ready: GitHub Copilot is signed in/));
    expect(check).toHaveBeenLastCalledWith(true);

    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(check).toHaveBeenCalledTimes(3));
    expect(check).toHaveBeenLastCalledWith(false);
  });

  it("shows a no-seat result as text, without implying a retry will help", async () => {
    const check = vi.fn(async () =>
      ok({ state: "no_seat", reason: "This GitHub account has no GitHub Copilot subscription.", checked_at_ms: 3, premium_requests: null, ai_credits: null, warning: null }),
    );
    render(<CopilotSignInCheck check={check} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("has no GitHub Copilot subscription."));
  });

  it("reports usage as not reported rather than zero when Copilot gave no figures", async () => {
    const check = vi.fn(async () =>
      ok({ state: "error", reason: "the Copilot run did not complete", checked_at_ms: 4, premium_requests: null, ai_credits: null, warning: null }),
    );
    render(<CopilotSignInCheck check={check} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByText("Usage: not reported by Copilot.")).toBeInTheDocument());
    expect(screen.queryByText(/Reported by Copilot/)).toBeNull();
  });

  it("shows a refused check's reason as text when the daemon cannot run Copilot here", async () => {
    const check = vi.fn(async () => refused("the copilot executable was not found"));
    render(<CopilotSignInCheck check={check} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("the copilot executable was not found"));
  });

  it("polls the free status read while the daemon's background check runs, then shows the settled state", async () => {
    const check = vi.fn(async () =>
      ok({ state: "checking", reason: null, checked_at_ms: null, premium_requests: null, ai_credits: null, warning: null }),
    );
    const status = vi
      .fn()
      .mockResolvedValueOnce(ok({ state: "checking", reason: null, checked_at_ms: null, premium_requests: null, ai_credits: null, warning: null }))
      .mockResolvedValueOnce(ok(READY));
    render(<CopilotSignInCheck check={check} status={status} pollIntervalMs={1} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/Ready: GitHub Copilot is signed in/));
    // The check starts once; polling uses only the free status read.
    expect(check).toHaveBeenCalledTimes(1);
    expect(status).toHaveBeenCalledTimes(2);
  });
});
