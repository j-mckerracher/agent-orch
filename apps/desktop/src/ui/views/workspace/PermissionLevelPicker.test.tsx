import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { PermissionLevelNotes, PermissionLevelPicker } from "./PermissionLevelPicker";
import { OLDER_DAEMON_NOTICE } from "./permissionLevel";

function picker(level: PermissionLevel, onChange: (next: PermissionLevel) => void, options: { daemonAdvertises?: boolean; agent?: string } = {}) {
  return render(
    <PermissionLevelPicker
      level={level}
      daemonAdvertises={options.daemonAdvertises ?? true}
      agent={options.agent}
      onChange={onChange}
    />,
  );
}

describe("the task permission level control", () => {
  it("lists the four levels, with Extended disabled and its reason shown", () => {
    picker("auto", vi.fn());
    const select = screen.getByRole("combobox", { name: "Permission level" });
    expect(select).toHaveValue("auto");
    expect(screen.getByRole("option", { name: "Restricted" })).toBeEnabled();
    expect(screen.getByRole("option", { name: "Extended (read-only git only, so it can't build, test or commit)" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "Auto" })).toBeEnabled();
    expect(screen.getByRole("option", { name: "Full" })).toBeEnabled();
  });

  it("changes straight to Restricted or Auto without a confirmation", async () => {
    const onChange = vi.fn();
    picker("auto", onChange);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Restricted");
    expect(onChange).toHaveBeenCalledWith("restricted");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks before Full, with Cancel focused, and keeps the level on Cancel", async () => {
    const onChange = vi.fn();
    picker("auto", onChange);
    const select = screen.getByRole("combobox", { name: "Permission level" });
    await userEvent.selectOptions(select, "Full");
    const dialog = await screen.findByRole("dialog", { name: "Run this task at Full?" });
    expect(onChange).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus());
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onChange).not.toHaveBeenCalled();
    expect(dialog).not.toBeInTheDocument();
    expect(select).toHaveValue("auto");
  });

  it("changes to Full only when the user confirms", async () => {
    const onChange = vi.fn();
    picker("auto", onChange);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    await userEvent.click(await screen.findByRole("button", { name: "Use Full" }));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("full");
  });

  it("closes the Full confirmation on Escape without changing the level", async () => {
    const onChange = vi.fn();
    picker("auto", onChange);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    await screen.findByRole("dialog");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("names only the agent a task names in the Full confirmation", async () => {
    picker("auto", vi.fn(), { agent: "codex" });
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("~/.codex");
    expect(dialog).not.toHaveTextContent("Copilot can run any shell command");
    expect(dialog).toHaveTextContent("Automatic review at Auto is not a guarantee either.");
  });

  it("names every agent that could answer when the daemon routes the task", async () => {
    picker("auto", vi.fn());
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("~/.codex");
    expect(dialog).toHaveTextContent("skips its permission checks");
    expect(dialog).toHaveTextContent("Copilot can run any shell command");
  });

  it("on an older daemon, leaves only Restricted choosable and says why", () => {
    picker("restricted", vi.fn(), { daemonAdvertises: false });
    expect(screen.getByRole("option", { name: "Restricted" })).toBeEnabled();
    expect(screen.getByRole("option", { name: "Auto (needs an updated daemon)" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "Full (needs an updated daemon)" })).toBeDisabled();
  });
});

describe("the task permission notes", () => {
  it("shows no note on a daemon that advertises permission levels", () => {
    render(<PermissionLevelNotes olderDaemon={false} />);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("shows the older-daemon notice when the daemon predates permission levels", () => {
    render(<PermissionLevelNotes olderDaemon />);
    expect(screen.getByRole("status")).toHaveTextContent(OLDER_DAEMON_NOTICE);
  });
});
