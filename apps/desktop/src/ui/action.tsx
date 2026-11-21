/**
 * The one feedback pattern for a button that does something (DPES-1170; Nielsen H1, H4, H9).
 * idle → pending → saved | fixture | failed, shown beside the control:
 *
 *   const action = useAction();
 *   <Button pending={action.pending} onClick={() => void action.run("Saving…", save)}>Save</Button>
 *   <ActionStatus action={action} />
 *
 * `save` resolves with what to tell the person (an Outcome) and may throw: a thrown error is shown as the failure.
 * Pending stays on screen at least MIN_PENDING_MS so a fast save still registers; a success clears itself after
 * CLEAR_MS; a failure stays, with Retry, until the next run or `clear()` (call it when the person edits the control).
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "./theme";
import { Notice } from "./widgets";

/** What one action tells the person when it ends. */
export interface Outcome {
  kind: "saved" | "fixture" | "failed";
  text: string;
}

type ActionState = { phase: "idle" } | { phase: "pending"; text: string } | { phase: "done"; outcome: Outcome };

export const MIN_PENDING_MS = 400;
export const CLEAR_MS = 6000;
const IDLE: ActionState = { phase: "idle" };

export function useAction() {
  const [state, setState] = useState<ActionState>(IDLE);
  const running = useRef(false);
  const last = useRef<{ text: string; fn: () => Promise<Outcome> } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
    };
  }, []);

  const clear = useCallback(() => {
    if (running.current) return;
    clearTimeout(timer.current);
    setState(IDLE);
  }, []);

  const run = useCallback(async (text: string, fn: () => Promise<Outcome>) => {
    if (running.current) return;
    running.current = true;
    last.current = { text, fn };
    clearTimeout(timer.current);
    setState({ phase: "pending", text });
    const started = Date.now();
    let outcome: Outcome;
    try {
      outcome = await fn();
    } catch (error) {
      outcome = { kind: "failed", text: error instanceof Error && error.message ? error.message : "That did not work. Try again." };
    }
    const wait = MIN_PENDING_MS - (Date.now() - started);
    if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
    running.current = false;
    if (!mounted.current) return;
    setState({ phase: "done", outcome });
    if (outcome.kind !== "failed") timer.current = setTimeout(() => setState(IDLE), CLEAR_MS);
  }, []);

  /** Runs the last action again with the values it had. */
  const retry = useCallback(() => {
    if (last.current) void run(last.current.text, last.current.fn);
  }, [run]);

  return { state, pending: state.phase === "pending", run, retry, clear };
}

export type Action = ReturnType<typeof useAction>;

/**
 * The result of an action, right beside its control. The polite status region is always mounted, so a result that
 * appears is announced; a failure is an alert with Retry. The glyph and the words carry the meaning, not the color.
 */
export function ActionStatus({ action, className }: { action: Action; className?: string }) {
  const { state } = action;
  const failed = state.phase === "done" && state.outcome.kind === "failed" ? state.outcome : null;
  return (
    <div className={`ui-action-status${className ? ` ${className}` : ""}`}>
      <div role="status">
        {state.phase === "pending" && <span className="ui-action-pending">{state.text}</span>}
        {state.phase === "done" && !failed && <Notice tone={state.outcome.kind === "fixture" ? "info" : "ok"}>{state.outcome.text}</Notice>}
      </div>
      {failed && (
        <Notice tone="bad" role="alert">
          {failed.text}{" "}
          <Button variant="quiet" onClick={action.retry}>
            Retry
          </Button>
        </Notice>
      )}
    </div>
  );
}
