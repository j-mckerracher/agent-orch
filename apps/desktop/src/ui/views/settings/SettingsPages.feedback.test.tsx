import { cleanup, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DecisionNotificationSnapshot, DecisionNotificationsClient } from "../../../native/decisionNotifications";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { renderSettingsPage } from "../../../test/settingsPage";
import { createServices } from "../../createServices";

afterEach(cleanup);

/**
 * The guard for "every action button gives feedback" (DPES-1170, docs/contributing.md). Each row is a button on a
 * Settings page that does something when clicked. After a click with a succeeding client a status text must appear;
 * with a failing client an alert must. A button a page gains later and this table does not list fails the coverage
 * test, so it cannot ship silent.
 */
const snapshot = (overrides: Partial<DecisionNotificationSnapshot> = {}): DecisionNotificationSnapshot => ({
  enabled: true,
  reminder_interval_minutes: 240,
  permission: "granted",
  pending_count: 2,
  error: null,
  ...overrides,
});

function stub(fail: boolean): DecisionNotificationsClient {
  const answer = async () => {
    if (fail) throw new Error("Unable to save notification settings.");
    return snapshot();
  };
  return { get: vi.fn(async () => snapshot()), set: vi.fn(answer), notice: vi.fn(answer), listen: vi.fn(async () => () => undefined) };
}

const services = (notifications: DecisionNotificationsClient) => createServices({ surfaces: createFixtureSurfaces(), notifications });

/** page, button name, what to do first (the interval save needs a changed value to reach the host), the success text. */
const BUTTONS = [
  { page: "notifications", button: "Save reminder interval", prepare: { label: "Reminder interval", value: "120" }, saved: "Saved. Reminders repeat every 2 hours." },
  { page: "notifications", button: "Disable notifications", saved: "Notifications disabled. Your reminder interval is kept." },
  { page: "notifications", button: "Acknowledge reminders", saved: "Reminders silenced for the decisions pending now." },
  { page: "notifications", button: "Defer reminders 60 minutes", saved: "Reminders deferred for 60 minutes." },
] as const;

/** Buttons that only navigate or change something visible at once; they need no feedback of their own. */
const NO_FEEDBACK_NEEDED: Record<string, string[]> = { notifications: [] };

async function openPage(page: string, fail: boolean) {
  const user = userEvent.setup();
  renderSettingsPage(services(stub(fail)), page);
  await screen.findByText(/Permission: granted/);
  return user;
}

describe("Settings buttons give feedback", () => {
  for (const row of BUTTONS) {
    it(`${row.page}: "${row.button}" says what happened when it works`, async () => {
      const user = await openPage(row.page, false);
      if ("prepare" in row) await user.selectOptions(screen.getByLabelText(row.prepare.label), row.prepare.value);
      await user.click(screen.getByRole("button", { name: row.button }));
      expect(await screen.findByText(row.saved)).toBeInTheDocument();
    });

    it(`${row.page}: "${row.button}" says why and offers Retry when it fails`, async () => {
      const user = await openPage(row.page, true);
      if ("prepare" in row) await user.selectOptions(screen.getByLabelText(row.prepare.label), row.prepare.value);
      await user.click(screen.getByRole("button", { name: row.button }));
      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(/Unable to save notification (settings|history)\./);
      expect(within(alert).getByRole("button", { name: "Retry" })).toBeInTheDocument();
    });
  }

  it("lists every button on the Notifications page", async () => {
    await openPage("notifications", false);
    const page = screen.getByRole("heading", { level: 2, name: "Notifications" }).closest("section, div, main") ?? document.body;
    const known = new Set<string>([...BUTTONS.map((row) => row.button), ...NO_FEEDBACK_NEEDED.notifications]);
    const shown = within(page as HTMLElement)
      .getAllByRole("button")
      .map((button) => button.textContent?.trim() ?? "")
      .filter((name) => !["Agents", "Limits", "Autonomy", "Repositories", "Memory", "Notifications", "Appearance"].includes(name));
    expect(shown.filter((name) => !known.has(name))).toEqual([]);
  });
});
