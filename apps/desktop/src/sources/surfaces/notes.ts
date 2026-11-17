/**
 * Orchestrator notes (DPES-1105 5b): `notes.list` and `notes.set`.
 *
 * Rules:
 * - the live source never imports fixtures. A failed read is `unavailable`, `unsupported` when the daemon has
 *   no `standing_approvals` capability; it never answers fixture data.
 * - `notes.set` is user-only and replaces the whole list. Its idempotency key is reused only while the previous
 *   outcome is unknown.
 * - the fixture source never calls the daemon and never saves anything. Its receipts say so.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { NotesWire } from "../../daemon/types";
import type { SurfaceFixtureScenario } from "./fixtures";
import { liveRead } from "./liveSupport";
import { notOk, observation, okResult } from "./result";
import type { NotesSnapshot, NotesSource, Provenance } from "./types";

export const NOTES_METHODS = ["notes.list", "notes.set"] as const;

export const FIXTURE_NOTES_NOTICE = "Fixture data: nothing was saved.";

/** The core's limits; the fixture daemon-less source mirrors them. */
const FIXTURE_MAX_NOTES = 10;
const FIXTURE_MAX_BYTES = 200;

export function mapNotes(wire: NotesWire): NotesSnapshot {
  return {
    scope: wire.scope,
    notes: [...wire.notes].sort((a, b) => a.position - b.position).map((note) => note.body),
    maxNotes: wire.limits.max_notes,
    maxBytes: wire.limits.max_bytes,
  };
}

export function createLiveNotes(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): NotesSource {
  return {
    provenance,
    list: (scope) => liveRead(provenance, now, async () => mapNotes(await connection.api().notesList(scope))),
    async set(scope, notes) {
      const tracked = `notes:set:${scope}`;
      const key = keys.keyFor(tracked, JSON.stringify(notes));
      try {
        await connection.api().notesSet(scope, notes, key);
        keys.settle(tracked, null);
        // The reply holds no note text, so the saved list is read back.
        const saved = await connection.api().notesList(scope);
        return { simulated: false, provenance, notice: null, snapshot: mapNotes(saved) };
      } catch (error) {
        keys.settle(tracked, asDaemonError(error));
        throw error;
      }
    },
  };
}

export function createFixtureNotes(scenario: SurfaceFixtureScenario, now: () => number): NotesSource {
  const provenance: Provenance = { kind: "fixture", id: "D17", fixture: "notes" };
  const snapshot = (scope: string, notes: string[]): NotesSnapshot => ({
    scope,
    notes,
    maxNotes: FIXTURE_MAX_NOTES,
    maxBytes: FIXTURE_MAX_BYTES,
  });
  return {
    provenance,
    list(scope) {
      if (scenario === "loading") return new Promise(() => undefined);
      if (scenario === "unavailable" || scenario === "unsupported" || scenario === "unknown") {
        return Promise.resolve(notOk(provenance, scenario, `Fixture scenario: notes are ${scenario}.`, observation(now())));
      }
      const notes = scenario === "empty" ? [] : ["Prefer small pull requests.", "The release freeze starts on Friday."];
      return Promise.resolve(okResult(provenance, snapshot(scope, notes), observation(now())));
    },
    // A fixture saves nothing: the receipt echoes what was sent and says so.
    set: (scope, notes) => Promise.resolve({ simulated: true, provenance, notice: FIXTURE_NOTES_NOTICE, snapshot: snapshot(scope, notes) }),
  };
}
