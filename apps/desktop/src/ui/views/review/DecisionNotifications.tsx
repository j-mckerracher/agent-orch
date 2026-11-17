import { useEffect, useRef, useState } from "react";
import {
  decisionNotificationsClient,
  type DecisionNoticeAction,
  type DecisionNotificationSnapshot,
  type DecisionNotificationsClient,
} from "../../../native/decisionNotifications";
import { Button } from "../../theme";

const INTERVALS = [60, 120, 240] as const;
/** Reminders for an unanswered decision start at 4 hours and repeat every 4 hours (owner decision 72334). */
const DEFAULT_INTERVAL = 240;
/** How long "Defer reminders" postpones them. */
const DEFER_MINUTES = 60;
type IntervalChoice = "off" | "60" | "120" | "240" | "custom";

function choiceFor(interval: number | null): IntervalChoice {
  if (interval === null) return "off";
  if (interval === 60) return "60";
  if (interval === 120) return "120";
  if (interval === 240) return "240";
  return "custom";
}

export function DecisionNotifications({ client = decisionNotificationsClient }: { client?: DecisionNotificationsClient }) {
  const [snapshot, setSnapshot] = useState<DecisionNotificationSnapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [choice, setChoice] = useState<IntervalChoice>(String(DEFAULT_INTERVAL) as IntervalChoice);
  const [customInterval, setCustomInterval] = useState(String(DEFAULT_INTERVAL));
  const revision = useRef(0);
  const lifecycle = useRef(0);
  const draftInterval = useRef<number | null | undefined>(undefined);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    lifecycle.current += 1;
    setSnapshot(null);
    setLoadError(null);
    const onSnapshot = (next: DecisionNotificationSnapshot) => {
      if (!active) return;
      revision.current += 1;
      setSnapshot(next);
      setLoadError(null);
      setOperationError(null);
    };
    const onListenError = () => {
      if (!active) return;
      revision.current += 1;
      setSnapshot(null);
      setLoadError("Notification status updates are unavailable.");
    };

    void client
      .listen(onSnapshot, onListenError)
      .then((stop) => {
        if (!active) {
          stop();
          return;
        }
        unlisten = stop;
        const requestRevision = ++revision.current;
        return client
          .get()
          .then((initial) => {
            if (active && revision.current === requestRevision) {
              setSnapshot(initial);
              setLoadError(null);
            }
          })
          .catch(() => {
            if (active && revision.current === requestRevision) {
              setSnapshot(null);
              setLoadError("Notification state could not be loaded.");
            }
          });
      })
      .catch(() => {
        if (active) setLoadError("Notification status updates are unavailable.");
      });

    return () => {
      active = false;
      revision.current += 1;
      lifecycle.current += 1;
      unlisten?.();
    };
  }, [client]);

  useEffect(() => {
    if (!snapshot || draftInterval.current === snapshot.reminder_interval_minutes) return;
    draftInterval.current = snapshot.reminder_interval_minutes;
    setChoice(choiceFor(snapshot.reminder_interval_minutes));
    if (snapshot.reminder_interval_minutes !== null) setCustomInterval(String(snapshot.reminder_interval_minutes));
  }, [snapshot]);

  async function save(enabled: boolean, interval: number | null, requestPermission: boolean) {
    if (!snapshot || busy) return;
    setBusy(true);
    setOperationError(null);
    const requestRevision = ++revision.current;
    const operationLifecycle = lifecycle.current;
    try {
      const next = await client.set({
        enabled,
        reminder_interval_minutes: interval,
        request_permission: requestPermission,
      });
      if (lifecycle.current === operationLifecycle && revision.current === requestRevision) {
        setSnapshot(next);
        setLoadError(null);
      }
    } catch {
      if (lifecycle.current === operationLifecycle && revision.current === requestRevision) {
        setOperationError("Notification settings could not be saved.");
      }
    } finally {
      if (lifecycle.current === operationLifecycle) setBusy(false);
    }
  }

  async function applyNotice(action: DecisionNoticeAction) {
    if (!snapshot || busy) return;
    setBusy(true);
    setOperationError(null);
    const requestRevision = ++revision.current;
    const operationLifecycle = lifecycle.current;
    try {
      const next = await client.notice(action);
      if (lifecycle.current === operationLifecycle && revision.current === requestRevision) {
        setSnapshot(next);
        setLoadError(null);
      }
    } catch {
      if (lifecycle.current === operationLifecycle && revision.current === requestRevision) {
        setOperationError("Reminder action could not be applied.");
      }
    } finally {
      if (lifecycle.current === operationLifecycle) setBusy(false);
    }
  }

  function saveInterval() {
    if (!snapshot) return;
    let interval: number | null;
    if (choice === "off") interval = null;
    else if (choice === "custom") {
      const parsed = Number(customInterval);
      if (!Number.isInteger(parsed) || parsed < 5 || parsed > 1440) {
        setOperationError("Enter a whole number from 5 to 1440 minutes.");
        return;
      }
      interval = parsed;
    } else interval = Number(choice);
    void save(snapshot.enabled, interval, false);
  }

  const unavailable = snapshot?.permission === "unavailable";
  const retryPermission = snapshot?.permission === "denied";

  return (
    <section className="rd-notifications" aria-label="Decision notifications">
      <h2>Decision notifications</h2>
      {loadError && <p role="alert">Notifications: unknown · Permission: unknown · Decisions needing attention: unknown. {loadError}</p>}
      {!snapshot && !loadError && <p role="status">Loading notification status… Decisions needing attention: unknown.</p>}
      {snapshot && (
        <>
          <p role="status">
            Notifications: {unavailable ? "unavailable" : snapshot.enabled ? "enabled" : "disabled"} · Permission: {snapshot.permission} · Decisions needing attention: {snapshot.pending_count ?? "unknown"}
          </p>
          {snapshot.error && <p role="alert">{snapshot.error}</p>}
          {operationError && <p role="alert">{operationError}</p>}
          {!unavailable && (
            <div className="rd-notification-controls">
              <label>
                Reminder interval
                <select
                  value={choice}
                  disabled={busy}
                  onChange={(event) => {
                    const value = event.currentTarget.value;
                    if (value === "off" || value === "15" || value === "30" || value === "60" || value === "custom") setChoice(value);
                  }}
                >
                  <option value="off">Off</option>
                  {INTERVALS.map((minutes) => <option key={minutes} value={minutes}>{minutes} minutes</option>)}
                  <option value="custom">Custom</option>
                </select>
              </label>
              {choice === "custom" && (
                <label>
                  Custom reminder interval (5–1440 minutes)
                  <input
                    type="number"
                    min={5}
                    max={1440}
                    step={1}
                    value={customInterval}
                    disabled={busy}
                    onChange={(event) => setCustomInterval(event.target.value)}
                  />
                </label>
              )}
              <Button onClick={saveInterval} disabled={busy}>
                Save reminder interval
              </Button>
              {!snapshot.enabled || snapshot.permission !== "granted" ? (
                <Button onClick={() => void save(true, snapshot.reminder_interval_minutes, true)} disabled={busy}>
                  {retryPermission ? "Retry permission" : "Enable notifications"}
                </Button>
              ) : (
                <Button onClick={() => void save(false, snapshot.reminder_interval_minutes, false)} disabled={busy}>
                  Disable notifications
                </Button>
              )}
            </div>
          )}
          {!unavailable && snapshot.pending_count !== null && snapshot.pending_count > 0 && (
            <div className="rd-notification-controls" role="group" aria-label="Local reminder actions">
              <Button onClick={() => void applyNotice({ action: "acknowledge" })} disabled={busy}>
                Acknowledge reminders
              </Button>
              <Button onClick={() => void applyNotice({ action: "defer", minutes: DEFER_MINUTES })} disabled={busy}>
                Defer reminders {DEFER_MINUTES} minutes
              </Button>
              <span>Local only: decisions stay pending and nothing is approved or rejected.</span>
            </div>
          )}
        </>
      )}
    </section>
  );
}
