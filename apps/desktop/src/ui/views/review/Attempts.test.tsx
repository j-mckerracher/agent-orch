import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ReviewAttemptWire } from "../../../daemon/types";
import { Attempts, AttemptsList } from "./Attempts";

const none: ReviewAttemptWire["usage"] = {
  input_tokens: null,
  cache_read_tokens: null,
  cache_write_tokens: null,
  output_tokens: null,
  reasoning_tokens: null,
};

const attempt = (overrides: Partial<ReviewAttemptWire>): ReviewAttemptWire => ({
  session_id: "s1",
  agent: "codex",
  requested: null,
  reported_model: null,
  usage: none,
  cost: null,
  coverage: "unknown",
  ...overrides,
});

describe("Attempts", () => {
  it("shows one row per attempt and says not reported instead of zero", () => {
    render(<AttemptsList attempts={[attempt({}), attempt({ session_id: "s2" })]} />);
    expect(screen.getAllByText(/^Session s/)).toHaveLength(2);
    expect(screen.getAllByText(/Usage unknown/)).toHaveLength(2);
    const row = screen.getAllByText(/^Tokens:/)[0].textContent ?? "";
    expect(row).toContain("input not reported");
    expect(row).not.toMatch(/\b0\b/);
    expect(screen.getAllByText("Cost: not reported")).toHaveLength(2);
    expect(screen.getAllByText("Requested: not set")).toHaveLength(2);
  });

  it("shows requested and reported models side by side, tokens and money labelled", () => {
    render(
      <AttemptsList
        attempts={[
          attempt({
            requested: { model: "gpt-6-luna", effort: "high" },
            reported_model: "gpt-6-sol",
            usage: { ...none, input_tokens: 1200, cache_read_tokens: 0, output_tokens: 30 },
            cost: { amount_nanos: 1_500_000_000, currency: "USD" },
            coverage: "partial",
          }),
        ]}
      />,
    );
    expect(screen.getByText("Requested: model gpt-6-luna, effort high")).toBeInTheDocument();
    expect(screen.getByText("Reported by usage receipt: model gpt-6-sol")).toBeInTheDocument();
    const tokens = screen.getByText(/^Tokens:/).textContent ?? "";
    expect(tokens).toContain("input 1,200");
    expect(tokens).toContain("cache read 0");
    expect(tokens).toContain("cache write not reported");
    expect(screen.getByText("Cost: 1.50 USD (money, not tokens)")).toBeInTheDocument();
    expect(screen.getByText("Usage partly reported")).toBeInTheDocument();
  });

  it("says so when the task has no sessions", () => {
    render(<AttemptsList attempts={[]} />);
    expect(screen.getByText("This task has no sessions.")).toBeInTheDocument();
  });

  it("loads the attempts for the approval and renders nothing when the daemon fails", async () => {
    const { unmount } = render(<Attempts taskId="t" approvalId="a" load={async () => [attempt({})]} />);
    expect(await screen.findByTestId("review-attempts")).toBeInTheDocument();
    unmount();
    render(<Attempts taskId="t" approvalId="a" load={() => Promise.reject(new Error("down"))} />);
    await Promise.resolve();
    expect(screen.queryByTestId("review-attempts")).toBeNull();
  });
});
