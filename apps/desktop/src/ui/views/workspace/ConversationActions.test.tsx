import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  AddCoordinatorDialog,
  ConversationActions,
  historyActionReason,
  type ConversationActionsProps,
} from "./ConversationActions";
import { DaemonError } from "../../../daemon/errors";
import { ConversationTargetPicker } from "./ConversationTargetPicker";
import { GLOBAL_TARGET, type TargetOption } from "./targets";

const coordinator: TargetOption = {
  key: "project:auth",
  target: { kind: "project", projectId: "auth" },
  label: "auth coordinator",
  scope: "auth",
  placeholder: false,
};
const global: TargetOption = {
  key: "global",
  target: GLOBAL_TARGET,
  label: "Global orchestrator",
  scope: "All workstreams",
  placeholder: false,
};

function actions(overrides: Partial<ConversationActionsProps> = {}) {
  const props: ConversationActionsProps = {
    option: coordinator,
    live: true,
    connected: true,
    supported: true,
    running: false,
    busy: false,
    onClear: vi.fn(),
    onRemove: vi.fn(),
    ...overrides,
  };
  render(<ConversationActions {...props} />);
  return props;
}

const user = () => userEvent.setup();
const trigger = () => screen.getByRole("button", { name: /Conversation actions for/ });
async function openMenu(u: ReturnType<typeof user>) {
  await u.click(trigger());
  return screen.getByRole("menu");
}

describe("historyActionReason", () => {
  const ready = { live: true, connected: true, supported: true, running: false, busy: false };

  it("is null when the daemon can run the action now", () => {
    expect(historyActionReason(ready)).toBeNull();
  });

  it("names the reason for each blocked state, in order: offline, older daemon, running, busy", () => {
    expect(historyActionReason({ ...ready, connected: false })).toMatch(/Offline/);
    expect(historyActionReason({ ...ready, supported: false })).toMatch(/older and cannot change history/);
    expect(historyActionReason({ ...ready, running: true })).toMatch(/still running/);
    expect(historyActionReason({ ...ready, busy: true })).toMatch(/Another action/);
    expect(historyActionReason({ ...ready, connected: false, running: true })).toMatch(/Offline/);
  });

  it("never disables fixture actions: they are simulated", () => {
    expect(historyActionReason({ ...ready, live: false, connected: false, supported: false })).toBeNull();
  });
});

