import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SIDEBAR_MAX, SIDEBAR_MIN } from "../layout/layoutSettings";
import { SidebarSplitter, type SidebarSplitterProps } from "./SidebarSplitter";

function setup(props: Partial<SidebarSplitterProps> = {}) {
  const handlers = { onResize: vi.fn(), onToggle: vi.fn(), onReset: vi.fn() };
  render(
    <>
      <section id="sb" />
      <SidebarSplitter controls="sb" width={214} collapsed={false} {...handlers} {...props} />
    </>,
  );
  return { ...handlers, separator: screen.getByRole("separator", { name: "Resize sidebar" }) };
}

describe("SidebarSplitter", () => {
  it("exposes the window-splitter ARIA contract", () => {
    const { separator } = setup();
    expect(separator).toHaveAttribute("aria-orientation", "vertical");
    expect(separator).toHaveAttribute("aria-controls", "sb");
    expect(separator).toHaveAttribute("aria-valuenow", "214");
    expect(separator).toHaveAttribute("aria-valuemax", String(SIDEBAR_MAX));
    expect(separator).toHaveAttribute("tabindex", "0");
  });

  it("reports a value of 0 while collapsed", () => {
    const { separator } = setup({ collapsed: true });
    expect(separator).toHaveAttribute("aria-valuenow", "0");
  });

  it("resizes with the arrow keys, using larger steps with Shift", async () => {
    const user = userEvent.setup();
    const { separator, onResize } = setup();
    separator.focus();
    await user.keyboard("{ArrowRight}");
    expect(onResize).toHaveBeenLastCalledWith(230);
    await user.keyboard("{ArrowLeft}");
    expect(onResize).toHaveBeenLastCalledWith(198);
    await user.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(onResize).toHaveBeenLastCalledWith(278);
  });

  it("jumps to the bounds with Home and End", async () => {
    const user = userEvent.setup();
    const { separator, onResize } = setup();
    separator.focus();
    await user.keyboard("{Home}");
    expect(onResize).toHaveBeenLastCalledWith(SIDEBAR_MIN);
    await user.keyboard("{End}");
    expect(onResize).toHaveBeenLastCalledWith(SIDEBAR_MAX);
  });

  it("toggles with Enter and resets on double-click", async () => {
    const user = userEvent.setup();
    const { separator, onToggle, onReset } = setup();
    separator.focus();
    await user.keyboard("{Enter}");
    expect(onToggle).toHaveBeenCalledTimes(1);
    await user.dblClick(separator);
    expect(onReset).toHaveBeenCalledTimes(1);
  });

  it("ignores ArrowLeft while collapsed and expands on ArrowRight", async () => {
    const user = userEvent.setup();
    const { separator, onResize } = setup({ collapsed: true });
    separator.focus();
    await user.keyboard("{ArrowLeft}");
    expect(onResize).not.toHaveBeenCalled();
    await user.keyboard("{ArrowRight}");
    expect(onResize).toHaveBeenLastCalledWith(16);
  });

  it("follows a pointer drag from the starting width", () => {
    const { separator, onResize } = setup();
    fireEvent.pointerDown(separator, { button: 0, clientX: 100, pointerId: 1 });
    fireEvent.pointerMove(separator, { clientX: 150, pointerId: 1 });
    expect(onResize).toHaveBeenLastCalledWith(264);
    fireEvent.pointerMove(separator, { clientX: 60, pointerId: 1 });
    expect(onResize).toHaveBeenLastCalledWith(174);
    fireEvent.pointerUp(separator, { pointerId: 1 });
    onResize.mockClear();
    fireEvent.pointerMove(separator, { clientX: 300, pointerId: 1 });
    expect(onResize).not.toHaveBeenCalled();
  });

  it("does not start a drag from a secondary button", () => {
    const { separator, onResize } = setup();
    fireEvent.pointerDown(separator, { button: 2, clientX: 100, pointerId: 1 });
    fireEvent.pointerMove(separator, { clientX: 150, pointerId: 1 });
    expect(onResize).not.toHaveBeenCalled();
  });

  it("labels the toggle by state and reports aria-expanded", async () => {
    const user = userEvent.setup();
    const { onToggle } = setup();
    const collapse = screen.getByRole("button", { name: "Collapse sidebar" });
    expect(collapse).toHaveAttribute("aria-expanded", "true");
    expect(collapse).toHaveAttribute("aria-controls", "sb");
    await user.click(collapse);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("offers Expand while collapsed", () => {
    setup({ collapsed: true });
    expect(screen.getByRole("button", { name: "Expand sidebar" })).toHaveAttribute("aria-expanded", "false");
  });
});
