/**
 * Helpers shared by the D9-D16 adapters. Imports only types, so fixture
 * adapters can use it without reaching the daemon client.
 */
import type { TaskState } from "../../daemon/types";
import type { Measured, Observation, Provenance, SurfaceResult } from "./types";

export const TASK_STATE_ORDER: readonly TaskState[] = [
  "Created",
  "Queued",
  "Running",
  "Verifying",
  "Succeeded",
  "VerificationFailed",
  "Failed",
  "Cancelled",
];

export function observation(nowMs: number, ageMs = 0): Observation {
  return { observedAt: new Date(nowMs - ageMs).toISOString(), ageMs };
}

export function okResult<T>(provenance: Provenance, data: T, obs: Observation): SurfaceResult<T> {
  return { status: "ok", data, provenance, ...obs };
}

export function notOk<T>(
  provenance: Provenance,
  status: "unavailable" | "unsupported" | "unknown",
  reason: string,
): SurfaceResult<T> {
  return { status, reason, provenance };
}

export function measured<T>(value: T, obs: Observation): Measured<T> {
  return { status: "ok", value, ...obs };
}

export function unsupportedValue<T>(reason: string): Measured<T> {
  return { status: "unsupported", reason };
}

export function countStates(states: readonly TaskState[]): Record<TaskState, number> {
  const counts = Object.fromEntries(TASK_STATE_ORDER.map((state) => [state, 0])) as Record<TaskState, number>;
  for (const state of states) counts[state] += 1;
  return counts;
}

/** The oldest observation among results: the age of a value composed from several reads. */
export function oldest(observations: readonly Observation[]): Observation | null {
  let result: Observation | null = null;
  for (const item of observations) if (!result || item.ageMs > result.ageMs) result = item;
  return result;
}
