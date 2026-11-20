import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  createDecisionNotificationsClient,
  type DecisionNoticeAction,
  type DecisionNotificationSettings,
  type DecisionNotificationSnapshot,
  type DecisionNotificationsClient,
} from "../../../native/decisionNotifications";
import { NotificationsSection } from "./NotificationsSection";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const snapshot = (overrides: Partial<DecisionNotificationSnapshot> = {}): DecisionNotificationSnapshot => ({
  enabled: false,
  reminder_interval_minutes: 240,
  permission: "prompt",
  pending_count: null,
  error: null,
  ...overrides,
});

function clientFor(initial: DecisionNotificationSnapshot) {
  let current = initial;
  let listener: ((next: DecisionNotificationSnapshot) => void) | undefined;
  const sets: DecisionNotificationSettings[] = [];
  const notices: DecisionNoticeAction[] = [];
  const client: DecisionNotificationsClient = {
    get: vi.fn(async () => current),
    notice: vi.fn(async (action) => {
      notices.push(action);
      return current;
    }),
    set: vi.fn(async (settings) => {
      sets.push(settings);
      current = snapshot({
        ...current,
        enabled: settings.enabled,
        reminder_interval_minutes: settings.reminder_interval_minutes,
        permission: settings.request_permission && current.permission === "prompt" ? "granted" : current.permission,
        error: current.permission === "denied" ? current.error : null,
      });
      listener?.(current);
      return current;
    }),
    listen: vi.fn(async (onSnapshot) => {
      listener = onSnapshot;
      return () => { listener = undefined; };
    }),
  };
  return { client, sets, notices, emit: (next: DecisionNotificationSnapshot) => listener?.(next) };
}

