/**
 * GitHub Copilot sign-in check (DPES-1105). Sign-in is learned from real runs by default. This control
 * is the one explicit path that spends a call: the cost is shown first, the user presses the button, and
 * the daemon caches the answer for the session. The daemon starts the run in the background, so this
 * control polls the zero-cost status read until the run settles. The state is shown as text, never as a control.
 */
import { useEffect, useRef, useState } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { CopilotSignInRecord, CopilotSignInState } from "../../../daemon/types";
import type { SurfaceResult } from "../../../sources/surfaces/types";
import { Button } from "../../theme";
import { ErrorNotice } from "../../widgets";

export type CopilotSignInCheckFn = (force: boolean) => Promise<SurfaceResult<CopilotSignInRecord>>;

/** Polls: the status read is free, so this is only about how quickly the answer appears. */
const POLL_INTERVAL_MS = 2000;
/** After this many polls the control stops and says the check is still running. */
const MAX_POLLS = 150;

/** Decided by the user (DPES-1105): hooks are the user's own, documented here and not blocked. */
const HOOKS_NOTE = "Copilot runs your own Copilot hooks from ~/.copilot";

const COST_NOTE =
  "Checking makes one billable Copilot call on claude-haiku-4.5 (documented as 0.33 premium requests). A settled ready answer is kept for the session. After a failed or missing sign-in, pressing again makes a new call. Sign-in is also confirmed by any successful Copilot run.";

/** The states a new check can change. Pressing the button again after one of these makes a new call. */
const RECHECK_STATES = new Set<CopilotSignInState>(["not_signed_in", "no_seat", "error"]);

function stateText(record: CopilotSignInRecord): string {
  switch (record.state) {
    case "ready":
      return "Ready: GitHub Copilot is signed in and this account has a Copilot seat.";
    case "not_signed_in":
      return record.reason ?? "Not signed in to GitHub Copilot. Run `copilot login`, then retry.";
    case "no_seat":
      return record.reason ?? "This GitHub account has no GitHub Copilot subscription.";
    case "error":
      return `Copilot did not confirm sign-in: ${record.reason ?? "the CLI reported an error"}.`;
    case "checking":
      return "Checking GitHub Copilot sign-in now. This takes a few seconds.";
    case "not_checked":
      return "Not confirmed yet: sign-in is checked on your first Copilot use. If you have not signed in, run `copilot login`.";
  }
}

function usageText(record: CopilotSignInRecord): string {
  const parts: string[] = [];
  if (record.premium_requests !== null) parts.push(`${record.premium_requests} premium requests`);
  if (record.ai_credits !== null) parts.push(`${record.ai_credits} AI credits`);
  return parts.length > 0 ? `Reported by Copilot: ${parts.join(", ")}.` : "Usage: not reported by Copilot.";
}

export function CopilotSignInCheck({
  check,
  status,
  known = null,
  onRecord,
  pollIntervalMs = POLL_INTERVAL_MS,
}: {
  check: CopilotSignInCheckFn;
  /** Zero-cost status read. Without it a run that is still checking is shown as checking, not polled. */
  status?: () => Promise<SurfaceResult<CopilotSignInRecord>>;
  /** The sign-in state the picker last read, so a press after a failed sign-in makes a new check. */
  known?: CopilotSignInRecord | null;
  /** Told each settled record, so the model picker can show the new sign-in state at once. */
  onRecord?: (record: CopilotSignInRecord) => void;
  pollIntervalMs?: number;
}) {
  const [result, setResult] = useState<SurfaceResult<CopilotSignInRecord> | null>(null);
  const [busy, setBusy] = useState(false);
  // A check that threw (not a refused result) is shown here, so the press never ends in silence.
  const [thrown, setThrown] = useState<DaemonError | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const run = async () => {
    setBusy(true);
    setThrown(null);
    try {
      const last = result?.status === "ok" ? result.data : known;
      let current = await check(last !== null && RECHECK_STATES.has(last.state));
      for (let polls = 0; polls < MAX_POLLS && alive.current; polls += 1) {
        if (current.status !== "ok" || current.data.state !== "checking" || status === undefined) break;
        setResult(current);
        await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
        if (!alive.current) return;
        current = await status();
      }
      if (alive.current) setResult(current);
      if (alive.current && current.status === "ok") onRecord?.(current.data);
    } catch (error) {
      if (alive.current) setThrown(asDaemonError(error));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  return (
    <div className="ws-field" data-testid="copilot-sign-in">
      <p className="ws-small">{HOOKS_NOTE}</p>
      <p className="ws-small">{COST_NOTE}</p>
      <Button onClick={() => void run()} pending={busy}>
        Check Copilot sign-in
      </Button>
      {result !== null && result.status === "ok" && (
        <>
          <p role="status" className="ws-small">
            {stateText(result.data)}
          </p>
          {result.data.warning !== null && (
            <p role="status" className="ws-small" data-testid="copilot-hook-warning">
              {`Warning: ${result.data.warning}`}
            </p>
          )}
          {result.data.state !== "checking" && <p className="ws-small">{usageText(result.data)}</p>}
        </>
      )}
      {result !== null && result.status !== "ok" && (
        <p role="alert" className="ws-small">
          {result.reason}
        </p>
      )}
      {thrown !== null && <ErrorNotice error={thrown} action="Checking Copilot sign-in" />}
    </div>
  );
}
