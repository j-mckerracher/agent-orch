import { describe, expect, it } from "vitest";
import type { PullWire } from "../../daemon/types";
import { FakeDaemon, until } from "../../test/fakeDaemon";
import { ConnectionManager } from "../../daemon/connection";
import { createFixturePulls, createLivePulls, mapPull, mapPullFacts } from "./pulls";
import { clock } from "./testkit";

const WIRE: PullWire = {
  task_id: "t-1",
  number: 42,
  state: "open",
  head_sha: "a".repeat(40),
  base_branch: "main",
  branch: "orchestrator/t-1",
  mergeable: true,
  required_total: 2,
  required_passed: 1,
  required_failed: 0,
  checks: [{ name: "build", result: "pass" }],
  title: "Fix it",
  observed_age_secs: 12,
  merge: null,
};

describe("pull request sources", () => {
  it("landing_map_pull_copies_every_wire_field_and_reads_mergeable_in_both_forms", () => {
    expect(mapPull(WIRE)).toEqual({
      taskId: "t-1",
      number: 42,
      state: "open",
      headSha: "a".repeat(40),
      baseBranch: "main",
      branch: "orchestrator/t-1",
      mergeable: "yes",
      requiredTotal: 2,
      requiredPassed: 1,
      requiredFailed: 0,
      checks: [{ name: "build", result: "pass" }],
      title: "Fix it",
      observedAgeSecs: 12,
      merge: null,
    });
    expect(mapPull({ ...WIRE, mergeable: "CONFLICTING" }).mergeable).toBe("no");
    expect(mapPull({ ...WIRE, mergeable: null }).mergeable).toBe("unknown");
    expect(mapPull({ ...WIRE, merge: { state: "refused", refusal: "stale" } }).merge).toEqual({ state: "refused", refusal: "stale" });
  });

  it("landing_map_pull_facts_defaults_a_missing_refusal_to_null", () => {
    const facts = mapPullFacts({ merge_method: "squash", required_total: 1, required_passed: 1, required_failed: 0, checks: [], observed_age_secs: 3 });
    expect(facts).toMatchObject({ mergeMethod: "squash", refusal: null, observedAgeSecs: 3 });
  });

  it("landing_fixture_source_lists_sample_rows_and_follows_the_scenario", async () => {
    const ok = await createFixturePulls("success", clock).list();
    expect(ok.status === "ok" && ok.data.length).toBe(2);
    expect(ok.provenance).toMatchObject({ kind: "fixture" });
    const empty = await createFixturePulls("empty", clock).list();
    expect(empty.status === "ok" && empty.data).toEqual([]);
    expect((await createFixturePulls("unsupported", clock).list()).status).toBe("unsupported");
    await expect(createFixturePulls("success", clock).refresh("nope")).rejects.toMatchObject({ kind: "not-found" });
  });

  it("landing_live_source_reads_pulls_list_and_a_daemon_without_the_method_is_unsupported", async () => {
    const fake = new FakeDaemon();
    fake.stub("pulls.list", { pulls: [WIRE] });
    const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 250 });
    connection.start();
    await until(() => connection.getSnapshot().status === "connected", "connected");
    const source = createLivePulls(connection, { kind: "daemon", id: "D16", methods: ["pulls.list"] }, clock);
    const read = await source.list();
    expect(read.status === "ok" && read.data[0]?.number).toBe(42);
    const bare = new FakeDaemon();
    const other = new ConnectionManager({ connector: bare, backoffMs: [5], requestTimeoutMs: 250 });
    other.start();
    await until(() => other.getSnapshot().status === "connected", "connected");
    const missing = await createLivePulls(other, { kind: "daemon", id: "D16", methods: ["pulls.list"] }, clock).list();
    expect(missing.status).toBe("unsupported");
    connection.stop();
    other.stop();
  });
});