describe("notifications settings section", () => {
  it("requests permission only after explicit enable and shows actual permission and count", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ pending_count: 2 }));
    render(<NotificationsSection client={fake.client} />);

    expect(await screen.findByText(/Permission: prompt/)).toHaveTextContent("Decisions needing attention: 2");
    expect(fake.client.set).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Enable notifications" }));

    await waitFor(() => expect(fake.sets).toEqual([{ enabled: true, reminder_interval_minutes: 240, request_permission: true }]));
    expect(await screen.findByText(/Notifications: enabled · Permission: granted/)).toBeInTheDocument();
  });

  it("renders no disabled owner, response-time, silence or audit controls", async () => {
    render(<NotificationsSection client={clientFor(snapshot()).client} />);
    await screen.findByText(/Permission: prompt/);
    const section = screen.getByRole("heading", { name: "Decision notifications" }).closest("section")!;
    expect(within(section).queryByText(/owner|response time|silence|audit/i)).toBeNull();
    for (const control of within(section).queryAllByRole("button")) expect(control).toBeEnabled();
  });

  it("rejects invalid interval atomically without native save", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ enabled: true, permission: "granted", pending_count: 1 }));
    render(<NotificationsSection client={fake.client} />);
    await screen.findByText(/Notifications: enabled/);

    await user.selectOptions(screen.getByLabelText("Reminder interval"), "custom");
    await user.clear(screen.getByLabelText("Custom reminder interval (5–1440 minutes)"));
    await user.type(screen.getByLabelText("Custom reminder interval (5–1440 minutes)"), "4");
    await user.click(screen.getByRole("button", { name: "Save reminder interval" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Enter a whole number from 5 to 1440 minutes.");
    expect(fake.client.set).not.toHaveBeenCalled();
    expect(screen.getByText(/Decisions needing attention: 1/)).toBeInTheDocument();

    await user.clear(screen.getByLabelText("Custom reminder interval (5–1440 minutes)"));
    await user.type(screen.getByLabelText("Custom reminder interval (5–1440 minutes)"), "5");
    await user.click(screen.getByRole("button", { name: "Save reminder interval" }));
    await waitFor(() => expect(fake.sets).toEqual([{ enabled: true, reminder_interval_minutes: 5, request_permission: false }]));
  });

  it("shows denial and unknown count, and asks again only from Retry permission", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ permission: "denied", error: "Permission denied." }));
    render(<NotificationsSection client={fake.client} />);

    expect(await screen.findByText(/Permission: denied/)).toHaveTextContent("Decisions needing attention: unknown");
    expect(screen.getByRole("alert")).toHaveTextContent("Permission denied.");
    await user.click(screen.getByRole("button", { name: "Retry permission" }));
    await waitFor(() => expect(fake.sets[0]).toEqual({ enabled: true, reminder_interval_minutes: 240, request_permission: true }));
  });

  it("marks browser-preview notifications unavailable without offering fake enablement", async () => {
    render(<NotificationsSection />);

    expect(await screen.findByText(/Notifications: unavailable · Permission: unavailable/)).toHaveTextContent("Decisions needing attention: unknown");
    expect(screen.getByText("Native notifications are unavailable in this preview.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Enable|Retry permission/ })).not.toBeInTheDocument();
  });

  it("shows native state-load errors without turning unknown count into zero", async () => {
    const client: DecisionNotificationsClient = {
      get: vi.fn(async () => { throw new Error("private host detail"); }),
      set: vi.fn(async () => snapshot()),
      notice: vi.fn(async () => snapshot()),
      listen: vi.fn(async () => () => undefined),
    };
    render(<NotificationsSection client={client} />);

    expect(await screen.findByText("Notifications: unknown · Permission: unknown · Decisions needing attention: unknown. Notification state could not be loaded.")).toBeInTheDocument();
    expect(screen.queryByText(/Decisions needing attention: 0/)).not.toBeInTheDocument();
    expect(client.set).not.toHaveBeenCalled();
  });

  it("still loads state and controls when the live subscription is rejected", async () => {
    const fake = clientFor(snapshot({ enabled: true, permission: "granted" }));
    fake.client.listen = vi.fn(async () => { throw new Error("event.listen not allowed"); });
    render(<NotificationsSection client={fake.client} />);

    expect(await screen.findByText(/Permission: granted/)).toBeInTheDocument();
    expect(screen.getByText("Live status updates are unavailable; reopen Settings to refresh.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Disable notifications" })).toBeInTheDocument();
  });

  it("shows the load error when both the subscription and state load fail", async () => {
    const client: DecisionNotificationsClient = {
      get: vi.fn(async () => { throw new Error("private host detail"); }),
      set: vi.fn(async () => snapshot()),
      notice: vi.fn(async () => snapshot()),
      listen: vi.fn(async () => { throw new Error("event.listen not allowed"); }),
    };
    render(<NotificationsSection client={client} />);

    expect(await screen.findByText(/Notification state could not be loaded\./)).toBeInTheDocument();
  });

  it("keeps newer event state over stale command results and removes listener on unmount", async () => {
    const user = userEvent.setup();
    let resolveSet: ((value: DecisionNotificationSnapshot) => void) | undefined;
    let emit: ((next: DecisionNotificationSnapshot) => void) | undefined;
    const stop = vi.fn();
    const client: DecisionNotificationsClient = {
      get: vi.fn(async () => snapshot()),
      set: vi.fn(() => {
        return new Promise<DecisionNotificationSnapshot>((resolve) => {
          resolveSet = resolve;
        });
      }),
      notice: vi.fn(async () => snapshot()),
      listen: vi.fn(async (onSnapshot) => {
        emit = onSnapshot;
        return stop;
      }),
    };
    const view = render(<NotificationsSection client={client} />);
    await screen.findByText(/Permission: prompt/);
    await user.click(screen.getByRole("button", { name: "Enable notifications" }));

    act(() => emit?.(snapshot({ enabled: true, permission: "granted", pending_count: 4 })));
    await screen.findByText(/Decisions needing attention: 4/);
    await act(async () => resolveSet?.(snapshot({ enabled: true, permission: "prompt", pending_count: 1 })));
    expect(screen.getByText(/Permission: granted/)).toHaveTextContent("Decisions needing attention: 4");

    view.unmount();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("defaults the reminder interval to 4 hours", async () => {
    const fake = clientFor(snapshot());
    render(<NotificationsSection client={fake.client} />);

    await screen.findByText(/Permission: prompt/);
    expect(screen.getByLabelText("Reminder interval")).toHaveValue("240");
    expect(screen.getByRole("option", { name: "240 minutes" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "15 minutes" })).not.toBeInTheDocument();
  });

  it("acknowledges and defers the notice locally: only the local notice action is called, never a decision", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ enabled: true, permission: "granted", pending_count: 2 }));
    render(<NotificationsSection client={fake.client} />);
    await screen.findByText(/Decisions needing attention: 2/);

    await user.click(screen.getByRole("button", { name: "Acknowledge reminders" }));
    await user.click(screen.getByRole("button", { name: "Defer reminders 60 minutes" }));

    await waitFor(() => expect(fake.notices).toEqual([{ action: "acknowledge" }, { action: "defer", minutes: 60 }]));
    expect(screen.getByText(/nothing is approved or rejected/)).toBeInTheDocument();
    // The component can reach only this client; settings are untouched and the decisions stay pending.
    expect(fake.client.set).not.toHaveBeenCalled();
    expect(Object.keys(fake.client).sort()).toEqual(["get", "listen", "notice", "set"]);
    expect(screen.getByText(/Decisions needing attention: 2/)).toBeInTheDocument();
  });

  it("offers local actions only when decisions are pending", async () => {
    const fake = clientFor(snapshot({ enabled: true, permission: "granted", pending_count: 0 }));
    const view = render(<NotificationsSection client={fake.client} />);
    await screen.findByText(/Decisions needing attention: 0/);
    expect(screen.queryByRole("button", { name: /Acknowledge reminders|Defer reminders/ })).not.toBeInTheDocument();

    act(() => fake.emit(snapshot({ enabled: true, permission: "granted", pending_count: null })));
    await screen.findByText(/Decisions needing attention: unknown/);
    expect(screen.queryByRole("button", { name: /Acknowledge reminders|Defer reminders/ })).not.toBeInTheDocument();
    view.unmount();
  });

  it("shows a failed local action without changing the pending count", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ enabled: true, permission: "granted", pending_count: 3 }));
    vi.mocked(fake.client.notice).mockRejectedValueOnce(new Error("private host detail"));
    render(<NotificationsSection client={fake.client} />);
    await screen.findByText(/Decisions needing attention: 3/);

    await user.click(screen.getByRole("button", { name: "Acknowledge reminders" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Reminder action could not be applied.");
    expect(screen.getByText(/Decisions needing attention: 3/)).toBeInTheDocument();
  });
});

describe("native decision notification client", () => {
  it("sends the local notice action as one notice command and rejects invalid actions before the host", async () => {
    invoke.mockClear();
    invoke.mockResolvedValue({
      enabled: true,
      reminder_interval_minutes: 240,
      permission: "granted",
      pending_count: 1,
      error: null,
    });
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    try {
      const client = createDecisionNotificationsClient();
      await client.notice({ action: "acknowledge" });
      await client.notice({ action: "defer", minutes: 60 });
      expect(invoke.mock.calls).toEqual([
        ["decision_notifications_notice", { notice: { action: "acknowledge", minutes: null } }],
        ["decision_notifications_notice", { notice: { action: "defer", minutes: 60 } }],
      ]);

      invoke.mockClear();
      for (const invalid of [
        { action: "defer", minutes: 4 },
        { action: "defer", minutes: 1441 },
        { action: "defer" },
        { action: "acknowledge", minutes: 60 },
        { action: "approve" },
        { action: "reject" },
      ]) {
        await expect(client.notice(invalid as never)).rejects.toThrow("Notification action is invalid.");
      }
      expect(invoke).not.toHaveBeenCalled();
    } finally {
      Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    }
  });
});
