import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import type { ActionProposal, GrantItem, GrantPolicy } from "../../../sources/surfaces/types";
import { approvedByText, failureText } from "../workspace/actionCardModel";
import {
  createFailureText,
  initialDraft,
  isActive,
  offeredKinds,
  revokedText,
  statusChip,
  termsOf,
  timeLeftText,
  tokensText,
  usesLeftText,
  validateDraft,
  type GrantDraft,
} from "./grantsModel";
import { projectCode, sha256 } from "./projectCode";

const POLICY: GrantPolicy = {
  grantableKinds: ["workstream.update", "objective.add", "objective.revise", "task.propose", "task.retry", "workstream.link_task"],
  defaultExpiresInSecs: 28800,
  maxExpiresInSecs: 604800,
  minExpiresInSecs: 60,
  defaultMaxUses: 20,
  maxMaxUses: 200,
  maxTokenBudget: 1_000_000_000,
};

const GOOD: GrantDraft = { scopeKind: "project", scopeId: "p-1a2b3c4d", kinds: ["task.propose"], hours: "8", uses: "20", budget: "2000000" };

const GRANT: GrantItem = {
  id: "grant-9f2c1d7e3a",
  scopeKind: "project",
  scopeId: "p-1a2b3c4d",
  scopeLabel: "motif-mcao",
  kinds: ["task.propose"],
  createdAtMs: 1_000,
  expiresAtMs: 1_000 + 3 * 3600_000 + 5 * 60_000,
  maxUses: 20,
  usesUsed: 3,
  tokenBudget: 2_000_000,
  tokensSpent: 410_000,
  status: "active",
  revokedAtMs: null,
};

