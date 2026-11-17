/**
 * Project delivery defaults (DPES-1105 Phase 4b): `projects.get_defaults` and `projects.set_defaults`, and the
 * pure checks the Projects page runs before it sends them.
 *
 * Rules (shared with `derived.ts`, which wires the live source in):
 * - The live source never imports fixtures. A failed read is `unavailable`, or `unsupported` when the daemon
 *   has no `projects.get_defaults`; it is never answered with fixture data.
 * - `projects.set_defaults` is a user-only method: the daemon refuses it unless this session proved the
 *   approver secret. The call carries an idempotency key that is reused only while the outcome is unknown.
 * - The checks here mirror the daemon's rules so the user sees a message before sending. The daemon stays the
 *   authority: an answer that disagrees is shown as the daemon's refusal.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { asDaemonError, presentError, type DaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { ProjectDefaultsWire } from "../../daemon/types";
import { liveRead } from "./liveSupport";
import type { ProjectDefaults, ProjectDefaultsSource, Provenance } from "./types";

export const PROJECT_DEFAULTS_METHODS = ["projects.get_defaults", "projects.set_defaults"] as const;

export const MODE_OPTIONS = [
  { value: "local-only", label: "Review only (local-only)", consequence: "A finished task waits for your review on this machine. Nothing leaves it." },
  {
    value: "direct-pr",
    label: "Pull request, run checks (direct-pr)",
    consequence: "A finished task is checked, then offered as a pull request you open yourself.",
  },
  {
    value: "no-mistakes",
    label: "Pull request, full verification (no-mistakes)",
    consequence: "Like direct-pr, but the project must have checks and every one must pass first.",
  },
] as const;

export const MERGE_OPTIONS = [
  { value: "squash", label: "Squash" },
  { value: "merge", label: "Merge commit" },
  { value: "rebase", label: "Rebase" },
] as const;

export const MAX_CHECKS = 8;
export const MAX_ARGS = 16;
export const MAX_ARG_BYTES = 256;
export const MAX_BRANCH_NAME = 100;
export const MAX_PREFIX = 64;
const RESERVED_NAMESPACE = "orchestrator/";

export const PREFIX_ERROR = "Use letters, digits, dot, dash, underscore and slash, ending with a slash. Do not start with orchestrator/.";
export const BRANCH_ERROR = "Use letters, digits, dot, dash, underscore and slash. Do not use HEAD, two dots in a row, or a name ending in a dot.";
export const NO_CHECKS_ERROR = "Full verification needs at least one check command.";
export const FIXTURE_SAVE_TEXT = "Fixture data: nothing was changed.";

/** What the form holds: text as typed. `checks` is one command per entry, each a space-separated line. */
export interface DefaultsDraft {
  mode: string;
  baseBranch: string;
  branchPrefix: string;
  checks: string[];
  mergeMethod: string;
}

export interface DefaultsErrors {
  baseBranch: string | null;
  branchPrefix: string | null;
  /** The first command the daemon would refuse, or the no-checks rule; one message for the whole editor. */
  checks: string | null;
}

export function draftOf(defaults: ProjectDefaults): DefaultsDraft {
  return {
    mode: defaults.mode,
    baseBranch: defaults.baseBranch ?? "",
    branchPrefix: defaults.branchPrefix ?? "",
    checks: defaults.checks.map((command) => command.join(" ")),
    mergeMethod: defaults.mergeMethod,
  };
}

/** One command line into arguments: split on white space only. No quotes, no escapes, no shell. */
export function splitCommand(line: string): string[] {
  return line.split(/\s+/).filter((word) => word !== "");
}

/** The commands the draft holds; a blank line is no command. */
export function commandsOf(draft: DefaultsDraft): string[][] {
  return draft.checks.map(splitCommand).filter((command) => command.length > 0);
}

/** Mirrors the daemon's branch-name rule: one name made of safe components. */
export function isSafeBranchName(name: string): boolean {
  if (name === "" || name.length > MAX_BRANCH_NAME || name === "HEAD" || name.endsWith(".") || name.includes("..")) return false;
  return name.split("/").every((part) => /^[A-Za-z0-9._-]+$/.test(part) && !/^[-.]/.test(part) && !part.endsWith(".lock"));
}

