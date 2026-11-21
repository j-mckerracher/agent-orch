import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ModeGroup } from "../../../sources/surfaces/types";
import { ModeDropdown } from "./ModeDropdown";

const GROUPS: ModeGroup[] = [
  {
    id: "permission",
    label: "Permission",
    options: [
      { id: "auto", label: "Auto", description: "Runs most actions.", available: true, unavailableReason: null, needsConfirmation: false, isDefault: true, allowsActions: true },
      { id: "plan", label: "Plan", description: "Plans first.", available: true, unavailableReason: null, needsConfirmation: false, isDefault: false, allowsActions: true },
      { id: "full", label: "Full access", description: "No sandbox.", available: true, unavailableReason: null, needsConfirmation: true, isDefault: false, allowsActions: false },
    ],
  },
  {
    id: "work",
    label: "Work",
    options: [
      { id: "standard", label: "Standard", description: "Answers each message.", available: true, unavailableReason: null, needsConfirmation: false, isDefault: true, allowsActions: true },
      { id: "plan-mode", label: "Plan mode", description: "Plans.", available: false, unavailableReason: "Not available through codex exec.", needsConfirmation: false, isDefault: false, allowsActions: false },
    ],
  },
];

function renderDropdown(overrides: Partial<Parameters<typeof ModeDropdown>[0]> = {}) {
  const onChange = vi.fn();
  render(<ModeDropdown agent="codex" groups={GROUPS} selection={{}} disabled={null} onChange={onChange} {...overrides} />);
  return { onChange, trigger: screen.getByRole("button", { name: /Chat mode:/ }) };
}

describe("the mode dropdown trigger", () => {
  it("shows the selected labels joined with a middle dot", () => {
    const { trigger } = renderDropdown();
    expect(trigger.textContent).toContain("Auto · Standard");
  });

  it("gives the full text in its label and title", () => {
    const { trigger } = renderDropdown({ selection: { permission: "plan" } });
    expect(trigger).toHaveAttribute("aria-label", "Chat mode: Plan · Standard");
    expect(trigger).toHaveAttribute("title", "Plan · Standard");
  });

  it("shows the reason it cannot be picked as its label, and the hint in the panel", async () => {
    const user = userEvent.setup();
    const { trigger } = renderDropdown({
      agent: undefined,
      groups: [],
      disabled: { label: "Default mode", reason: "Choose an agent to pick its mode." },
    });
    expect(trigger.textContent).toBe("Default mode⌄");
    await user.click(trigger);
    expect(screen.getByRole("status")).toHaveTextContent("Choose an agent to pick its mode.");
  });
});

describe("the mode dropdown panel", () => {
  it("lists each group under its own name, with the option described", async () => {
    const user = userEvent.setup();
    renderDropdown();
    await user.click(screen.getByRole("button", { name: /Chat mode:/ }));
    const permission = screen.getByRole("group", { name: "Permission" });
    expect(within(permission).getByRole("radio", { name: /Auto/ })).toBeChecked();
    expect(within(permission).getByRole("radio", { name: /Plan/ })).toBeEnabled();
  });

  it("shows an unavailable option's reason instead of its description, and does not let it be chosen", async () => {
    const user = userEvent.setup();
    renderDropdown();
    await user.click(screen.getByRole("button", { name: /Chat mode:/ }));
    const work = screen.getByRole("group", { name: "Work" });
    const planMode = within(work).getByRole("radio", { name: /Plan mode/ });
    expect(planMode).toBeDisabled();
    expect(within(work).getByText("Not available through codex exec.")).toBeInTheDocument();
  });

  it("reports a choice with the rest of the selection unchanged", async () => {
    const user = userEvent.setup();
    const { onChange } = renderDropdown({ selection: { work: "standard" } });
    await user.click(screen.getByRole("button", { name: /Chat mode:/ }));
    await user.click(within(screen.getByRole("group", { name: "Permission" })).getByRole("radio", { name: /Plan/ }));
    expect(onChange).toHaveBeenCalledWith({ work: "standard", permission: "plan" });
  });
});

describe("a choice that needs confirmation", () => {
  it("applies only when the user confirms Full", async () => {
    const user = userEvent.setup();
    const { onChange } = renderDropdown();
    await user.click(screen.getByRole("button", { name: /Chat mode:/ }));
    await user.click(within(screen.getByRole("group", { name: "Permission" })).getByRole("radio", { name: /Full access/ }));
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "Run this chat at Full?" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Use Full" }));
    expect(onChange).toHaveBeenCalledWith({ permission: "full" });
  });

  it("applies nothing when the user cancels", async () => {
    const user = userEvent.setup();
    const { onChange } = renderDropdown();
    await user.click(screen.getByRole("button", { name: /Chat mode:/ }));
    await user.click(within(screen.getByRole("group", { name: "Permission" })).getByRole("radio", { name: /Full access/ }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onChange).not.toHaveBeenCalled();
  });
});
