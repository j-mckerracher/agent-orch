/**
 * Watched pull requests (DPES-1105 Phase 4c): `pulls.list` and `pulls.refresh`, both reads.
 *
 * Rules:
 * - The live source never imports fixtures. A failed read is `unavailable`, and `unsupported` when the daemon has
 *   no `landing` capability; it is never answered with fixture data.
 * - Nothing here merges anything: a merge is reachable only through an approved `delivery.merge` card.
 * - Titles, branch names and check names are remote text. This module only copies them; the views show them as plain text.
 * - The fixture source never calls a daemon.
 * - One mapping function per wire type, so a change to the daemon's shape is a change in one place.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError } from "../../daemon/errors";
import type { PullFactsWire, PullWire } from "../../daemon/types";
import type { SurfaceFixtureScenario } from "./fixtures";
import { liveRead } from "./liveSupport";
import { notOk, observation, okResult } from "./result";
import type { PullFacts, PullItem, PullsSource, Provenance } from "./types";

export const PULLS_METHODS = ["pulls.list", "pulls.refresh"] as const;

/** The daemon allows one fresh read per task every 10 seconds; the Refresh button waits as long. */
export const PULL_REFRESH_COOLDOWN_MS = 10_000;

/** The host's `mergeable` as a boolean or a word; anything else is not known. */
function mergeableOf(value: PullWire["mergeable"]): PullItem["mergeable"] {
  if (value === true) return "yes";
  if (value === false) return "no";
  if (typeof value !== "string") return "unknown";
  const word = value.trim().toLowerCase();
  if (word === "yes" || word === "mergeable" || word === "clean") return "yes";
  return word === "no" || word === "conflicting" || word === "conflict" ? "no" : "unknown";
}

/** The facts of a merge card's `target.pull`. */
export function mapPullFacts(wire: PullFactsWire): PullFacts {
  return {
    mergeMethod: wire.merge_method,
    requiredTotal: wire.required_total,
    requiredPassed: wire.required_passed,
    requiredFailed: wire.required_failed,
    checks: wire.checks.map((check) => ({ name: check.name, result: check.result })),
    observedAgeSecs: wire.observed_age_secs,
    refusal: wire.refusal ?? null,
  };
}

/** One `pulls.list` row, and the `pulls.refresh` answer. */
export function mapPull(wire: PullWire): PullItem {
  return {
    taskId: wire.task_id,
    number: wire.number,
    state: wire.state,
    headSha: wire.head_sha,
    baseBranch: wire.base_branch,
    branch: wire.branch,
    mergeable: mergeableOf(wire.mergeable),
    requiredTotal: wire.required_total,
    requiredPassed: wire.required_passed,
    requiredFailed: wire.required_failed,
    checks: wire.checks.map((check) => ({ name: check.name, result: check.result })),
    title: wire.title,
    observedAgeSecs: wire.observed_age_secs,
    merge: wire.merge === null || wire.merge === undefined ? null : { state: wire.merge.state, refusal: wire.merge.refusal },
  };
}

export function createLivePulls(connection: ConnectionManager, provenance: Provenance, now: () => number): PullsSource {
  return {
    provenance,
    list: () => liveRead(provenance, now, async () => (await connection.api().pullsList()).pulls.map(mapPull)),
    async refresh(taskId) {
      return mapPull(await connection.api().pullsRefresh(taskId));
    },
  };
}

/** Two sample rows: one with every required check passed, one with a failed check. */
const FIXTURE_PULLS: readonly PullItem[] = [
  {
    taskId: "fixture:task-docs-1",
    number: 42,
    state: "open",
    headSha: "abc123def4567890abc123def4567890abc12345",
    baseBranch: "main",
    branch: "orchestrator/docs-1",
    mergeable: "yes",
    requiredTotal: 2,
    requiredPassed: 2,
    requiredFailed: 0,
    checks: [
      { name: "build", result: "pass" },
      { name: "lint", result: "pass" },
    ],
    title: "Sample: tidy the docs",
    observedAgeSecs: 40,
    merge: null,
  },
  {
    taskId: "fixture:task-api-2",
    number: 43,
    state: "open",
    headSha: "0123456789abcdef0123456789abcdef01234567",
    baseBranch: "main",
    branch: "orchestrator/api-2",
    mergeable: "unknown",
    requiredTotal: 2,
    requiredPassed: 1,
    requiredFailed: 1,
    checks: [
      { name: "build", result: "pass" },
      { name: "tests", result: "fail" },
    ],
    title: "Sample: retry the sync call",
    observedAgeSecs: 125,
    merge: null,
  },
];

export function createFixturePulls(scenario: SurfaceFixtureScenario, now: () => number): PullsSource {
  const provenance: Provenance = { kind: "fixture", id: "D16", fixture: "pulls" };
  return {
    provenance,
    list() {
      if (scenario === "loading") return new Promise(() => undefined);
      if (scenario === "unavailable" || scenario === "unsupported" || scenario === "unknown") {
        return Promise.resolve(notOk(provenance, scenario, `Fixture scenario: pull requests are ${scenario}.`, observation(now())));
      }
      const rows = scenario === "empty" ? [] : FIXTURE_PULLS.map((pull) => ({ ...pull }));
      return Promise.resolve(okResult(provenance, rows, observation(now())));
    },
    refresh(taskId) {
      const pull = FIXTURE_PULLS.find((item) => item.taskId === taskId);
      return pull === undefined
        ? Promise.reject(new DaemonError("not-found", "Fixture data has no such pull request."))
        : Promise.resolve({ ...pull, observedAgeSecs: 0 });
    },
  };
}
