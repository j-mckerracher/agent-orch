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
import type { LoadLimitsWire, LoadMeterWire, SettingsPatchWire, SettingsWire } from "../../daemon/types";
import { liveRead } from "./liveSupport";
import type { GroupsPatch, LoadLimits, LoadReading, Provenance, SettingsSource, SettingsSnapshot } from "./types";

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
    // An older daemon sends neither group; the page then says it cannot change them.
    ...(wire.supervision
      ? {
          supervision: {
            baseIntervalSecs: wire.supervision.base_interval_secs,
            minIntervalSecs: wire.supervision.min_interval_secs,
            attemptWallMinutes: wire.supervision.attempt_wall_minutes,
          },
        }
      : {}),
    ...(wire.wake_limits
      ? {
          wakeLimits: {
            hourlyCap: wire.wake_limits.hourly_cap,
            dailyCap: wire.wake_limits.daily_cap,
            dailyTokenBudget: wire.wake_limits.daily_token_budget,
          },
        }
      : {}),
    ...(wire.chat_pr_delivery === undefined ? {} : { chatPrDelivery: wire.chat_pr_delivery }),
    ...(wire.autonomous_actions === undefined ? {} : { autonomousActions: wire.autonomous_actions }),
    ...(wire.mcp === undefined ? {} : { mcpEnabled: wire.mcp.enabled }),
    ...(wire.home_directory === undefined ? {} : { homeDirectory: wire.home_directory }),
    ...(wire.dispatch_defaults === undefined ? {} : {
      dispatchDefaults: { defaultAgent: wire.dispatch_defaults.default_agent, providers: wire.dispatch_defaults.providers },
    }),
  };
}

function patchOf(patch: Partial<LoadLimits>): Partial<LoadLimitsWire> {
  return {
    ...(patch.cpu === undefined ? {} : { cpu_percent: patch.cpu }),
    ...(patch.memory === undefined ? {} : { memory_percent: patch.memory }),
    ...(patch.disk === undefined ? {} : { disk_percent: patch.disk }),
  };
}

/** Only the fields present are sent, and a group with no field is left out: the daemon refuses an empty group. */
export function groupsPatchOf(
  patch: GroupsPatch,
): Pick<SettingsPatchWire, "supervision" | "wake_limits" | "chat_pr_delivery" | "autonomous_actions" | "mcp" | "home_directory" | "dispatch_defaults"> {
  const { supervision: s, wakeLimits: w } = patch;
  const supervision = {
    ...(s?.baseIntervalSecs === undefined ? {} : { base_interval_secs: s.baseIntervalSecs }),
    ...(s?.minIntervalSecs === undefined ? {} : { min_interval_secs: s.minIntervalSecs }),
    ...(s?.attemptWallMinutes === undefined ? {} : { attempt_wall_minutes: s.attemptWallMinutes }),
  };
  // `null` is kept: it clears a cap. `undefined` is dropped: it leaves the cap alone.
  const wake_limits = {
    ...(w?.hourlyCap === undefined ? {} : { hourly_cap: w.hourlyCap }),
    ...(w?.dailyCap === undefined ? {} : { daily_cap: w.dailyCap }),
    ...(w?.dailyTokenBudget === undefined ? {} : { daily_token_budget: w.dailyTokenBudget }),
  };
  return {
    ...(Object.keys(supervision).length === 0 ? {} : { supervision }),
    ...(Object.keys(wake_limits).length === 0 ? {} : { wake_limits }),
    ...(patch.chatPrDelivery === undefined ? {} : { chat_pr_delivery: patch.chatPrDelivery }),
    ...(patch.autonomousActions === undefined ? {} : { autonomous_actions: patch.autonomousActions }),
    ...(patch.mcpEnabled === undefined ? {} : { mcp: { enabled: patch.mcpEnabled } }),
    // `null` is kept: it clears the folder.
    ...(patch.homeDirectory === undefined ? {} : { home_directory: patch.homeDirectory }),
    ...(patch.dispatchDefaults === undefined ? {} : {
      dispatch_defaults: { default_agent: patch.dispatchDefaults.defaultAgent, providers: patch.dispatchDefaults.providers },
    }),
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
    async setGroups(patch) {
      const scope = "settings:set-groups";
      const key = keys.keyFor(scope, JSON.stringify(patch));
      try {
        const wire = await connection.api().settingsSetGroups(groupsPatchOf(patch), key);
        keys.settle(scope, null);
        return { simulated: false, provenance, snapshot: mapSettings(wire) };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };
}
