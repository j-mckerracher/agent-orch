import { afterEach, describe, expect, it } from "vitest";
import { ConnectionManager } from "../../daemon/connection";
import { IdempotencyTracker } from "../../daemon/idempotency";
import { ALL_CAPABILITIES, type ProjectDefaultsWire } from "../../daemon/types";
import { FakeDaemon, until } from "../../test/fakeDaemon";
import { FIXTURE_PROJECT_DEFAULTS, createFixtureSurfaces } from "./fixtures";
import {
  NO_CHECKS_ERROR,
  PREFIX_ERROR,
  commandsOf,
  createLiveProjectDefaults,
  defaultsOf,
  draftOf,
  isSafeBranchName,
  isSafePrefix,
  splitCommand,
  validateDraft,
  type DefaultsDraft,
} from "./projectDefaults";

const PROJECT = "/work/repo";
const WIRE: ProjectDefaultsWire = {
  mode: "direct-pr",
  base_branch: "main",
  branch_prefix: "moa/",
  checks: [["cargo", "test"]],
  merge_method: "squash",
};
const DRAFT: DefaultsDraft = { mode: "direct-pr", baseBranch: "", branchPrefix: "", checks: ["cargo test"], mergeMethod: "squash" };

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function liveSource(fake: FakeDaemon, start = true) {
  const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
  stops.push(() => connection.stop());
  if (start) {
    connection.start();
    await until(() => connection.getSnapshot().status === "connected", "connected");
  }
  return createLiveProjectDefaults(connection, { kind: "daemon", id: "D2", methods: ["projects.get_defaults"] }, new IdempotencyTracker(), () => 0);
}

describe("project defaults checks", () => {
  it("project_defaults_split_a_command_on_white_space_only_without_shell_parsing", () => {
    expect(splitCommand("  cargo   test --all ")).toEqual(["cargo", "test", "--all"]);
    expect(splitCommand(`sh -c "echo $HOME; rm x"`)).toEqual(["sh", "-c", `"echo`, "$HOME;", "rm", `x"`]);
    expect(commandsOf({ ...DRAFT, checks: ["cargo test", "   ", "npm run lint"] })).toEqual([["cargo", "test"], ["npm", "run", "lint"]]);
  });

  it("project_defaults_branch_and_prefix_rules_match_the_daemon", () => {
    for (const name of ["main", "release/1.2", "feat/a_b-c.d"]) expect(isSafeBranchName(name)).toBe(true);
    for (const name of ["", "HEAD", "a..b", "a/", "/a", "a//b", "-x", ".x", "x.lock", "a b", "x."]) expect(isSafeBranchName(name)).toBe(false);
    expect(isSafeBranchName("a".repeat(101))).toBe(false);
    expect(isSafePrefix("moa/")).toBe(true);
    for (const prefix of ["moa", "orchestrator/", "orchestrator/x/", "/", "a//", "a b/"]) expect(isSafePrefix(prefix)).toBe(false);
    expect(isSafePrefix(`${"a".repeat(64)}/`)).toBe(false);
  });

  it("project_defaults_validate_the_draft_before_sending", () => {
    expect(validateDraft(DRAFT)).toEqual({ baseBranch: null, branchPrefix: null, checks: null });
    expect(validateDraft({ ...DRAFT, branchPrefix: "orchestrator/" }).branchPrefix).toBe(PREFIX_ERROR);
    expect(validateDraft({ ...DRAFT, mode: "no-mistakes", checks: [" "] }).checks).toBe(NO_CHECKS_ERROR);
    expect(validateDraft({ ...DRAFT, mode: "direct-pr", checks: [] }).checks).toBeNull();
    expect(validateDraft({ ...DRAFT, checks: Array.from({ length: 9 }, () => "x") }).checks).toMatch(/at most 8/);
    expect(validateDraft({ ...DRAFT, checks: [Array.from({ length: 17 }, () => "x").join(" ")] }).checks).toMatch(/more than 16/);
    expect(validateDraft({ ...DRAFT, checks: ["x".repeat(257)] }).checks).toMatch(/256 bytes/);
  });

  it("project_defaults_send_empty_text_as_null_and_round_trip_a_saved_value", () => {
    expect(defaultsOf({ ...DRAFT, baseBranch: "  ", branchPrefix: " " })).toEqual({
      mode: "direct-pr",
      baseBranch: null,
      branchPrefix: null,
      checks: [["cargo", "test"]],
      mergeMethod: "squash",
    });
    const saved = FIXTURE_PROJECT_DEFAULTS["fixture:project-gamma"]!;
    expect(defaultsOf(draftOf(saved))).toEqual(saved);
  });
});

