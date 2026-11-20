import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ActionStatus, CLEAR_MS, MIN_PENDING_MS, useAction, type Outcome } from "./action";
import { Button } from "./theme";

let action!: ReturnType<typeof useAction>;
function Harness() {
  action = useAction();
  return (
    <>
      <Button pending={action.pending} onClick={() => void 0}>
        Go
      </Button>
      <ActionStatus action={action} />
    </>
  );
}

const saved: Outcome = { kind: "saved", text: "Saved. Done." };

beforeEach(() => {
  vi.useFakeTimers();
  render(<Harness />);
});
afterEach(() => vi.useRealTimers());

describe("useAction", () => {
  it("goes idle → pending → saved, keeps pending for the minimum time, then clears the success", async () => {
    let finish!: () => void;
    let run!: Promise<void>;
    act(() => {
      run = action.run("Saving…", () => new Promise<Outcome>((resolve) => { finish = () => resolve(saved); }));
    });
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Go" })).toHaveAttribute("aria-busy", "true");

    await act(async () => finish());
    expect(screen.getByText("Saving…")).toBeInTheDocument(); // answered at once, still shown
    await act(async () => { await vi.advanceTimersByTimeAsync(MIN_PENDING_MS); await run; });
    expect(screen.getByText("Saved. Done.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Go" })).not.toHaveAttribute("aria-busy");

    act(() => { vi.advanceTimersByTime(CLEAR_MS); });
    expect(screen.queryByText("Saved. Done.")).not.toBeInTheDocument();
  });

  it("shows a thrown error as an alert that stays, and Retry runs the action again", async () => {
    const fn = vi.fn<() => Promise<Outcome>>().mockRejectedValueOnce(new Error("Disk is full.")).mockResolvedValueOnce(saved);
    await act(async () => { void action.run("Saving…", fn); await vi.advanceTimersByTimeAsync(MIN_PENDING_MS); });

    expect(screen.getByRole("alert")).toHaveTextContent("Disk is full.");
    act(() => { vi.advanceTimersByTime(CLEAR_MS * 2); });
    expect(screen.getByRole("alert")).toBeInTheDocument();

    await act(async () => { screen.getByRole("button", { name: "Retry" }).click(); await vi.advanceTimersByTimeAsync(MIN_PENDING_MS); });
    expect(fn).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByText("Saved. Done.")).toBeInTheDocument();
  });

  it("ignores a second run while one is pending, and clear() removes a result", async () => {
    const fn = vi.fn(async () => saved);
    await act(async () => { void action.run("Saving…", fn); void action.run("Saving…", fn); await vi.advanceTimersByTimeAsync(MIN_PENDING_MS); });
    expect(fn).toHaveBeenCalledTimes(1);

    act(() => action.clear());
    expect(screen.queryByText("Saved. Done.")).not.toBeInTheDocument();
  });

  it("keeps the status region mounted for announcements, and a pending button ignores clicks without being disabled", () => {
    expect(screen.getByRole("status")).toBeInTheDocument();
    const onClick = vi.fn();
    const { rerender } = render(<Button pending onClick={onClick}>Pending</Button>);
    const button = screen.getByRole("button", { name: "Pending" });
    button.click();
    expect(onClick).not.toHaveBeenCalled();
    expect(button).not.toBeDisabled();
    expect(button).toHaveAttribute("aria-disabled", "true");
    rerender(<Button onClick={onClick}>Pending</Button>);
    screen.getByRole("button", { name: "Pending" }).click();
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
