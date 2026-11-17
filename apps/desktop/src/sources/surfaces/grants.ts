/**
 * Standing approvals (DPES-1105 Phase 5): `grants.list`, `grants.create` and `grants.revoke`.
 *
 * Rules:
 * - The live source never imports fixtures. A failed read is `unavailable`, and `unsupported` when the daemon has
 *   no `standing_approvals` capability; it is never answered with fixture data.
 * - `grants.create` and `grants.revoke` are user-only: the daemon refuses unless this session proved the approver
 *   secret. Each call carries an idempotency key that is reused only while the previous outcome is unknown.
 * - The fixture source never calls a daemon and never saves anything. Its receipts say so.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { GrantPolicyWire, GrantWire, GrantsCreateParams } from "../../daemon/types";
import type { SurfaceFixtureScenario } from "./fixtures";
import { liveRead } from "./liveSupport";
import { notOk, observation, okResult } from "./result";
import type { GrantItem, GrantPolicy, GrantTerms, MergePolicy, GrantsSnapshot, GrantsSource, Provenance } from "./types";

export const GRANTS_METHODS = ["grants.list", "grants.create", "grants.revoke"] as const;

export const FIXTURE_GRANT_NOTICE = "Fixture data: nothing was saved.";

export function mapGrant(wire: GrantWire): GrantItem {
  return {
    id: wire.id,
    scopeKind: wire.scope.kind,
    scopeId: wire.scope.id,
    scopeLabel: wire.scope.label ?? null,
    kinds: wire.kinds,
    createdAtMs: wire.created_at_ms,
    expiresAtMs: wire.expires_at_ms,
    maxUses: wire.max_uses,
    usesUsed: wire.uses_used,
    tokenBudget: wire.token_budget,
    tokensSpent: wire.tokens_spent,
    status: wire.status,
    revokedAtMs: wire.revoked_at_ms,
    branchPrefix: wire.branch_prefix ?? null,
  };
}

/** The merge limits as the daemon sends them (`merge_*`, in seconds); null unless the daemon says the merge kind is available. */
function mapMergePolicy(wire: GrantPolicyWire): MergePolicy | null {
  if (wire.merge_kind_available !== true) return null;
  return {
    ...(wire.merge_default_max_uses === undefined ? {} : { defaultUses: wire.merge_default_max_uses }),
    ...(wire.merge_max_max_uses === undefined ? {} : { maxUses: wire.merge_max_max_uses }),
    ...(wire.merge_max_expires_in_secs === undefined ? {} : { maxExpiryHours: Math.floor(wire.merge_max_expires_in_secs / 3600) }),
  };
}

export function mapPolicy(wire: GrantPolicyWire): GrantPolicy {
  return {
    grantableKinds: wire.grantable_kinds,
    defaultExpiresInSecs: wire.default_expires_in_secs,
    maxExpiresInSecs: wire.max_expires_in_secs,
    minExpiresInSecs: wire.min_expires_in_secs,
    defaultMaxUses: wire.default_max_uses,
    maxMaxUses: wire.max_max_uses,
    maxTokenBudget: wire.max_token_budget,
    mergePolicy: mapMergePolicy(wire),
  };
}

/** The `grants.create` params for terms; the merge fields travel only with a merge grant. */
export function createParams(terms: GrantTerms): GrantsCreateParams {
  return {
    scope: { kind: terms.scopeKind, id: terms.scopeId },
    kinds: terms.kinds,
    expires_in_secs: terms.expiresInSecs,
    max_uses: terms.maxUses,
    token_budget: terms.tokenBudget,
    ...(terms.confirmUnattendedMerge === undefined ? {} : { confirm_unattended_merge: terms.confirmUnattendedMerge }),
    ...(terms.branchPrefix ? { branch_prefix: terms.branchPrefix } : {}),
  };
}

