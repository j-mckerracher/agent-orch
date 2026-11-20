import { useId, useState, type FormEvent } from "react";
import { Button } from "../../theme";

/**
 * Submit guard for a settings form whose Save button is never disabled. Call the hook before any early return;
 * `guard(reason, run)` then gives the form's `onSubmit`. A blocked save (not connected, no approver, an invalid
 * field) does nothing but flag `tried`, so the reason is read as an alert instead of leaving a dead button.
 * An unchanged form still saves.
 */
export function useSaveGuard() {
  const [tried, setTried] = useState(false);
  const guard = (reason: string | null, run: () => void) => (event: FormEvent) => {
    event.preventDefault();
    setTried(reason !== null);
    if (reason === null) run();
  };
  return { tried, guard };
}

/** The Save button of a settings form, large and always enabled, with the reason a save is blocked. */
export function SaveButton({
  label,
  reason,
  saving,
  tried,
  reasonId,
}: {
  label: string;
  reason: string | null;
  saving: boolean;
  tried: boolean;
  /** The id other fields already point at, when the form has one. */
  reasonId?: string;
}) {
  const ownId = useId();
  const id = reasonId ?? ownId;
  return (
    <>
      <Button type="submit" variant="primary" className="ui-btn-save" pending={saving} aria-describedby={reason === null ? undefined : id}>
        {label}
      </Button>
      {reason !== null && (
        <p id={id} role={tried ? "alert" : undefined} className={tried ? "settings-error" : "settings-reason"}>
          {reason}
        </p>
      )}
    </>
  );
}
