/**
 * Preferences and learnings (DPES-1108 phase 6): `memory.list`, `memory.set`, `memory.remove` and `memory.restore`.
 *
 * Rules:
 * - the live source never imports fixtures. A failed read is `unavailable`, `unsupported` when the daemon has
 *   no `standing_approvals` capability or no memory methods; it never answers fixture data.
 * - the three changes are user-only. Each idempotency key is reused only while the previous outcome is unknown.
 * - the fixture source never calls the daemon and never saves anything. Its receipts say so.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { MemoryListWire } from "../../daemon/types";
import type { SurfaceFixtureScenario } from "./fixtures";
import { liveRead } from "./liveSupport";
import { notOk, observation, okResult } from "./result";
import type { MemoryEntry, MemoryKind, MemoryReceipt, MemorySnapshot, MemorySource, Provenance } from "./types";

export const MEMORY_METHODS = ["memory.list", "memory.set", "memory.remove", "memory.restore"] as const;

export const FIXTURE_MEMORY_NOTICE = "Fixture data: nothing was saved.";

export function mapMemory(wire: MemoryListWire): MemorySnapshot {
  return {
    kind: wire.kind,
    scope: wire.scope,
    entries: wire.entries.map((entry) => ({
      key: entry.key,
      text: entry.text,
      writtenBy: entry.written_by,
      createdAtMs: entry.created_at_ms,
      confirmedAtMs: entry.confirmed_at_ms,
      stale: entry.stale,
    })),
    bytes: wire.bytes,
    budget: wire.budget,
    archived: wire.archived.map((row) => ({
      id: row.id,
      key: row.key,
      text: row.text,
      writtenBy: row.written_by,
      archivedAtMs: row.archived_at_ms,
      reason: row.reason,
      restored: row.restored,
    })),
    maxKeyBytes: wire.limits.max_key_bytes,
    maxTextBytes: wire.limits.max_text_bytes,
    staleAfterDays: wire.limits.stale_after_days,
  };
}

export function createLiveMemory(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): MemorySource {
  /** Sends one change under its tracked key; the key survives only an answer that leaves the outcome unknown. */
  const change = async <T>(tracked: string, fingerprint: string, send: (key: string) => Promise<T>): Promise<MemoryReceipt<T>> => {
    const key = keys.keyFor(tracked, fingerprint);
    try {
      const result = await send(key);
      keys.settle(tracked, null);
      return { simulated: false, provenance, notice: null, result };
    } catch (error) {
      keys.settle(tracked, asDaemonError(error));
      throw error;
    }
  };
  return {
    provenance,
    list: (kind, scope) => liveRead(provenance, now, async () => mapMemory(await connection.api().memoryList(kind, scope))),
    set: (kind, scope, key, text) =>
      change(`memory:set:${kind}:${scope}:${key}`, text, async (idempotencyKey) => {
        const wire = await connection.api().memorySet(kind, scope, key, text, idempotencyKey);
        return { outcome: wire.outcome, bytes: wire.bytes, budget: wire.budget };
      }),
    remove: (kind, scope, key) =>
      change(`memory:remove:${kind}:${scope}:${key}`, "", async (idempotencyKey) => {
        const wire = await connection.api().memoryRemove(kind, scope, key, idempotencyKey);
        return { removed: wire.removed };
      }),
    restore: (archiveId) =>
      change(`memory:restore:${archiveId}`, "", async (idempotencyKey) => {
        const wire = await connection.api().memoryRestore(archiveId, idempotencyKey);
        return { outcome: wire.outcome, bytes: wire.bytes ?? null, budget: wire.budget ?? null };
      }),
  };
}

const DAY_MS = 86_400_000;

/** The core's budgets and limits; the daemon-less fixture source mirrors them. */
const FIXTURE_LIMITS = { maxKeyBytes: 40, maxTextBytes: 280 };
const fixtureBudget = (kind: MemoryKind) => (kind === "learning" ? 1024 : 2048);

function fixtureEntries(kind: MemoryKind, scope: string, now: number): MemoryEntry[] {
  const entry = (key: string, text: string, writtenBy: MemoryEntry["writtenBy"], ageDays: number, stale = false): MemoryEntry => ({
    key,
    text,
    writtenBy,
    createdAtMs: now - (ageDays + 10) * DAY_MS,
    confirmedAtMs: now - ageDays * DAY_MS,
    stale,
  });
  if (kind === "learning") {
    return [
      entry("ci-runner", "The CI runner needs the VPN for the internal package feed.", "orchestrator", 3),
      entry("release-freeze", "Releases freeze on Fridays after noon.", "orchestrator", 200, true),
    ];
  }
  if (scope === "global") {
    return [
      entry("small-prs", "Prefer small pull requests.", "user", 1),
      entry("tests-first", "Write a failing test before a fix.", "orchestrator", 20),
    ];
  }
  return [entry("review-style", "Ask before changing the public API.", "user", 5)];
}

export function createFixtureMemory(scenario: SurfaceFixtureScenario, now: () => number): MemorySource {
  const provenance: Provenance = { kind: "fixture", id: "D17", fixture: "memory" };
  const simulated = <T>(): Promise<MemoryReceipt<T>> => Promise.resolve({ simulated: true, provenance, notice: FIXTURE_MEMORY_NOTICE });
  return {
    provenance,
    list(kind, scope) {
      if (scenario === "loading") return new Promise(() => undefined);
      if (scenario === "unavailable" || scenario === "unsupported" || scenario === "unknown") {
        return Promise.resolve(notOk(provenance, scenario, `Fixture scenario: preferences are ${scenario}.`, observation(now())));
      }
      const entries = scenario === "empty" ? [] : fixtureEntries(kind, scope, now());
      const snapshot: MemorySnapshot = {
        kind,
        scope,
        entries,
        bytes: entries.reduce((sum, item) => sum + new TextEncoder().encode(item.key + item.text).length, 0),
        budget: fixtureBudget(kind),
        archived:
          scenario === "empty"
            ? []
            : [{ id: 1, key: "old-branch", text: "Branch from develop.", writtenBy: "user", archivedAtMs: now() - 2 * DAY_MS, reason: "removed", restored: false }],
        ...FIXTURE_LIMITS,
        staleAfterDays: kind === "learning" ? 180 : null,
      };
      return Promise.resolve(okResult(provenance, snapshot, observation(now())));
    },
    set: () => simulated(),
    remove: () => simulated(),
    restore: () => simulated(),
  };
}
