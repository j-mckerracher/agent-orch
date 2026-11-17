import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  DecisionNotificationSettings,
  DecisionNotificationSnapshot,
  DecisionNotificationsClient,
} from "../../../native/decisionNotifications";
import { DecisionNotifications } from "./DecisionNotifications";

const snapshot = (overrides: Partial<DecisionNotificationSnapshot> = {}): DecisionNotificationSnapshot => ({
  enabled: false,
  reminder_interval_minutes: 15,
  permission: "prompt",
  pending_count: null,
  error: null,
  ...overrides,
});

function clientFor(initial: DecisionNotificationSnapshot) {
  let current = initial;
  let listener: ((next: DecisionNotificationSnapshot) => void) | undefined;
  const sets: DecisionNotificationSettings[] = [];
  const client: DecisionNotificationsClient = {
    get: vi.fn(async () => current),
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
  return { client, sets, emit: (next: DecisionNotificationSnapshot) => listener?.(next) };
}

describe("decision notification control", () => {
  it("requests permission only after explicit enable and shows actual permission and count", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ pending_count: 2 }));
    render(<DecisionNotifications client={fake.client} />);

    expect(await screen.findByText(/Permission: prompt/)).toHaveTextContent("Decisions needing attention: 2");
    expect(fake.client.set).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Enable notifications" }));

    await waitFor(() => expect(fake.sets).toEqual([{ enabled: true, reminder_interval_minutes: 15, request_permission: true }]));
    expect(await screen.findByText(/Notifications: enabled · Permission: granted/)).toBeInTheDocument();
  });

  it("rejects invalid interval atomically without native save", async () => {
    const user = userEvent.setup();
    const fake = clientFor(snapshot({ enabled: true, permission: "granted", pending_count: 1 }));
    render(<DecisionNotifications client={fake.client} />);
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
    render(<DecisionNotifications client={fake.client} />);

    expect(await screen.findByText(/Permission: denied/)).toHaveTextContent("Decisions needing attention: unknown");
    expect(screen.getByRole("alert")).toHaveTextContent("Permission denied.");
    await user.click(screen.getByRole("button", { name: "Retry permission" }));
    await waitFor(() => expect(fake.sets[0]).toEqual({ enabled: true, reminder_interval_minutes: 15, request_permission: true }));
  });

  it("marks browser-preview notifications unavailable without offering fake enablement", async () => {
    render(<DecisionNotifications />);

    expect(await screen.findByText(/Notifications: unavailable · Permission: unavailable/)).toHaveTextContent("Decisions needing attention: unknown");
    expect(screen.getByRole("alert")).toHaveTextContent("Native notifications are unavailable in this preview.");
    expect(screen.queryByRole("button", { name: /Enable|Retry permission/ })).not.toBeInTheDocument();
  });

  it("shows native state-load errors without turning unknown count into zero", async () => {
    const client: DecisionNotificationsClient = {
      get: vi.fn(async () => { throw new Error("private host detail"); }),
      set: vi.fn(async () => snapshot()),
      listen: vi.fn(async () => () => undefined),
    };
    render(<DecisionNotifications client={client} />);

    expect(await screen.findByText("Notifications: unknown · Permission: unknown · Decisions needing attention: unknown. Notification state could not be loaded.")).toBeInTheDocument();
    expect(screen.queryByText(/Decisions needing attention: 0/)).not.toBeInTheDocument();
    expect(client.set).not.toHaveBeenCalled();
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
      listen: vi.fn(async (onSnapshot) => {
        emit = onSnapshot;
        return stop;
      }),
    };
    const view = render(<DecisionNotifications client={client} />);
    await screen.findByText(/Permission: prompt/);
    await user.click(screen.getByRole("button", { name: "Enable notifications" }));

    act(() => emit?.(snapshot({ enabled: true, permission: "granted", pending_count: 4 })));
    await screen.findByText(/Decisions needing attention: 4/);
    await act(async () => resolveSet?.(snapshot({ enabled: true, permission: "prompt", pending_count: 1 })));
    expect(screen.getByText(/Permission: granted/)).toHaveTextContent("Decisions needing attention: 4");

    view.unmount();
    expect(stop).toHaveBeenCalledTimes(1);
  });
});
