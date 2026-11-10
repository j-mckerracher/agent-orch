/**
 * Read helpers for the M7-24 live adapters (`conversationLive`, `modelCatalogLive`).
 * Same mapping as the guarded reads in `derived.ts`, kept here so those adapters
 * do not import `derived.ts` back.
 */
import { asDaemonError, presentError } from "../../daemon/errors";
import { notOk, observation, okResult } from "./result";
import type { Provenance, SurfaceResult } from "./types";

/** A failed live call is `unsupported` when the daemon lacks the method, else `unavailable`. */
export function liveFailure<T>(provenance: Provenance, error: unknown, override?: string): SurfaceResult<T> {
  const failed = asDaemonError(error);
  const reason = override ?? `${presentError(failed).reason} (${failed.kind}: ${failed.message})`;
  if (failed.kind === "method-unavailable" || failed.kind === "capability-unavailable") {
    return notOk(provenance, "unsupported", reason);
  }
  return notOk(provenance, "unavailable", reason);
}

export async function liveRead<T>(
  provenance: Provenance,
  now: () => number,
  read: () => Promise<T>,
  describe?: (error: unknown) => string | undefined,
): Promise<SurfaceResult<T>> {
  try {
    return okResult(provenance, await read(), observation(now()));
  } catch (error) {
    return liveFailure(provenance, error, describe?.(error));
  }
}
