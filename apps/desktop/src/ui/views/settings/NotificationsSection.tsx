import { useEffect, useId, useRef, useState } from "react";
import {
  decisionNotificationsClient,
  type DecisionNoticeAction,
  type DecisionNotificationSnapshot,
  type DecisionNotificationsClient,
} from "../../../native/decisionNotifications";
import { ActionStatus, useAction, type Action, type Outcome } from "../../action";
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

const RANGE_TEXT = "Enter a whole number from 5 to 1440 minutes.";
const BLOCKED_TEXT = "Notifications are blocked for Motif. Allow them in System Settings → Notifications, then try again.";

function every(minutes: number): string {
  if (minutes % 60 !== 0) return `every ${minutes} minutes`;
  const hours = minutes / 60;
  return hours === 1 ? "every hour" : `every ${hours} hours`;
}
const describe = (minutes: number | null) => (minutes === null ? "off" : every(minutes));

/** The interval the form holds, null for Off, or undefined when the custom value is not 5–1440 whole minutes. */
function draftOf(choice: IntervalChoice, custom: string): number | null | undefined {
  if (choice === "off") return null;
  if (choice !== "custom") return Number(choice);
  const minutes = Number(custom);
  return Number.isInteger(minutes) && minutes >= 5 && minutes <= 1440 ? minutes : undefined;
}

/** What a finished Enable says. The host answers with the permission it actually got. */
function enableOutcome(next: DecisionNotificationSnapshot): Outcome {
  if (next.enabled && next.permission === "granted") return { kind: "saved", text: "Notifications enabled." };
  if (next.permission === "denied") return { kind: "failed", text: BLOCKED_TEXT };
  if (next.permission === "unavailable") return { kind: "failed", text: "Notifications are not available on this computer." };
  return { kind: "failed", text: "The system has not allowed notifications yet. Answer its prompt, then try again." };
}

