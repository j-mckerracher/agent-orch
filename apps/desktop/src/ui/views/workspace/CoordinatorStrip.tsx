import { useId, useState, type FormEvent } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { CoordinatorState, ScopeReceipt } from "../../../sources/surfaces/types";
import { Button } from "../../theme";
import { ErrorNotice, Notice } from "../../widgets";
import {
  COORDINATOR_HEADING,
  NO_SCOPE_TEXT,
  REPORTS_HEADING,
  REQUESTS_HEADING,
  countsText,
  noteText,
  openRequestsOf,
  reportLabel,
  reportsOf,
  requestLabel,
  scopeError,
  scopeToSend,
  stateFrom,
} from "./coordinatorModel";

const INVALID_SCOPE_TEXT = "The daemon refused this scope. It must be one line of at most 300 bytes, with no hidden characters.";

export interface CoordinatorStripProps {
  snapshot: { coordinator?: CoordinatorState } | null;
  /** Why the scope cannot be changed now; null when it can. */
  lockReason: string | null;
  /** Absent when the surface cannot change the scope: the sentence is shown without an editor. */
  onSaveScope?: (scopeSentence: string | null) => Promise<ScopeReceipt>;
  /** Called after a save the daemon confirmed, so the conversation is read again. */
  onSaved?: () => void;
}

/**
 * A project coordinator's scope, the work it owns and its newest reports, above the composer (DPES-1108). Nothing
 * when the daemon sends no coordinator state. Notes are agent text: rendered as text, never as markdown.
 */
export function CoordinatorStrip({ snapshot, lockReason, onSaveScope, onSaved }: CoordinatorStripProps) {
  const ids = useId();
  const [typed, setTyped] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<{ notice: string } | { error: DaemonError } | null>(null);
  const state = stateFrom(snapshot);
  if (state === null) return null;

  const requests = openRequestsOf(state);
  const reports = reportsOf(state);
  const draft = typed ?? state.scopeSentence ?? "";
  const invalid = scopeError(draft);
  const reason = lockReason ?? invalid;

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!onSaveScope || reason !== null || saving) return;
    setSaving(true);
    setResult(null);
    try {
      const receipt = await onSaveScope(scopeToSend(draft));
      setResult({ notice: receipt.notice });
      if (!receipt.simulated) {
        setTyped(null);
        onSaved?.();
      }
    } catch (error) {
      setResult({ error: asDaemonError(error) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-label={COORDINATOR_HEADING} className="ws-coordinator">
      <h3>{COORDINATOR_HEADING}</h3>
      <p>Scope: {state.scopeSentence ?? NO_SCOPE_TEXT}</p>
      <p className="ws-small">{countsText(state)}</p>
      {requests.length > 0 && (
        <>
          <h4>{REQUESTS_HEADING}</h4>
          <ul aria-label={REQUESTS_HEADING}>
            {requests.map((request) => (
              <li key={request.id}>{requestLabel(request)}</li>
            ))}
          </ul>
        </>
      )}
      {reports.length > 0 && (
        <>
          <h4>{REPORTS_HEADING}</h4>
          <ul aria-label={REPORTS_HEADING}>
            {reports.map((report) => {
              const note = noteText(report);
              return (
                <li key={report.id}>
                  {reportLabel(report)}
                  {note !== null && <span className="ws-coordinator-note">{note}</span>}
                </li>
              );
            })}
          </ul>
        </>
      )}
      {onSaveScope && (
        <form className="ws-coordinator-scope" onSubmit={(event) => void save(event)} noValidate>
          <label>
            Scope sentence
            <input
              value={draft}
              disabled={lockReason !== null}
              aria-invalid={invalid !== null}
              aria-describedby={reason !== null ? `${ids}-reason` : `${ids}-hint`}
              onChange={(event) => setTyped(event.target.value)}
            />
          </label>
          <Button
            type="submit"
            variant="secondary"
            disabled={reason !== null}
            pending={saving}
            aria-describedby={reason !== null ? `${ids}-reason` : undefined}
          >
            Save scope
          </Button>
          {reason !== null ? (
            <p id={`${ids}-reason`} className="ws-small">
              {reason}
            </p>
          ) : (
            <p id={`${ids}-hint`} className="ws-small">
              One line. The global orchestrator routes work here by it. Leave it empty to clear it.
            </p>
          )}
          <div role="status" aria-live="polite">
            {result !== null && "notice" in result && <p className="ws-small">{result.notice}</p>}
            {result !== null && "error" in result &&
              (result.error.kind === "invalid-params" ? (
                <Notice tone="bad" role="alert">{INVALID_SCOPE_TEXT}</Notice>
              ) : (
                <ErrorNotice error={result.error} action="Saving the scope" />
              ))}
          </div>
        </form>
      )}
    </section>
  );
}
