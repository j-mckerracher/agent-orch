import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ThinkingBlock } from "./ThinkingBlock";

const details = (container: HTMLElement) => container.querySelector("details")!;

describe("ThinkingBlock", () => {
  it("renders the full text with no truncation, as Markdown", () => {
    const long = Array.from({ length: 2000 }, (_, i) => `line ${i}`).join("\n\n");
    const { container } = render(<ThinkingBlock text={`${long}\n\nend **bold**`} streaming={false} />);
    expect(container.querySelectorAll("p")).toHaveLength(2001);
    expect(screen.getByText("bold").tagName).toBe("STRONG");
    expect(screen.getByText("line 1999")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy thinking" })).toBeInTheDocument();
  });

  it("is open with a Streaming badge while streaming, then collapses", async () => {
    const { container, rerender } = render(<ThinkingBlock text="a" streaming />);
    expect(details(container).open).toBe(true);
    expect(screen.getByText("Streaming")).toBeInTheDocument();
    rerender(<ThinkingBlock text="ab" streaming={false} />);
    await waitFor(() => expect(details(container).open).toBe(false));
    expect(screen.queryByText("Streaming")).toBeNull();
    expect(screen.getByText("Thinking")).toBeInTheDocument();
  });

  it("starts collapsed when it is already finished", () => {
    const { container } = render(<ThinkingBlock text="a" streaming={false} />);
    expect(details(container).open).toBe(false);
  });

  it("keeps it closed after the person closes it while streaming", async () => {
    const user = userEvent.setup();
    const { container, rerender } = render(<ThinkingBlock text="a" streaming />);
    await user.click(screen.getByText("Thinking"));
    await waitFor(() => expect(details(container).open).toBe(false));
    rerender(<ThinkingBlock text="ab" streaming />);
    rerender(<ThinkingBlock text="ab" streaming={false} />);
    expect(details(container).open).toBe(false);
  });

  it("keeps it open after the person opens it once streaming has ended", async () => {
    const user = userEvent.setup();
    const { container, rerender } = render(<ThinkingBlock text="a" streaming />);
    rerender(<ThinkingBlock text="a" streaming={false} />);
    await waitFor(() => expect(details(container).open).toBe(false));
    await user.click(screen.getByText("Thinking"));
    await waitFor(() => expect(details(container).open).toBe(true));
    rerender(<ThinkingBlock text="a more" streaming={false} />);
    expect(details(container).open).toBe(true);
  });

  it("keeps a person's choice made while streaming even if it matches the streaming default", async () => {
    const user = userEvent.setup();
    const { container, rerender } = render(<ThinkingBlock text="a" streaming />);
    // Close then reopen: the last choice (open) is theirs, so it stays open after the stream ends.
    await user.click(screen.getByText("Thinking"));
    await waitFor(() => expect(details(container).open).toBe(false));
    await user.click(screen.getByText("Thinking"));
    await waitFor(() => expect(details(container).open).toBe(true));
    rerender(<ThinkingBlock text="a" streaming={false} />);
    expect(details(container).open).toBe(true);
  });
});