export function createLiveGrants(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): GrantsSource {
  /** One user-only write: the key survives only an outcome the app cannot know. */
  async function write<T extends { grant: GrantWire }>(scope: string, fingerprint: string, send: (key: string) => Promise<T>): Promise<T> {
    const key = keys.keyFor(scope, fingerprint);
    try {
      const result = await send(key);
      keys.settle(scope, null);
      return result;
    } catch (error) {
      keys.settle(scope, asDaemonError(error));
      throw error;
    }
  }
  return {
    provenance,
    list: () =>
      liveRead(provenance, now, async (): Promise<GrantsSnapshot> => {
        const result = await connection.api().grantsList(true);
        return { grants: result.grants.map(mapGrant), policy: mapPolicy(result.policy) };
      }),
    async create(terms) {
      const params = createParams(terms);
      const result = await write("grants:create", JSON.stringify(params), (key) => connection.api().grantsCreate(params, key));
      return { simulated: false, provenance, notice: null, grant: mapGrant(result.grant), stopped: 0 };
    },
    async revoke(grantId) {
      const result = await write(`grants:revoke:${grantId}`, "revoke", (key) => connection.api().grantsRevoke(grantId, key));
      return { simulated: false, provenance, notice: null, grant: mapGrant(result.grant), stopped: result.stopped };
    },
  };
}

/** The limits the core enforces; the fixture daemon-less policy mirrors them. */
const FIXTURE_POLICY: GrantPolicy = {
  grantableKinds: ["workstream.update", "objective.add", "objective.revise", "task.propose", "task.retry", "workstream.link_task"],
  defaultExpiresInSecs: 8 * 3600,
  maxExpiresInSecs: 7 * 24 * 3600,
  minExpiresInSecs: 60,
  defaultMaxUses: 20,
  maxMaxUses: 200,
  maxTokenBudget: 1_000_000_000,
};

export function createFixtureGrants(scenario: SurfaceFixtureScenario, now: () => number): GrantsSource {
  const provenance: Provenance = { kind: "fixture", id: "D17", fixture: "grants" };
  const fixtureGrant = (): GrantItem => ({
    id: "fixture:grant-1",
    scopeKind: "project",
    scopeId: "fixture:project",
    scopeLabel: "sample-project",
    kinds: ["task.propose", "task.retry"],
    createdAtMs: now(),
    expiresAtMs: now() + 4 * 3600 * 1000,
    maxUses: 20,
    usesUsed: 3,
    tokenBudget: 2_000_000,
    tokensSpent: 410_000,
    status: "active",
    revokedAtMs: null,
  });
  return {
    provenance,
    list() {
      if (scenario === "loading") return new Promise(() => undefined);
      if (scenario === "unavailable" || scenario === "unsupported" || scenario === "unknown") {
        return Promise.resolve(notOk(provenance, scenario, `Fixture scenario: standing approvals are ${scenario}.`, observation(now())));
      }
      const grants = scenario === "empty" ? [] : [fixtureGrant()];
      return Promise.resolve(okResult(provenance, { grants, policy: FIXTURE_POLICY }, observation(now())));
    },
    // A fixture saves nothing: the receipt echoes what was typed and says so.
    create(terms: GrantTerms) {
      const grant: GrantItem = {
        ...fixtureGrant(),
        id: "fixture:grant-new",
        scopeKind: terms.scopeKind,
        scopeId: terms.scopeId,
        scopeLabel: null,
        kinds: terms.kinds,
        expiresAtMs: now() + terms.expiresInSecs * 1000,
        maxUses: terms.maxUses,
        usesUsed: 0,
        tokenBudget: terms.tokenBudget,
        tokensSpent: 0,
        branchPrefix: terms.branchPrefix ?? null,
      };
      return Promise.resolve({ simulated: true, provenance, notice: FIXTURE_GRANT_NOTICE, grant, stopped: 0 });
    },
    revoke(grantId) {
      if (grantId !== fixtureGrant().id) return Promise.reject(new DaemonError("grant-not-found", "Fixture data has no such standing approval."));
      return Promise.resolve({
        simulated: true,
        provenance,
        notice: FIXTURE_GRANT_NOTICE,
        grant: { ...fixtureGrant(), status: "revoked", revokedAtMs: now() },
        stopped: 0,
      });
    },
  };
}
