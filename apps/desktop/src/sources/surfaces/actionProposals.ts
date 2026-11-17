/**
 * Action proposals for the live conversation (DPES-1105): the wire copy and the approve or reject call,
 * bound to `actions.resolve`. The daemon confirms every decision; this adapter never guesses an outcome.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { ActionProposalWire } from "../../daemon/types";
import { isFixtureLikeId } from "./fixtureIds";
import type { ActionProposal, Decision, Provenance, ResolveReceipt } from "./types";

export const RESOLVE_METHOD = "actions.resolve" as const;

/** Copies a proposal from the wire. The daemon's `params` carry `kind`; the view reads it from `kind`, so it is dropped here. */
export function mapProposal(wire: ActionProposalWire): ActionProposal {
  const params: Record<string, unknown> = { ...wire.params };
  delete params.kind;
  return {
    id: wire.id,
    turnId: wire.turn_id,
    index: wire.index,
    kind: wire.kind,
    params,
    digest: wire.digest,
    dependsOn: wire.depends_on,
    state: wire.state,
    resultRef: wire.result_ref,
    errorCode: wire.error_code,
    // An older daemon sends neither field.
    denial: wire.denial ?? null,
    target: wire.target
      ? {
          projectName: wire.target.project_name,
          taskTitle: wire.target.task_title,
          taskState: wire.target.task_state,
          belowProjectDefault: wire.target.below_project_default,
          projectMode: wire.target.project_mode,
          taskCount: wire.target.task_count,
          runningTasks: wire.target.running_tasks,
        }
      : null,
    updatedAt: wire.updated_at,
  };
}

/**
 * Approve or reject one proposal. The key is per card and per decision and digest: a retry after an outcome the
 * app cannot know (timeout, disconnect, protocol) reuses it, so the daemon replays its answer. Every other outcome
 * mints a new key on the next click. `idempotency-in-progress` and `internal` are not replayable: after a crash
 * the daemon keeps the old key in progress forever (1c Q4), and a store error caches `internal` under the key
 * (1c Q2). A new key cannot run the change twice, because the approve is a compare-and-set from `pending` and
 * the execution does not depend on the key.
 */
export function createLiveResolve(connection: ConnectionManager, provenance: Provenance, keys: IdempotencyTracker) {
  return async (proposalId: string, decision: Decision, digest: string): Promise<ResolveReceipt> => {
    if (isFixtureLikeId(proposalId)) throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
    const scope = `resolve:${proposalId}`;
    const key = keys.keyFor(scope, `${decision}:${digest}`);
    try {
      const result = await connection.api().actionsResolve(proposalId, decision, digest, key);
      keys.settle(scope, null);
      return { simulated: false, provenance, notice: null, proposal: mapProposal(result.proposal) };
    } catch (error) {
      const failed = asDaemonError(error);
      keys.settle(scope, failed.kind === "idempotency-in-progress" || failed.kind === "internal" ? null : failed);
      throw error;
    }
  };
}
