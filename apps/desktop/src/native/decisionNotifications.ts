import { invoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { insideTauri } from "./tauriConnector";

export type NotificationPermission = "granted" | "denied" | "prompt" | "unavailable";

export interface DecisionNotificationSnapshot {
  enabled: boolean;
  reminder_interval_minutes: number | null;
  permission: NotificationPermission;
  pending_count: number | null;
  error: string | null;
}

export interface DecisionNotificationSettings {
  enabled: boolean;
  reminder_interval_minutes: number | null;
  request_permission: boolean;
}

export interface DecisionNotificationsClient {
  get(): Promise<DecisionNotificationSnapshot>;
  set(settings: DecisionNotificationSettings): Promise<DecisionNotificationSnapshot>;
  listen(
    onSnapshot: (snapshot: DecisionNotificationSnapshot) => void,
    onError: () => void,
  ): Promise<() => void>;
}

const EVENT_NAME = "decision-notifications-status";
const PREVIEW_SNAPSHOT: DecisionNotificationSnapshot = {
  enabled: false,
  reminder_interval_minutes: null,
  permission: "unavailable",
  pending_count: null,
  error: "Native notifications are unavailable in this preview.",
};

function validInterval(value: unknown): value is number | null {
  return value === null || (typeof value === "number" && Number.isInteger(value) && value >= 5 && value <= 1440);
}

function validSettings(value: unknown): value is DecisionNotificationSettings {
  return (
    typeof value === "object" &&
    value !== null &&
    "enabled" in value &&
    typeof value.enabled === "boolean" &&
    "reminder_interval_minutes" in value &&
    validInterval(value.reminder_interval_minutes) &&
    "request_permission" in value &&
    typeof value.request_permission === "boolean"
  );
}

function parseSnapshot(value: unknown): DecisionNotificationSnapshot {
  if (
    typeof value !== "object" ||
    value === null ||
    !("enabled" in value) ||
    typeof value.enabled !== "boolean" ||
    !("reminder_interval_minutes" in value) ||
    !validInterval(value.reminder_interval_minutes) ||
    !("permission" in value) ||
    !["granted", "denied", "prompt", "unavailable"].includes(String(value.permission)) ||
    !("pending_count" in value) ||
    !(value.pending_count === null || (typeof value.pending_count === "number" && Number.isInteger(value.pending_count) && value.pending_count >= 0)) ||
    !("error" in value) ||
    !(value.error === null || (typeof value.error === "string" && value.error.length <= 240))
  ) {
    throw new Error("Native notifications returned invalid status.");
  }
  const permission = value.permission;
  if (permission !== "granted" && permission !== "denied" && permission !== "prompt" && permission !== "unavailable") {
    throw new Error("Native notifications returned invalid status.");
  }
  return {
    enabled: value.enabled,
    reminder_interval_minutes: value.reminder_interval_minutes,
    permission,
    pending_count: value.pending_count,
    error: value.error,
  };
}

function isNativeHost(): boolean {
  return typeof window !== "undefined" && insideTauri(window);
}

export function createDecisionNotificationsClient(): DecisionNotificationsClient {
  return {
    async get() {
      if (!isNativeHost()) return PREVIEW_SNAPSHOT;
      try {
        return parseSnapshot(await invoke<unknown>("decision_notifications_get"));
      } catch {
        throw new Error("Notification state could not be loaded.");
      }
    },
    async set(settings) {
      if (!validSettings(settings)) throw new Error("Notification settings are invalid.");
      if (!isNativeHost()) return PREVIEW_SNAPSHOT;
      try {
        return parseSnapshot(await invoke<unknown>("decision_notifications_set", { settings }));
      } catch {
        throw new Error("Notification settings could not be saved.");
      }
    },
    async listen(onSnapshot, onError) {
      if (!isNativeHost()) return () => undefined;
      try {
        return await tauriListen<unknown>(EVENT_NAME, ({ payload }) => {
          try {
            onSnapshot(parseSnapshot(payload));
          } catch {
            onError();
          }
        });
      } catch {
        throw new Error("Notification status updates could not be subscribed.");
      }
    },
  };
}

export const decisionNotificationsClient = createDecisionNotificationsClient();
