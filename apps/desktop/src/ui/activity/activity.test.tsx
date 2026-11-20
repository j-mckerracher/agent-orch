import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityItem, LiveActivity } from "../../sources/surfaces/types";
import { ChatLog } from "../chat/ChatLog";
import { ActivityLog } from "./ActivityLog";
import { ActivityStatus } from "./ActivityStatus";

const live = (over: Partial<LiveActivity> = {}): LiveActivity => ({ phase: "thinking", tool: null, phaseMs: 4000, idleMs: 0, receivedAt: Date.now(), ...over });

describe("ActivityStatus", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("adds the time since the snapshot arrived, and re-renders only itself on the tick", () => {
    let renders = 0;
    const Spy = () => {
      renders += 1;
      return null;
    };
    const snapshot = live();
    const Parent = ({ children }: { children?: ReactNode }) => (
      <>
        <Spy />
        {children}
      </>
    );
    render(
      <Parent>
        <ActivityStatus live={snapshot} />
      </Parent>,
    );
    expect(screen.getByText("Thinking").closest("p")).toHaveTextContent("Thinking · 4 s");
    act(() => void vi.advanceTimersByTime(3000));
    expect(screen.getByText("Thinking").closest("p")).toHaveTextContent("Thinking · 7 s");
    expect(renders).toBe(1);
  });

  it("turns to No activity when the idle time reaches 20 s, counting up from the snapshot", () => {
    render(<ActivityStatus live={live({ idleMs: 18_000, phaseMs: 30_000 })} />);
    expect(screen.getByText("Thinking")).toBeInTheDocument();
    act(() => void vi.advanceTimersByTime(2000));
    expect(screen.getByText("No activity").closest("p")).toHaveTextContent("No activity for 20 s");
    act(() => void vi.advanceTimersByTime(5000));
    expect(screen.getByText("No activity").closest("p")).toHaveTextContent("No activity for 25 s");
  });

  it("announces the words in a polite live region and hides the ticking number from readers", () => {
    render(<ActivityStatus live={live({ phase: "tool", tool: { id: "a", name: "Read", summary: "" } })} />);
    const words = screen.getByRole("status");
    expect(words).toHaveAttribute("aria-live", "polite");
    expect(words).toHaveTextContent(/^Running Read$/);
  });

  it("names a phase it does not know as the daemon sent it, and shows the fallback with its wait time", () => {
    const { rerender } = render(<ActivityStatus live={live({ phase: "compacting" })} />);
    expect(screen.getByText("compacting")).toBeInTheDocument();
    rerender(<ActivityStatus live={null} fallback={{ text: "Waiting…", since: Date.now() - 6000 }} />);
    expect(screen.getByText("Waiting…").closest("p")).toHaveTextContent("Waiting… · 6 s");
    rerender(<ActivityStatus live={null} />);
    expect(document.querySelector(".act-status")).toBeNull();
  });
});

describe("ActivityLog", () => {
  it("shows the cap marker and one thinking block per run, open only while it is the newest item and running", () => {
    const items: ActivityItem[] = [
      { kind: "thinking", key: "t1", text: "first thought" },
      { kind: "tool", key: "u2", id: "a", name: "Read", summary: "", state: "ok", result: "" },
      { kind: "thinking", key: "t3", text: "second thought" },
      { kind: "cap", key: "c4" },
    ];
    const { container } = render(<ActivityLog items={items} running={false} />);
    expect(container.querySelectorAll("details")).toHaveLength(2);
    expect(screen.getByText(/stopped recording tool calls/)).toBeInTheDocument();
  });
});

describe("ChatLog scroll follow", () => {
  const metrics = (el: HTMLElement, scrollHeight: number, clientHeight: number) => {
    Object.defineProperty(el, "scrollHeight", { configurable: true, value: scrollHeight });
    Object.defineProperty(el, "clientHeight", { configurable: true, value: clientHeight });
  };

  it("follows the newest line until the person scrolls up, then offers Jump to latest", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ChatLog label="Thread" scrollKey={1}>one</ChatLog>);
    const log = screen.getByRole("log", { name: "Thread" });
    metrics(log, 1000, 200);
    rerender(<ChatLog label="Thread" scrollKey={2}>one two</ChatLog>);
    expect(log.scrollTop).toBe(1000);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();

    log.scrollTop = 100;
    fireEvent.scroll(log);
    rerender(<ChatLog label="Thread" scrollKey={3}>one two three</ChatLog>);
    expect(log.scrollTop).toBe(100);
    await user.click(screen.getByRole("button", { name: "Jump to latest" }));
    expect(log.scrollTop).toBe(1000);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
  });
});
