import type { Tone } from "../../theme";
import type { TaskState } from "../../../daemon/types";

/** State tone for the Pill. Every state is also written out as text, so tone is never the only cue. */
export const STATE_TONE: Record<TaskState, Tone> = {
  Created: "neutral",
  Queued: "neutral",
  Running: "info",
  Verifying: "info",
  Succeeded: "ok",
  VerificationFailed: "bad",
  Failed: "bad",
  Cancelled: "warn",
};
