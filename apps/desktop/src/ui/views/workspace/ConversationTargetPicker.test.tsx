import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ConversationTargetPicker } from "./ConversationTargetPicker";
import { GLOBAL_TARGET, type TargetOption } from "./targets";

const global: TargetOption = {
  key: "global",
  target: GLOBAL_TARGET,
  label: "Global orchestrator",
  scope: "All workstreams",
  placeholder: false,
};
const auth: TargetOption = {
  key: "project:auth",
  target: { kind: "project", projectId: "auth" },
  label: "auth coordinator",
  scope: "auth",
  placeholder: false,
};
const billing: TargetOption = {
  key: "project:billing",
  target: { kind: "project", projectId: "billing" },
  label: "billing coordinator",
  scope: "billing",
  placeholder: false,
};

/** The picker with its selection held in state, as the workspace holds it. */
function Harness({ onSelect }: { onSelect: (option: TargetOption) => void }) {
  const [selected, setSelected] = useState("global");
  return (
    <ConversationTargetPicker
      options={[global, auth, billing]}
      selectedKey={selected}
      onSelect={(option) => {
        onSelect(option);
        setSelected(option.key);
      }}
    />
  );
}

function setup() {
  const onSelect = vi.fn();
  const user = userEvent.setup();
  render(<Harness onSelect={onSelect} />);
  const trigger = screen.getByRole("button", { name: /Choose conversation target/ });
  const search = () => screen.getByRole("searchbox", { name: "Search conversations" });
  return { user, onSelect, trigger, search };
}

describe("conversation target picker closes after a selection", () => {
  it("a pointer press on a radio selects the target, closes the panel and returns focus to the trigger", async () => {
    const { user, onSelect, trigger } = setup();
    await user.click(trigger);
    await user.click(screen.getByRole("radio", { name: /auth coordinator/ }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect.mock.calls[0][0]).toMatchObject({ key: "project:auth" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("radio")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("a pointer press on the option label closes the panel too", async () => {
    const { user, onSelect, trigger } = setup();
    await user.click(trigger);
    await user.click(screen.getByText("billing coordinator"));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect.mock.calls[0][0]).toMatchObject({ key: "project:billing" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
  });

  it("arrow keys move and select through the radios but keep the panel open", async () => {
    const { user, onSelect, trigger, search } = setup();
    await user.click(trigger);
    expect(search()).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /Global orchestrator/ })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /auth coordinator/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /auth coordinator/ })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /billing coordinator/ })).toBeChecked();
    expect(onSelect).toHaveBeenCalledTimes(2);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("radio", { name: /billing coordinator/ })).toHaveFocus();
  });

  it("Enter on a radio selects that target and closes the panel", async () => {
    const { user, onSelect, trigger, search } = setup();
    await user.click(trigger);
    await user.type(search(), "billing");
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /billing coordinator/ })).toHaveFocus();
    expect(onSelect).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect.mock.calls[0][0]).toMatchObject({ key: "project:billing" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
  });

  it("Space on a radio selects that target and closes the panel", async () => {
    const { user, onSelect, trigger, search } = setup();
    await user.click(trigger);
    await user.type(search(), "billing");
    await user.keyboard("{ArrowDown}");
    await user.keyboard(" ");
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect.mock.calls[0][0]).toMatchObject({ key: "project:billing" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
  });

  it("Space on the already selected radio closes the panel without selecting it again", async () => {
    const { user, onSelect, trigger } = setup();
    await user.click(trigger);
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /Global orchestrator/ })).toHaveFocus();
    await user.keyboard(" ");
    expect(onSelect).not.toHaveBeenCalled();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
  });

  it("Escape closes the panel, returns focus to the trigger and selects nothing", async () => {
    const { user, onSelect, trigger } = setup();
    await user.click(trigger);
    await user.keyboard("{ArrowDown}");
    await user.keyboard("{Escape}");
    expect(onSelect).not.toHaveBeenCalled();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
    expect(screen.getByRole("button", { name: /Choose conversation target/ })).toHaveTextContent("Global orchestrator");
  });
});