describe("conversation actions menu", () => {
  it("offers clear for Global and clear plus remove for a project coordinator", async () => {
    const u = user();
    render(<ConversationActions option={global} live connected supported running={false} busy={false} onClear={vi.fn()} />);
    expect(within(await openMenu(u)).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Clear history…",
    ]);
  });

  it("lists remove after clear for a project coordinator", async () => {
    const u = user();
    actions();
    const items = within(await openMenu(u)).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual(["Clear history…", "Remove coordinator…"]);
  });

  it("opens and closes with the trigger and Escape, and returns focus to the trigger", async () => {
    const u = user();
    actions();
    expect(screen.getByRole("menu", { hidden: true })).not.toBeVisible();
    await openMenu(u);
    expect(screen.getByRole("menuitem", { name: "Clear history…" })).toHaveFocus();
    await u.keyboard("{Escape}");
    expect(screen.getByRole("menu", { hidden: true })).not.toBeVisible();
    expect(document.activeElement).toBe(trigger());
  });

  it("stays open when a press moves focus to the scrolling main region, as WebKit does, so the item opens its dialog", async () => {
    const u = user();
    render(
      <div tabIndex={-1} data-testid="main">
        <ConversationActions option={global} live connected supported running={false} busy={false} onClear={vi.fn()} />
      </div>,
    );
    await openMenu(u);
    const clear = screen.getByRole("menuitem", { name: "Clear history…" });
    // WebKit does not focus a pressed button; it focuses the nearest focusable ancestor instead.
    fireEvent.pointerDown(clear);
    act(() => screen.getByTestId("main").focus());
    fireEvent.pointerUp(clear);
    expect(screen.getByRole("menu")).toBeVisible();
    fireEvent.click(clear);
    expect(screen.getByRole("dialog", { name: "Clear history for Global orchestrator?" })).toBeVisible();
  });

  it("closes when focus leaves without a press, as with Tab", async () => {
    const u = user();
    render(
      <>
        <ConversationActions option={global} live connected supported running={false} busy={false} onClear={vi.fn()} />
        <button type="button">Outside</button>
      </>,
    );
    await openMenu(u);
    act(() => screen.getByRole("button", { name: "Outside" }).focus());
    expect(screen.getByRole("menu", { hidden: true })).not.toBeVisible();
  });

  it("moves between items with the arrow keys", async () => {
    const u = user();
    actions();
    await openMenu(u);
    await u.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Remove coordinator…" })).toHaveFocus();
    await u.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Clear history…" })).toHaveFocus();
    await u.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "Remove coordinator…" })).toHaveFocus();
  });

  it.each([
    ["offline", { connected: false }, /Offline: reconnect/],
    ["an older daemon", { supported: false }, /older and cannot change history/],
    ["a running turn", { running: true }, /A reply is still running/],
  ])("disables both actions with a visible reason while %s, and opens no dialog", async (_, state, reason) => {
    const u = user();
    actions(state);
    const menu = await openMenu(u);
    const clear = within(menu).getByRole("menuitem", { name: "Clear history…" });
    expect(clear).toHaveAttribute("aria-disabled", "true");
    expect(clear).toHaveAccessibleDescription(reason);
    expect(within(menu).getByRole("menuitem", { name: "Remove coordinator…" })).toHaveAttribute("aria-disabled", "true");
    expect(within(menu).getByText(reason)).toBeVisible();
    await u.click(clear);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("is enabled in fixture mode and never claims to reach the daemon", async () => {
    const u = user();
    actions({ live: false, connected: false, supported: false });
    const menu = await openMenu(u);
    expect(within(menu).getByRole("menuitem", { name: "Clear history…" })).not.toHaveAttribute("aria-disabled");
    expect(within(menu).queryByText(/Offline|older/)).toBeNull();
  });
});

