/**
 * Decision holds (DPES-1105 Phase 4d): `holds.list`, `holds.set` and `holds.release`.
 *
 * Rules:
 * - The live source never imports fixtures. A failed read is `unavailable`, and `unsupported` when the daemon has
 *   no `holds` capability; it is never answered with fixture data.
 * - `holds.set` and `holds.release` are user-only: the daemon refuses unless this session proved the approver secret.
 *   Each call carries an idempotency key that is reused only while the previous outcome is unknown.
 * - The fixture source never calls a daemon. It keeps its holds in memory for the session and says so.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { HoldWire } from "../../daemon/types";
import type { SurfaceFixtureScenario } from "./fixtures";
import { liveRead } from "./liveSupport";
import { notOk, observation, okResult } from "./result";
import type { HoldItem, HoldReceipt, HoldSubjectKind, HoldsSource, Provenance } from "./types";

export const HOLDS_METHODS = ["holds.list", "holds.set", "holds.release"] as const;

export const FIXTURE_HOLD_NOTICE = "Fixture data: nothing was changed.";

export function mapHold(wire: HoldWire): HoldItem {
  return { id: wire.id, subjectKind: wire.subject_kind, subjectId: wire.subject_id, untilDate: wire.until_date, state: wire.state };
}

/** The task ids whose approvals are held, and the proposal ids that are held. */
export interface HeldSets {
  tasks: ReadonlySet<string>;
  proposals: ReadonlySet<string>;
}

export const NO_HELD: HeldSets = { tasks: new Set(), proposals: new Set() };

export function heldSets(holds: readonly HoldItem[]): HeldSets {
  const active = holds.filter((hold) => hold.state === "active");
  return {
    tasks: new Set(active.filter((hold) => hold.subjectKind === "approval").map((hold) => hold.subjectId)),
    proposals: new Set(active.filter((hold) => hold.subjectKind === "proposal").map((hold) => hold.subjectId)),
  };
}

export function createLiveHolds(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): HoldsSource {
  /** One user-only write: the key survives only an outcome the app cannot know. */
  async function write(scope: string, fingerprint: string, send: (key: string) => Promise<{ hold: HoldWire }>): Promise<HoldReceipt> {
    const key = keys.keyFor(scope, fingerprint);
    try {
      const result = await send(key);
      keys.settle(scope, null);
      return { simulated: false, provenance, notice: null, hold: mapHold(result.hold) };
    } catch (error) {
      keys.settle(scope, asDaemonError(error));
      throw error;
    }
  }
  return {
    provenance,
    list: () => liveRead(provenance, now, async () => (await connection.api().holdsList()).holds.map(mapHold)),
    set: (subjectKind, subjectId, until) =>
      write(`holds:set:${subjectKind}:${subjectId}`, until, (key) => connection.api().holdsSet(subjectKind, subjectId, until, key)),
    release: (holdId) => write(`holds:release:${holdId}`, "release", (key) => connection.api().holdsRelease(holdId, key)),
  };
}

/** A held decision in the fixture queue: the approval of the docs task, held until a day in early January. */
const FIXTURE_HOLD: HoldItem = {
  id: "fixture:hold-1",
  subjectKind: "approval",
  subjectId: "fixture:task-docs-1",
  untilDate: "2026-01-09",
  state: "active",
};

export function createFixtureHolds(scenario: SurfaceFixtureScenario, now: () => number): HoldsSource {
  const provenance: Provenance = { kind: "fixture", id: "D16", fixture: "holds" };
  let held: HoldItem[] = [FIXTURE_HOLD];
  let counter = 0;
  const receipt = (hold: HoldItem): HoldReceipt => ({ simulated: true, provenance, notice: FIXTURE_HOLD_NOTICE, hold });
  return {
    provenance,
    list() {
      if (scenario === "loading") return new Promise(() => undefined);
      if (scenario === "unavailable" || scenario === "unsupported" || scenario === "unknown") {
        return Promise.resolve(notOk(provenance, scenario, `Fixture scenario: holds are ${scenario}.`, observation(now())));
      }
      return Promise.resolve(okResult(provenance, scenario === "empty" ? [] : held.map((hold) => ({ ...hold })), observation(now())));
    },
    set(subjectKind: HoldSubjectKind, subjectId, until) {
      counter += 1;
      const hold: HoldItem = { id: `fixture:hold-new-${counter}`, subjectKind, subjectId, untilDate: until, state: "active" };
      held = [...held.filter((item) => !(item.subjectKind === subjectKind && item.subjectId === subjectId)), hold];
      return Promise.resolve(receipt(hold));
    },
    release(holdId) {
      const hold = held.find((item) => item.id === holdId);
      if (hold === undefined) return Promise.reject(new DaemonError("not-found", "Fixture data has no such hold."));
      held = held.filter((item) => item.id !== holdId);
      return Promise.resolve(receipt({ ...hold, state: "released" }));
    },
  };
}
