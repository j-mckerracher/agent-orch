import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CopilotSignInRecord } from "../../../daemon/types";
import type { SurfaceResult } from "../../../sources/surfaces/types";
import { CopilotSignInCheck } from "./CopilotSignInCheck";

const ok = (data: CopilotSignInRecord) => ({ status: "ok", data }) as unknown as SurfaceResult<CopilotSignInRecord>;

describe("CopilotSignInCheck: a completed turn with a hook warning (DPES-1105)", () => {
  it("shows ready, the hook as a visible warning, and the hooks sentence in the provider details", async () => {
    const check = vi.fn(async () =>
      ok({
        state: "ready",
        reason: null,
        checked_at_ms: 1,
        premium_requests: 0.33,
        ai_credits: 2.940125,
        warning: "agentStop hook from \"global settings\" failed in the session sandbox",
      }),
    );
    render(<CopilotSignInCheck check={check} />);
    expect(screen.getByText("Copilot runs your own Copilot hooks from ~/.copilot")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() =>
      expect(screen.getByTestId("copilot-hook-warning")).toHaveTextContent(
        "Warning: agentStop hook from \"global settings\" failed in the session sandbox",
      ),
    );
    expect(screen.getByText(/^Ready: GitHub Copilot is signed in/)).toBeInTheDocument();
    expect(screen.getByText("Reported by Copilot: 0.33 premium requests, 2.940125 AI credits.")).toBeInTheDocument();
  });

  it("shows no warning line when the run had no hook error", async () => {
    const check = vi.fn(async () =>
      ok({ state: "ready", reason: null, checked_at_ms: 1, premium_requests: null, ai_credits: null, warning: null }),
    );
    render(<CopilotSignInCheck check={check} />);
    await userEvent.click(screen.getByRole("button", { name: "Check Copilot sign-in" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Ready: GitHub Copilot is signed in"));
    expect(screen.queryByTestId("copilot-hook-warning")).toBeNull();
  });
});
