/**
 * Live D17 adapter (DPES-1105): `settings.get` and `settings.set`, the machine load limits and the
 * gate they feed.
 *
 * Rules (shared with `derived.ts`, which wires this in):
 * - Never imports fixtures. A failed read is `unavailable`, or `unsupported` when the daemon has no
 *   `settings` capability; it is never answered with fixture data.
 * - A meter the gate cannot read is unknown, never zero.
 * - `settings.set` is a user-only method: the daemon refuses it unless this session proved the approver
 *   secret. The call carries an idempotency key that is reused only while the previous outcome is unknown.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { LoadLimitsWire, LoadMeterWire, SettingsWire } from "../../daemon/types";
import { liveRead } from "./liveSupport";
import type { LoadLimits, LoadReading, Provenance, SettingsSource, SettingsSnapshot } from "./types";

export const SETTINGS_METHODS = ["settings.get", "settings.set"] as const;

function readingOf(wire: LoadMeterWire | undefined): LoadReading {
  if (wire?.state === "mean") return { state: "mean", meanPercent: wire.mean_percent };
  if (wire?.state === "off") return { state: "off" };
  // A meter the daemon did not list, or could not read, is unknown.
  return { state: "unknown", reason: wire?.state === "unknown" && wire.reason === "warming_up" ? "warming_up" : "unavailable" };
}

export function mapSettings(wire: SettingsWire): SettingsSnapshot {
  const meters = new Map(wire.gate.meters.map((meter) => [meter.meter, meter]));
  const { hold } = wire.gate;
  return {
    limits: { cpu: wire.load.cpu_percent, memory: wire.load.memory_percent, disk: wire.load.disk_percent },
    gate: {
      state: wire.gate.state,
      hold: hold
        ? { meter: hold.meter, meanPercent: hold.mean_percent, limitPercent: hold.limit_percent, reopenPercent: hold.reopen_percent }
        : null,
      readings: { cpu: readingOf(meters.get("cpu")), memory: readingOf(meters.get("memory")), disk: readingOf(meters.get("disk")) },
    },
    feed: wire.feed ?? "sampler",
  };
}

function patchOf(patch: Partial<LoadLimits>): Partial<LoadLimitsWire> {
  return {
    ...(patch.cpu === undefined ? {} : { cpu_percent: patch.cpu }),
    ...(patch.memory === undefined ? {} : { memory_percent: patch.memory }),
    ...(patch.disk === undefined ? {} : { disk_percent: patch.disk }),
  };
}

export function createLiveSettings(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): SettingsSource {
  return {
    provenance,
    get: () => liveRead(provenance, now, async () => mapSettings(await connection.api().settingsGet())),
    async set(patch) {
      const scope = "settings:set";
      const key = keys.keyFor(scope, JSON.stringify(patch));
      try {
        const wire = await connection.api().settingsSet(patchOf(patch), key);
        keys.settle(scope, null);
        return { simulated: false, provenance, snapshot: mapSettings(wire) };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };
}