describe("live project defaults source", () => {
  it("project_defaults_live_reads_the_wire_into_defaults", async () => {
    const fake = new FakeDaemon();
    fake.stub("projects.get_defaults", { defaults: WIRE });
    const result = await (await liveSource(fake)).get(PROJECT);
    expect(result.status === "ok" && result.data).toEqual({
      mode: "direct-pr",
      baseBranch: "main",
      branchPrefix: "moa/",
      checks: [["cargo", "test"]],
      mergeMethod: "squash",
    });
    expect(fake.callsTo("projects.get_defaults")).toEqual([{ method: "projects.get_defaults", params: { project_id: PROJECT } }]);
  });

  it("project_defaults_live_reads_a_daemon_without_the_method_as_unsupported", async () => {
    const fake = new FakeDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "projects");
    const result = await (await liveSource(fake)).get(PROJECT);
    expect(result.status).toBe("unsupported");
    expect(fake.callsTo("projects.get_defaults")).toEqual([]);
  });

  it("project_defaults_live_refuses_a_fixture_id_by_rejection_not_a_synchronous_throw", async () => {
    const fake = new FakeDaemon();
    const source = await liveSource(fake);
    const result = await source.get("fixture:project-alpha");
    expect(result.status).toBe("unavailable");
    await expect(source.set("fixture:project-alpha", defaultsOf(DRAFT))).rejects.toMatchObject({ kind: "invalid-params" });
    expect(fake.callsTo("projects.get_defaults")).toEqual([]);
    expect(fake.callsTo("projects.set_defaults")).toEqual([]);
  });

  it("project_defaults_live_save_sends_the_full_wire_with_a_key_and_returns_what_the_daemon_holds", async () => {
    const fake = new FakeDaemon();
    fake.stub("projects.set_defaults", { defaults: { ...WIRE, merge_method: "rebase" } });
    const receipt = await (await liveSource(fake)).set(PROJECT, defaultsOf(DRAFT));
    expect(receipt).toMatchObject({ simulated: false, defaults: { mergeMethod: "rebase" } });
    expect(fake.callsTo("projects.set_defaults")).toEqual([
      {
        method: "projects.set_defaults",
        params: {
          project_id: PROJECT,
          mode: "direct-pr",
          base_branch: null,
          branch_prefix: null,
          checks: [["cargo", "test"]],
          merge_method: "squash",
          idempotency_key: expect.any(String),
        },
      },
    ]);
  });

  it("project_defaults_live_save_rejects_a_refusal_and_mints_a_new_key_for_the_next_try", async () => {
    const fake = new FakeDaemon();
    fake.failNext("projects.set_defaults", { code: -32602, message: "base branch not found" });
    fake.stub("projects.set_defaults", { defaults: WIRE });
    const source = await liveSource(fake);
    await expect(source.set(PROJECT, defaultsOf(DRAFT))).rejects.toMatchObject({ kind: "invalid-params" });
    await source.set(PROJECT, defaultsOf(DRAFT));
    const keys = fake.callsTo("projects.set_defaults").map((call) => (call.params as { idempotency_key: string }).idempotency_key);
    expect(keys[0]).not.toBe(keys[1]);
  });
});

describe("fixture project defaults source", () => {
  it("project_defaults_fixture_has_one_project_per_mode_and_never_saves", async () => {
    const source = createFixtureSurfaces().projectDefaults;
    const modes = await Promise.all(
      ["fixture:project-alpha", "fixture:project-beta", "fixture:project-gamma"].map(async (id) => {
        const result = await source.get(id);
        return result.status === "ok" ? result.data.mode : result.status;
      }),
    );
    expect(modes).toEqual(["local-only", "direct-pr", "no-mistakes"]);
    const other = await source.get("fixture:project-unknown");
    expect(other.status === "ok" && other.data.mode).toBe("local-only");
    const receipt = await source.set("fixture:project-alpha", defaultsOf(DRAFT));
    expect(receipt).toMatchObject({ simulated: true, provenance: { kind: "fixture" } });
  });
});