/** Mirrors the daemon's prefix rule: a safe name that ends with a slash, outside `orchestrator/`. */
export function isSafePrefix(prefix: string): boolean {
  return prefix.endsWith("/") && prefix.length <= MAX_PREFIX && !prefix.startsWith(RESERVED_NAMESPACE) && isSafeBranchName(prefix.slice(0, -1));
}

const byteLength = (text: string) => new TextEncoder().encode(text).length;

function checksError(draft: DefaultsDraft): string | null {
  const commands = commandsOf(draft);
  if (commands.length > MAX_CHECKS) return `Use at most ${MAX_CHECKS} commands.`;
  for (const [index, command] of commands.entries()) {
    const where = `Command ${index + 1}`;
    if (command.length > MAX_ARGS) return `${where} has more than ${MAX_ARGS} words.`;
    if (command.some((word) => byteLength(word) > MAX_ARG_BYTES || word.includes("\0"))) {
      return `${where} has a word longer than ${MAX_ARG_BYTES} bytes or one with a null character.`;
    }
  }
  return draft.mode === "no-mistakes" && commands.length === 0 ? NO_CHECKS_ERROR : null;
}

export function validateDraft(draft: DefaultsDraft): DefaultsErrors {
  const base = draft.baseBranch.trim();
  const prefix = draft.branchPrefix.trim();
  return {
    baseBranch: base === "" || isSafeBranchName(base) ? null : BRANCH_ERROR,
    branchPrefix: prefix === "" || isSafePrefix(prefix) ? null : PREFIX_ERROR,
    checks: checksError(draft),
  };
}

export const hasErrors = (errors: DefaultsErrors): boolean => Object.values(errors).some((message) => message !== null);

/** What Save sends. Empty text is `null`: the checkout's current branch and the machine-wide prefix. */
export function defaultsOf(draft: DefaultsDraft): ProjectDefaults {
  return {
    mode: draft.mode,
    baseBranch: draft.baseBranch.trim() === "" ? null : draft.baseBranch.trim(),
    branchPrefix: draft.branchPrefix.trim() === "" ? null : draft.branchPrefix.trim(),
    checks: commandsOf(draft),
    mergeMethod: draft.mergeMethod,
  };
}

export function sameDefaults(a: ProjectDefaults, b: ProjectDefaults): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Plain words for a failed save. The daemon is the authority, so its refusal is explained, not second-guessed. */
export function saveFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "invalid-params":
      return "The daemon refused these values. The base branch may not exist in this repository, the prefix may use orchestrator/, or full verification may have no checks. Nothing was changed.";
    case "forbidden":
      return "Only the app that started the service can change project defaults. Nothing was changed.";
    case "timeout":
      return "The daemon did not answer. Nothing was changed.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}

export function mapDefaults(wire: ProjectDefaultsWire): ProjectDefaults {
  return {
    mode: wire.mode,
    baseBranch: wire.base_branch,
    branchPrefix: wire.branch_prefix,
    checks: wire.checks,
    mergeMethod: wire.merge_method,
  };
}

function wireOf(defaults: ProjectDefaults): ProjectDefaultsWire {
  return {
    mode: defaults.mode,
    base_branch: defaults.baseBranch,
    branch_prefix: defaults.branchPrefix,
    checks: defaults.checks,
    merge_method: defaults.mergeMethod,
  };
}

export function createLiveProjectDefaults(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): ProjectDefaultsSource {
  return {
    provenance,
    get: (projectId) => liveRead(provenance, now, async () => mapDefaults((await connection.api().projectsGetDefaults(projectId)).defaults)),
    async set(projectId, defaults) {
      const scope = `project-defaults:set:${projectId}`;
      const wire = wireOf(defaults);
      const key = keys.keyFor(scope, JSON.stringify(wire));
      try {
        const result = await connection.api().projectsSetDefaults(projectId, wire, key);
        keys.settle(scope, null);
        return { simulated: false, provenance, defaults: mapDefaults(result.defaults) };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };
}
