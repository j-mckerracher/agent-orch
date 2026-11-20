import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { LastTurnNote, PermissionLevelNotes, PermissionLevelPicker } from "./PermissionLevelPicker";
import { OLDER_DAEMON_NOTICE } from "./permissionLevel";

function picker(
  level: PermissionLevel,
  onChange: (next: PermissionLevel) => void,
  options: { daemonAdvertises?: boolean; agent?: string; closePanel?: () => void } = {},
) {
  return render(
    <PermissionLevelPicker
      level={level}
      daemonAdvertises={options.daemonAdvertises ?? true}
      agent={options.agent}
      surface="chat"
      onChange={onChange}
      closePanel={options.closePanel ?? vi.fn()}
    />,
  );
}

describe("permission level control in the model picker panel", () => {
  it("lists the four levels, with Extended disabled and its reason shown", () => {
    picker("auto", vi.fn());
    const select = screen.getByRole("combobox", { name: "Permission level" });
    expect(select).toHaveValue("auto");
    expect(screen.getByRole("option", { name: "Restricted" })).toBeEnabled();
    expect(screen.getByRole("option", { name: "Extended (not available for chat yet)" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "Auto" })).toBeEnabled();
    expect(screen.getByRole("option", { name: "Full" })).toBeEnabled();
  });

  it("changes straight to Restricted or Auto without a confirmation or closing the panel", async () => {
    const onChange = vi.fn();
    const closePanel = vi.fn();
    picker("auto", onChange, { closePanel });
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Restricted");
    expect(onChange).toHaveBeenCalledWith("restricted");
    expect(closePanel).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks before Full, with Cancel focused, and keeps the level on Cancel", async () => {
    const onChange = vi.fn();
    picker("auto", onChange);
    const select = screen.getByRole("combobox", { name: "Permission level" });
    await userEvent.selectOptions(select, "Full");
    const dialog = await screen.findByRole("dialog", { name: "Run this chat at Full?" });
    expect(onChange).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus());
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onChange).not.toHaveBeenCalled();
    expect(dialog).not.toBeInTheDocument();
    expect(select).toHaveValue("auto");
  });

  it("closes the panel before the Full confirmation opens, so the dialog can return focus to the chip", async () => {
    const closePanel = vi.fn();
    picker("auto", vi.fn(), { closePanel });
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    await screen.findByRole("dialog", { name: "Run this chat at Full?" });
    expect(closePanel).toHaveBeenCalledTimes(1);
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

  it("names only the agent a message names in the Full confirmation", async () => {
    picker("auto", vi.fn(), { agent: "codex" });
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("~/.codex");
    expect(dialog).not.toHaveTextContent("Copilot can run any shell command");
    expect(dialog).toHaveTextContent("Automatic review at Auto is not a guarantee either.");
  });

  it("names every agent that could answer when the daemon routes the message", async () => {
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

describe("permission level notes below the composer and in the picker panel", () => {
  it("shows no level indicator and no one-click switch: the level is chosen in the model picker", () => {
    render(<PermissionLevelNotes olderDaemon={false} lastTurn={null} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByText(/automatic review/i)).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("shows the older-daemon notice when the daemon predates permission levels", () => {
    render(<PermissionLevelNotes olderDaemon lastTurn={null} />);
    expect(screen.getByRole("status")).toHaveTextContent(OLDER_DAEMON_NOTICE);
  });

  it("reports the level the daemon recorded for the last turn", () => {
    render(<LastTurnNote lastTurn={{ daemonLevel: "auto", sentLevel: "auto" }} />);
    expect(screen.getByText("The daemon reports the last reply ran at Auto.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("keeps the panel note quiet for a mismatch, which the composer alert reports", () => {
    render(<LastTurnNote lastTurn={{ daemonLevel: "restricted", sentLevel: "auto" }} />);
    expect(screen.queryByText(/ran at/)).toBeNull();
  });

  it("shows an error when the daemon reports a different level from the one sent", () => {
    render(<PermissionLevelNotes olderDaemon={false} lastTurn={{ daemonLevel: "restricted", sentLevel: "auto" }} />);
    expect(screen.getByRole("alert")).toHaveTextContent("ran at Restricted, not Auto.");
  });

  it("shows no permission note for an automatic update, and never shows the mismatch alert for it", () => {
    render(<PermissionLevelNotes olderDaemon={false} lastTurn={{ daemonLevel: "restricted", sentLevel: "auto", trigger: "wake" }} />);
    expect(screen.queryByText(/ran at/)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("keeps the reply wording for a turn the user sent", () => {
    render(<LastTurnNote lastTurn={{ daemonLevel: "auto", sentLevel: "auto", trigger: "user" }} />);
    expect(screen.getByText("The daemon reports the last reply ran at Auto.")).toBeInTheDocument();
  });

  it("says so when the daemon recorded no level for the last turn", () => {
    render(<LastTurnNote lastTurn={{ daemonLevel: null, sentLevel: "auto" }} />);
    expect(screen.getByText("The daemon has not recorded a permission level for the last turn.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
