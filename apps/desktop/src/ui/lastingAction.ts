/**
 * For a destructive or irreversible action (cancel, archive, discard, force start, approve, reject): the result stays
 * beside the control until the next run or clear(), where useAction would clear a success after CLEAR_MS. Failures
 * already stay. Thin wrapper over useAction, so the same ActionStatus renders it; move into action.tsx as an option.
 */
import { useEffect, useState } from "react";
import { type Action, type Outcome, useAction } from "./action";

export function useLastingAction(): Action {
  const action = useAction();
  const { state, clear } = action;
  const [held, setHeld] = useState<Outcome | null>(null);

  useEffect(() => {
    if (state.phase === "done") setHeld(state.outcome);
  }, [state]);

  // ponytail: keep the last result visible once useAction's timer has returned it to idle.
  const shown: Action["state"] = state.phase === "idle" && held ? { phase: "done", outcome: held } : state;
  return { ...action, state: shown, clear: () => { setHeld(null); clear(); } };
}
