/**
 * Live monitoring source (DPES-1105 Phase 3): `monitoring.get`, `monitoring.set` and `monitoring.ack`.
 *
 * Rules:
 * - Never imports fixtures. A failed read is `unavailable`, or `unsupported` when the daemon has no
 *   `monitoring` capability; it is never answered with fixture data.
 * - Unknown mode and reason words are kept as strings and rendered as unknown, never coerced.
 * - `monitoring.set` is a user-only method: the daemon refuses it unless this session proved the approver
 *   secret. The call carries an idempotency key that is reused only while the previous outcome is unknown.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { BriefItemWire, BriefWire, MonitoringTargetWire, MonitoringWire } from "../../daemon/types";
import { liveRead } from "./liveSupport";
import type { Brief, BriefItem, LiveConversationTarget, MonitoringSource, MonitoringState, Provenance } from "./types";

export const MONITORING_METHODS = ["monitoring.get", "monitoring.set", "monitoring.ack", "monitoring.brief", "monitoring.brief_ack"] as const;

function targetOf(wire: MonitoringTargetWire): LiveConversationTarget {
  return wire.kind === "global" ? { kind: "global" } : { kind: "workstream", workstreamId: wire.workstream_id };
}

export function toWireTarget(target: LiveConversationTarget): MonitoringTargetWire {
  return target.kind === "global" ? { kind: "global" } : { kind: "workstream", workstream_id: target.workstreamId };
}

export function mapMonitoring(wire: MonitoringWire): MonitoringState {
  return {
    mode: wire.mode,
    paused: wire.paused.map((item) => ({ target: targetOf(item.target), reason: item.reason, until: item.until })),
    unseen: wire.unseen.map((item) => ({ target: targetOf(item.target), count: item.count })),
  };
}

const mapItem = (wire: BriefItemWire): BriefItem => ({
  kind: wire.kind,
  id: wire.id,
  title: wire.title,
  state: wire.state,
  project: wire.project,
  atMs: wire.at_ms,
});

export function mapBrief(wire: BriefWire): Brief {
  const c = wire.counts;
  return {
    fromMs: wire.from_ms,
    toMs: wire.to_ms,
    open: wire.open,
    counts: {
      tasksSucceeded: c.tasks_succeeded,
      tasksFailed: c.tasks_failed,
      tasksStalled: c.tasks_stalled,
      tasksRunning: c.tasks_running,
      proposalsApprovedByUser: c.proposals_approved_by_user,
      proposalsApprovedByGrant: c.proposals_approved_by_grant,
      executedByGrant: c.executed_by_grant,
      failedUnderGrant: c.failed_under_grant,
      proposalsWaiting: c.proposals_waiting,
      deliveriesWaiting: c.deliveries_waiting,
      decisionsHeld: c.decisions_held,
      grantUses: c.grant_uses,
      grantsExpired: c.grants_expired,
      grantsRevoked: c.grants_revoked,
      quietUpdates: c.quiet_updates,
      repliesPausedByLimit: c.replies_paused_by_limit,
    },
    grants: wire.grants.map((grant) => ({
      id: grant.id,
      scopeKind: grant.scope_kind,
      scopeLabel: grant.scope_label,
      usesInWindow: grant.uses_in_window,
      usesUsed: grant.uses_used,
      maxUses: grant.max_uses,
      status: grant.status,
    })),
    waiting: wire.waiting.map(mapItem),
    attention: wire.attention.map(mapItem),
  };
}

export function createLiveMonitoring(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): MonitoringSource {
  return {
    provenance,
    get: () => liveRead(provenance, now, async () => mapMonitoring(await connection.api().monitoringGet())),
    async set(mode) {
      const scope = "monitoring:set";
      const key = keys.keyFor(scope, mode);
      try {
        const wire = await connection.api().monitoringSet(mode, key);
        keys.settle(scope, null);
        return { simulated: false, provenance, state: mapMonitoring(wire) };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
    async ack(target) {
      const scope = `monitoring:ack:${JSON.stringify(target)}`;
      const key = keys.keyFor(scope, "ack");
      try {
        await connection.api().monitoringAck(toWireTarget(target), key);
        keys.settle(scope, null);
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
    brief: () =>
      liveRead(provenance, now, async () => {
        const { brief } = await connection.api().briefGet();
        return brief === null ? null : mapBrief(brief);
      }),
    async briefAck() {
      const scope = "monitoring:brief_ack";
      const key = keys.keyFor(scope, "ack");
      try {
        await connection.api().briefAck(key);
        keys.settle(scope, null);
        return { simulated: false };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };
}