export function NotificationsSection({ client = decisionNotificationsClient }: { client?: DecisionNotificationsClient }) {
  const [snapshot, setSnapshot] = useState<DecisionNotificationSnapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [tried, setTried] = useState(false);
  const [reminder, setReminder] = useState<"acknowledge" | "defer">("acknowledge");
  const [liveUpdatesUnavailable, setLiveUpdatesUnavailable] = useState(false);
  const interval = useAction();
  const permission = useAction();
  const reminders = useAction();
  const busy = interval.pending || permission.pending || reminders.pending;
  const customRef = useRef<HTMLInputElement>(null);
  const hintId = useId();
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
    setLiveUpdatesUnavailable(false);
    const onSnapshot = (next: DecisionNotificationSnapshot) => {
      if (!active) return;
      revision.current += 1;
      setSnapshot(next);
      setLoadError(null);
    };
    const onListenError = () => {
      if (!active) return;
      revision.current += 1;
      setSnapshot(null);
      setLoadError("Notification status updates are unavailable.");
    };

    // A failed subscription only costs live updates; the controls still work from get().
    void client
      .listen(onSnapshot, onListenError)
      .then(
        (stop) => {
          if (active) unlisten = stop;
          else stop();
        },
        () => {
          if (active) setLiveUpdatesUnavailable(true);
        },
      )
      .then(() => {
        if (!active) return;
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
      });

    return () => {
      active = false;
      revision.current += 1;
      lifecycle.current += 1;
      unlisten?.();
    };
  }, [client, attempt]);

  useEffect(() => {
    if (!snapshot || draftInterval.current === snapshot.reminder_interval_minutes) return;
    draftInterval.current = snapshot.reminder_interval_minutes;
    setChoice(choiceFor(snapshot.reminder_interval_minutes));
    if (snapshot.reminder_interval_minutes !== null) setCustomInterval(String(snapshot.reminder_interval_minutes));
  }, [snapshot]);

  /** Sends one command and keeps its snapshot unless a newer one arrived or the section was reset meanwhile. */
  async function send(command: () => Promise<DecisionNotificationSnapshot>) {
    const requestRevision = ++revision.current;
    const operationLifecycle = lifecycle.current;
    const next = await command();
    if (lifecycle.current === operationLifecycle && revision.current === requestRevision) {
      setSnapshot(next);
      setLoadError(null);
    }
    return next;
  }

  const draft = draftOf(choice, customInterval);
  const invalid = draft === undefined;
  const saved = snapshot?.reminder_interval_minutes ?? null;

  function saveInterval() {
    if (!snapshot || busy) return;
    setTried(true);
    if (draft === undefined) {
      customRef.current?.focus();
      return;
    }
    const enabled = snapshot.enabled;
    void interval.run("Saving…", async () => {
      if (draft === saved) return { kind: "saved", text: `Already set to ${describe(draft)}.` };
      await send(() => client.set({ enabled, reminder_interval_minutes: draft, request_permission: false }));
      return { kind: "saved", text: draft === null ? "Saved. Reminders are off." : `Saved. Reminders repeat ${every(draft)}.` };
    });
  }

  function toggle(enable: boolean) {
    if (!snapshot || busy) return;
    const minutes = snapshot.reminder_interval_minutes;
    void permission.run(enable ? "Enabling…" : "Disabling…", async () => {
      const next = await send(() => client.set({ enabled: enable, reminder_interval_minutes: minutes, request_permission: enable }));
      return enable ? enableOutcome(next) : { kind: "saved", text: "Notifications disabled. Your reminder interval is kept." };
    });
  }

  function notice(action: DecisionNoticeAction) {
    if (!snapshot || busy) return;
    setReminder(action.action);
    const defer = action.action === "defer";
    void reminders.run(defer ? "Deferring…" : "Acknowledging…", async () => {
      await send(() => client.notice(action));
      return { kind: "saved", text: defer ? `Reminders deferred for ${DEFER_MINUTES} minutes.` : "Reminders silenced for the decisions pending now." };
    });
  }

  /** Every button but the one that is running is disabled; the running one shows its own spinner. */
  const locked = (own: Action, active = true) => busy && !(own.pending && active);

  const unavailable = snapshot?.permission === "unavailable";
  const retryPermission = snapshot?.permission === "denied";

  return (
    <section className="settings-section settings-notifications" aria-label="Decision notifications">
      <h3>Decision notifications</h3>
      {loadError && (
        <>
          <p role="alert">Notifications: unknown · Permission: unknown · Decisions needing attention: unknown. {loadError}</p>
          <Button onClick={() => setAttempt((count) => count + 1)}>Retry</Button>
        </>
      )}
      {!snapshot && !loadError && <p role="status">Loading notification status… Decisions needing attention: unknown.</p>}
      {snapshot && (
        <>
          <p role="status">
            Notifications: {unavailable ? "unavailable" : snapshot.enabled ? "enabled" : "disabled"} · Permission: {snapshot.permission} · Decisions needing attention: {snapshot.pending_count ?? "unknown"}
          </p>
          {liveUpdatesUnavailable && <p>Live status updates are unavailable; reopen Settings to refresh.</p>}
          {snapshot.error &&<p role={unavailable ? undefined : "alert"}>{snapshot.error}</p>}
          {unavailable && <p>Reminders cannot be changed while notifications are unavailable. Reopen Settings to check again.</p>}
          {retryPermission && !permission.pending && <p>Notifications are blocked for Motif. Allow them in System Settings → Notifications, then choose Retry permission.</p>}
          {!unavailable && (
            <div className="settings-notification-controls">
              <label>
                Reminder interval
                <select
                  value={choice}
                  disabled={busy}
                  onChange={(event) => {
                    const value = event.currentTarget.value;
                    if (value === "off" || value === "60" || value === "120" || value === "240" || value === "custom") {
                      setChoice(value);
                      interval.clear();
                    }
                  }}
                >
                  <option value="off">Off</option>
                  {INTERVALS.map((minutes) => <option key={minutes} value={minutes}>{minutes} minutes</option>)}
                  <option value="custom">Custom</option>
                </select>
              </label>
              {choice === "custom" && (
                <div className="settings-notification-field">
                  <label>
                    Custom reminder interval (5–1440 minutes)
                    <input
                      ref={customRef}
                      type="number"
                      min={5}
                      max={1440}
                      step={1}
                      value={customInterval}
                      disabled={busy}
                      aria-invalid={invalid}
                      aria-describedby={hintId}
                      onChange={(event) => {
                        setCustomInterval(event.target.value);
                        setTried(false);
                        interval.clear();
                      }}
                    />
                  </label>
                  {/* Outside the label, so the field's name stays "Custom reminder interval (5–1440 minutes)". */}
                  <small id={hintId} role={invalid && tried ? "alert" : undefined} className={invalid ? "settings-error" : "settings-hint"}>
                    {RANGE_TEXT}
                  </small>
                </div>
              )}
              <Button onClick={saveInterval} pending={interval.pending} disabled={locked(interval)}>
                Save reminder interval
              </Button>
              {!snapshot.enabled || snapshot.permission !== "granted" ? (
                <Button onClick={() => toggle(true)} pending={permission.pending} disabled={locked(permission)}>
                  {retryPermission ? "Retry permission" : "Enable notifications"}
                </Button>
              ) : (
                <Button onClick={() => toggle(false)} pending={permission.pending} disabled={locked(permission)}>
                  Disable notifications
                </Button>
              )}
            </div>
          )}
          {!unavailable && (
            <p className="settings-hint">{invalid || draft !== saved ? `Not saved yet. Saved: ${describe(saved)}.` : `Saved: ${describe(saved)}.`}</p>
          )}
          {/* Outside the conditionals above: a live snapshot that hides a control must not take its result with it. */}
          <ActionStatus action={interval} />
          <ActionStatus action={permission} />
          {!unavailable && snapshot.pending_count !== null && snapshot.pending_count > 0 && (
            <div className="settings-notification-controls" role="group" aria-label="Local reminder actions">
              <Button onClick={() => notice({ action: "acknowledge" })} pending={reminders.pending && reminder === "acknowledge"} disabled={locked(reminders, reminder === "acknowledge")}>
                Acknowledge reminders
              </Button>
              <Button onClick={() => notice({ action: "defer", minutes: DEFER_MINUTES })} pending={reminders.pending && reminder === "defer"} disabled={locked(reminders, reminder === "defer")}>
                Defer reminders {DEFER_MINUTES} minutes
              </Button>
              <span>Local only: decisions stay pending and nothing is approved or rejected.</span>
            </div>
          )}
          <ActionStatus action={reminders} />
        </>
      )}
    </section>
  );
}