describe("grants model", () => {
  it("grants_validation_accepts_the_defaults_with_a_scope_a_kind_and_a_budget", () => {
    expect(initialDraft(POLICY)).toMatchObject({ hours: "8", uses: "20", budget: "", kinds: [], scopeId: "" });
    expect(validateDraft(GOOD, POLICY)).toEqual({ scope: null, kinds: null, hours: null, uses: null, budget: null });
  });

  it("grants_validation_refuses_what_the_daemon_would_refuse", () => {
    const errors = (patch: Partial<GrantDraft>) => validateDraft({ ...GOOD, ...patch }, POLICY);
    expect(errors({ scopeId: "" }).scope).not.toBeNull();
    expect(errors({ kinds: [] }).kinds).not.toBeNull();
    expect(errors({ kinds: ["task.cancel"] }).kinds).not.toBeNull();
    expect(errors({ hours: "0" }).hours).not.toBeNull();
    expect(errors({ hours: "168" }).hours).toBeNull();
    expect(errors({ hours: "169" }).hours).not.toBeNull();
    expect(errors({ hours: "2.5" }).hours).not.toBeNull();
    expect(errors({ uses: "0" }).uses).not.toBeNull();
    expect(errors({ uses: "200" }).uses).toBeNull();
    expect(errors({ uses: "201" }).uses).not.toBeNull();
    expect(errors({ budget: "" }).budget).not.toBeNull();
    expect(errors({ budget: "0" }).budget).not.toBeNull();
    expect(errors({ budget: "1e3" }).budget).not.toBeNull();
    expect(errors({ budget: "1000000000" }).budget).toBeNull();
    expect(errors({ budget: "1000000001" }).budget).not.toBeNull();
  });

  it("grants_offered_kinds_are_the_daemons_list_without_a_never_grantable_kind", () => {
    const wide: GrantPolicy = { ...POLICY, grantableKinds: [...POLICY.grantableKinds, "task.cancel", "task.revise", "project.remove", "delivery.merge", "future.kind"] };
    expect(offeredKinds(wide)).toEqual(POLICY.grantableKinds);
  });

  it("grants_terms_carry_exactly_the_chosen_kinds_in_seconds", () => {
    const terms = termsOf({ ...GOOD, kinds: ["task.retry", "task.propose"], hours: "2" }, POLICY);
    expect(terms).toEqual({
      scopeKind: "project",
      scopeId: "p-1a2b3c4d",
      kinds: ["task.propose", "task.retry"],
      expiresInSecs: 7200,
      maxUses: 20,
      tokenBudget: 2_000_000,
    });
  });

  it("grants_status_chips_say_each_state_in_words", () => {
    const at = (status: string, now = 2_000) => statusChip({ ...GRANT, status }, now).text;
    expect(at("active")).toBe("Active");
    expect(at("active", GRANT.expiresAtMs)).toBe("Expired");
    expect(at("expired")).toBe("Expired");
    expect(at("revoked")).toBe("Revoked");
    expect(at("exhausted")).toBe("Used up");
    expect(at("budget_exhausted")).toBe("Out of budget");
    expect(at("paused")).toBe("Unknown status: paused");
    expect(isActive(GRANT, 2_000)).toBe(true);
    expect(isActive({ ...GRANT, status: "revoked" }, 2_000)).toBe(false);
  });

  it("grants_list_text_shows_uses_left_time_left_and_tokens_reported_so_far", () => {
    expect(usesLeftText(GRANT)).toBe("17 of 20 uses left");
    expect(timeLeftText(GRANT, 1_000)).toBe("3 hours 5 minutes left");
    expect(timeLeftText({ ...GRANT, status: "expired" }, 1_000)).toBe("Ended");
    expect(tokensText(GRANT)).toBe("410,000 of 2,000,000 tokens reported so far");
  });

  it("grants_failure_text_names_the_refusal_without_a_path", () => {
    expect(createFailureText(new DaemonError("forbidden", "x", -32012))).toMatch(/Only the app that started the service/);
    expect(createFailureText(new DaemonError("grant-invalid", "x", -32025))).toMatch(/refused these terms/);
    expect(revokedText(0)).toBe("Standing approval revoked.");
    expect(revokedText(1)).toBe("Standing approval revoked. 1 pending action stopped.");
    expect(revokedText(3)).toBe("Standing approval revoked. 3 pending actions stopped.");
  });

  it("grants_project_code_follows_the_daemons_sha256_rule", () => {
    // The published SHA-256 test vector for "abc".
    const digest = Array.from(sha256(new TextEncoder().encode("abc")), (byte) => byte.toString(16).padStart(2, "0")).join("");
    expect(digest).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    expect(projectCode("abc")).toBe("p-ba7816bf");
    // A message longer than one block.
    const long = Array.from(sha256(new TextEncoder().encode("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")), (byte) => byte.toString(16).padStart(2, "0")).join("");
    expect(long).toBe("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
  });
});

const CARD: ActionProposal = {
  id: "proposal-1",
  turnId: "turn-1",
  index: 0,
  kind: "task.propose",
  params: {},
  digest: "d".repeat(64),
  dependsOn: null,
  state: "executed",
  resultRef: "task-1",
  errorCode: null,
  denial: null,
  target: null,
  updatedAt: "2026-11-10T21:00:00Z",
};

describe("grant label on a card", () => {
  it("grants_card_says_approved_by_a_standing_approval_with_a_short_label_only", () => {
    const text = approvedByText({ ...CARD, approvedBy: { kind: "grant", grantId: "grant-9f2c1d7e3a5b6c7d8e9f" } });
    expect(text).toBe("Approved by a standing approval (grant-9f2c).");
    expect(approvedByText({ ...CARD, approvedBy: { kind: "user" } })).toBeNull();
    expect(approvedByText(CARD)).toBeNull();
  });

  it("grants_card_failure_codes_read_in_plain_words", () => {
    expect(failureText({ ...CARD, state: "failed", errorCode: "grant_revoked" })).toMatch(/standing approval was revoked before this ran/);
    expect(failureText({ ...CARD, state: "failed", errorCode: "grant_expired" })).toMatch(/standing approval had expired before this ran/);
  });
});
