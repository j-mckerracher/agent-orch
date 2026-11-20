// "Hold until…" for one pending decision (DPES-1105 Phase 4d). The date input only narrows the picker to today through
// today plus 60 days by the local date; the daemon checks the range with its own clock and its answer is the one shown.
import { useId, useMemo, useState } from "react";
import { Button } from "../../theme";
import { MAX_HOLD_DAYS, holdRange } from "./holdDates";
import { holdBusyKey, type HoldsModel } from "./useHolds";

/** The one element that gives the reason a hold control is off; the queue renders it once. */
export const HOLD_REASON_ID = "rd-hold-reason";

/**
 * The hold form for one task. Its result is `holds.action`, rendered by the view that holds the control: a successful hold
 * moves the row to Held, so the form cannot carry the result itself.
 */
export function HoldControl({ taskId, holds, onStart }: { taskId: string; holds: HoldsModel; onStart: () => void }) {
  const [open, setOpen] = useState(false);
  const [day, setDay] = useState("");
  const inputId = useId();
  const hintId = useId();
  // Read once per opening, so the picker does not move while a person is choosing.
  const range = useMemo(() => (open ? holdRange(new Date()) : null), [open]);
  const pending = holds.busy === holdBusyKey("approval", taskId);
  const off = holds.blocked !== null || (holds.busy !== null && !pending);
  const reason = holds.blocked !== null ? HOLD_REASON_ID : undefined;

  if (!open || range === null) {
    return (
      <Button
        variant="quiet"
        className="rd-row-action"
        aria-label={`Hold until… decisions for task ${taskId}`}
        aria-describedby={reason}
        disabled={off}
        onClick={() => setOpen(true)}
      >
        Hold until…
      </Button>
    );
  }
  return (
    <form
      className="rd-hold-form"
      onSubmit={(event) => {
        event.preventDefault();
        onStart();
        void holds.hold("approval", taskId, day, `task ${taskId}`).then((held) => {
          if (held) setOpen(false);
        });
      }}
    >
      <label htmlFor={inputId}>Hold until</label>
      <input
        id={inputId}
        type="date"
        required
        min={range.min}
        max={range.max}
        value={day}
        aria-describedby={hintId}
        onChange={(event) => setDay(event.target.value)}
      />
      <span id={hintId} className="rd-reason">
        Pick a day from today to {MAX_HOLD_DAYS} days from today. Every pending decision of this task returns to Actionable on that
        day. Holding does not approve or reject anything.
      </span>
      <Button type="submit" variant="primary" pending={pending} disabled={off || day === ""} aria-describedby={reason}>
        Hold decision
      </Button>
      <Button variant="quiet" disabled={holds.busy !== null} onClick={() => setOpen(false)}>
        Cancel
      </Button>
    </form>
  );
}