describe("clear history confirmation", () => {
  it("names the conversation, puts the safe choice first, and says it cannot be undone", async () => {
    const u = user();
    actions();
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    const dialog = screen.getByRole("dialog", { name: "Clear history for auth coordinator?" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(within(dialog).getByText(/cannot be undone/)).toBeVisible();
    expect(within(dialog).getByText(/Other conversations, the project and its tasks are not changed/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "Clear history permanently" })).toHaveClass("ui-btn-danger");
  });

  it("Escape cancels without clearing and returns focus to the trigger", async () => {
    const u = user();
    const props = actions();
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    await u.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(props.onClear).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(trigger());
  });

  it("calls the clear handler once on confirmation and closes", async () => {
    const u = user();
    const props = actions();
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    await u.click(screen.getByRole("button", { name: "Clear history permanently" }));
    expect(props.onClear).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("holds the confirm button pending, and keeps the dialog open, until the clear ends", async () => {
    const u = user();
    let finish = (): void => {};
    const props = actions({
      onClear: vi.fn(() => new Promise<void>((resolve) => (finish = () => resolve()))),
    });
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    await u.click(screen.getByRole("button", { name: "Clear history permanently" }));
    expect(screen.getByRole("button", { name: "Clear history permanently" })).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    finish();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(props.onClear).toHaveBeenCalledTimes(1);
  });

  it("holds the remove confirm pending until the removal ends", async () => {
    const u = user();
    let finish = (): void => {};
    const props = actions({
      onRemove: vi.fn(() => new Promise<void>((resolve) => (finish = () => resolve()))),
    });
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Remove coordinator…" }));
    await u.click(screen.getByRole("button", { name: "Remove coordinator permanently" }));
    expect(screen.getByRole("button", { name: "Remove coordinator permanently" })).toHaveAttribute("aria-busy", "true");
    finish();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(props.onRemove).toHaveBeenCalledTimes(1);
  });

  it("keeps Tab and Shift+Tab inside the dialog", async () => {
    const u = user();
    actions();
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const destructive = screen.getByRole("button", { name: "Clear history permanently" });
    expect(cancel).toHaveFocus();
    await u.tab();
    expect(destructive).toHaveFocus();
    await u.tab();
    expect(cancel).toHaveFocus();
    await u.tab({ shift: true });
    expect(destructive).toHaveFocus();
  });

  it("labels fixture confirmations as placeholder data", async () => {
    const u = user();
    actions({ live: false });
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("PLACEHOLDER DATA");
  });
});

describe("remove coordinator confirmation", () => {
  it("states the conversation is deleted, the project, scope and preferences stay, and the coordinator can be added back", async () => {
    const u = user();
    const props = actions();
    await openMenu(u);
    await u.click(screen.getByRole("menuitem", { name: "Remove coordinator…" }));
    const dialog = screen.getByRole("dialog", { name: "Remove auth coordinator?" });
    expect(within(dialog).getByText(/permanently deletes every message and reply in its conversation/)).toBeVisible();
    expect(
      within(dialog).getByText(/its tasks, the coordinator's scope sentence and the project's preferences and learnings are kept/),
    ).toBeVisible();
    expect(within(dialog).getByText(/starts with an empty conversation/)).toBeVisible();
    expect(dialog).not.toHaveTextContent(/notes/);
    await u.click(screen.getByRole("button", { name: "Remove coordinator permanently" }));
    expect(props.onRemove).toHaveBeenCalledTimes(1);
    expect(props.onClear).not.toHaveBeenCalled();
  });
});

describe("add coordinator dialog", () => {
  it("lists the removed coordinators and adds the chosen one", async () => {
    const u = user();
    const onAdd = vi.fn();
    render(
      <AddCoordinatorDialog
        projects={[{ id: "auth", label: "auth coordinator" }]}
        busyId={null}
        onAdd={onAdd}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText(/keeps its scope sentence and the project's preferences and learnings/)).toBeVisible();
    expect(screen.queryByText(/notes/)).toBeNull();
    const list = screen.getByRole("list", { name: "Projects without a coordinator" });
    await u.click(within(list).getByRole("button", { name: "Add auth coordinator" }));
    expect(onAdd).toHaveBeenCalledWith("auth");
  });

  it("says so when every project has a coordinator, and shows progress while one is added", () => {
    const { rerender } = render(
      <AddCoordinatorDialog projects={[]} busyId={null} onAdd={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByText("Every project already has an active coordinator.")).toBeVisible();
    rerender(
      <AddCoordinatorDialog
        projects={[{ id: "auth", label: "auth coordinator" }]}
        busyId="auth"
        onAdd={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Add auth coordinator" })).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Add auth coordinator" })).toHaveTextContent("Adding…");
  });

  it("shows a failed add inside the dialog as an alert, since the dialog covers the page", () => {
    render(
      <AddCoordinatorDialog
        projects={[{ id: "auth", label: "auth coordinator" }]}
        busyId={null}
        error={new DaemonError("timeout", "the daemon did not answer")}
        onAdd={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Adding the coordinator failed.");
  });
});

describe("target picker add entry", () => {
  const options = [global, coordinator];

  it("opens the add dialog through the entry when it is available", async () => {
    const u = user();
    const onAdd = vi.fn();
    render(
      <ConversationTargetPicker
        options={options}
        selectedKey="global"
        onSelect={vi.fn()}
        addCoordinator={{ onAdd, reason: null }}
      />,
    );
    await u.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    await u.click(screen.getByRole("button", { name: "Add coordinator…" }));
    expect(onAdd).toHaveBeenCalledTimes(1);
  });

  it("shows the entry disabled with its reason and does nothing when unavailable", async () => {
    const u = user();
    const onAdd = vi.fn();
    render(
      <ConversationTargetPicker
        options={options}
        selectedKey="global"
        onSelect={vi.fn()}
        addCoordinator={{ onAdd, reason: "Offline: reconnect to the daemon to change history." }}
      />,
    );
    await u.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    const add = screen.getByRole("button", { name: "Add coordinator…" });
    expect(add).toHaveAttribute("aria-disabled", "true");
    expect(add).toHaveAccessibleDescription("Offline: reconnect to the daemon to change history.");
    await u.click(add);
    expect(onAdd).not.toHaveBeenCalled();
  });
});
