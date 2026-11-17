import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { mapProposal } from "./actionProposals";
import { createFixtureSurfaces } from "./fixtures";
import { FIXTURE_GRANT_NOTICE, mapGrant } from "./grants";
import { connect, populate } from "./testkit";
import type { GrantTerms } from "./types";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

const GRANT = {
  id: "grant-9f2c",
  scope: { kind: "project", id: "p-1a2b3c4d", label: "motif-mcao" },
  kinds: ["task.propose", "task.retry"],
  created_at_ms: 1_763_350_000_000,
  expires_at_ms: 1_763_378_800_000,
  max_uses: 20,
  uses_used: 3,
  token_budget: 2_000_000,
  tokens_spent: 410_000,
  status: "active",
  revoked_at_ms: null,
};
const POLICY = {
  grantable_kinds: ["task.propose", "task.retry"],
  default_expires_in_secs: 28800,
  max_expires_in_secs: 604800,
  min_expires_in_secs: 60,
  default_max_uses: 20,
  max_max_uses: 200,
  max_token_budget: 1_000_000_000,
};
const TERMS: GrantTerms = { scopeKind: "project", scopeId: "p-1a2b3c4d", kinds: ["task.propose"], expiresInSecs: 7200, maxUses: 5, tokenBudget: 1000 };

async function live(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces;
}

describe("live grants source", () => {
  it("grants_live_list_maps_the_grants_and_the_policy_and_asks_for_ended_ones", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("grants.list", { grants: [GRANT], more: false, policy: POLICY });
    const result = await (await live(fake)).grants.list();
    expect(result.status === "ok" && result.data.grants).toEqual([mapGrant(GRANT)]);
    expect(result.status === "ok" && result.data.grants[0]).toMatchObject({ scopeKind: "project", scopeLabel: "motif-mcao", usesUsed: 3, tokensSpent: 410_000 });
    expect(result.status === "ok" && result.data.policy).toMatchObject({ grantableKinds: ["task.propose", "task.retry"], maxMaxUses: 200 });
    expect(fake.callsTo("grants.list")[0]!.params).toEqual({ include_ended: true, limit: 50 });
  });

  it("grants_live_list_is_unsupported_when_the_daemon_has_no_such_method", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("grants.list", { code: -32601, message: "method not found" });
    expect((await (await live(fake)).grants.list()).status).toBe("unsupported");
  });

  it("grants_live_create_sends_the_scope_the_kinds_the_limits_and_a_key", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("grants.create", { grant: GRANT });
    const receipt = await (await live(fake)).grants.create(TERMS);
    expect(receipt).toMatchObject({ simulated: false, notice: null, grant: { id: "grant-9f2c" } });
    const [call] = fake.callsTo("grants.create");
    expect(Object.keys(call!.params as object).sort()).toEqual(["expires_in_secs", "idempotency_key", "kinds", "max_uses", "scope", "token_budget"]);
    expect(call!.params).toMatchObject({
      scope: { kind: "project", id: "p-1a2b3c4d" },
      kinds: ["task.propose"],
      expires_in_secs: 7200,
      max_uses: 5,
      token_budget: 1000,
    });
  });

  it("grants_live_revoke_sends_the_id_and_a_key_and_reports_how_many_were_stopped", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("grants.revoke", { grant: { ...GRANT, status: "revoked", revoked_at_ms: 1 }, stopped: 2 });
    const receipt = await (await live(fake)).grants.revoke("grant-9f2c");
    expect(receipt).toMatchObject({ simulated: false, stopped: 2, grant: { status: "revoked" } });
    const [call] = fake.callsTo("grants.revoke");
    expect(Object.keys(call!.params as object).sort()).toEqual(["grant_id", "idempotency_key"]);
  });

  it("grants_live_create_rejects_a_refusal_and_never_answers_with_a_receipt", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("grants.create", { code: -32025, message: "grant invalid" });
    fake.failNext("grants.create", { code: -32012, message: "forbidden" });
    const grants = (await live(fake)).grants;
    await expect(grants.create(TERMS)).rejects.toMatchObject({ kind: "grant-invalid" });
    await expect(grants.create(TERMS)).rejects.toMatchObject({ kind: "forbidden" });
  });

  it("grants_live_revoke_refuses_a_fixture_id_before_any_request", async () => {
    const fake = populate(new FakeDaemon());
    await expect((await live(fake)).grants.revoke("fixture:grant-1")).rejects.toBeDefined();
    expect(fake.callsTo("grants.revoke")).toEqual([]);
  });
});

describe("fixture grants source", () => {
  it("grants_fixture_says_nothing_was_saved_and_keeps_its_list", async () => {
    const source = createFixtureSurfaces().grants;
    const created = await source.create(TERMS);
    expect(created).toMatchObject({ simulated: true, notice: FIXTURE_GRANT_NOTICE });
    expect(FIXTURE_GRANT_NOTICE).toBe("Fixture data: nothing was saved.");
    const revoked = await source.revoke("fixture:grant-1");
    expect(revoked.simulated).toBe(true);
    const listed = await source.list();
    expect(listed.status === "ok" && listed.data.grants.map((grant) => grant.status)).toEqual(["active"]);
  });
});

describe("grant actor on a proposal", () => {
  const wire = {
    id: "proposal-1",
    turn_id: "turn-1",
    index: 0,
    kind: "task.propose",
    params: {},
    digest: "d".repeat(64),
    depends_on: null,
    state: "executed",
    result_ref: null,
    error_code: null,
    created_at: "2026-11-10T21:00:00Z",
    updated_at: "2026-11-10T21:00:00Z",
  };

  it("grants_proposal_reads_a_grant_approver_and_leaves_older_shapes_alone", () => {
    expect(mapProposal({ ...wire, approved_by: { kind: "grant", grant_id: "grant-9f2c" } }).approvedBy).toEqual({ kind: "grant", grantId: "grant-9f2c" });
    expect(mapProposal({ ...wire, approved_by: "grant:grant-9f2c" }).approvedBy).toEqual({ kind: "grant", grantId: "grant-9f2c" });
    expect(mapProposal({ ...wire, approved_by: { kind: "user" } }).approvedBy).toEqual({ kind: "user" });
    expect("approvedBy" in mapProposal(wire)).toBe(false);
    expect("approvedBy" in mapProposal({ ...wire, approved_by: "user" })).toBe(false);
  });
});
