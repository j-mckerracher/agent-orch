import type { SubmitDraft } from "./drafts";
import type { TaskSubmission } from "../daemon/types";
import { isFixtureId } from "../daemon/api";

export type SubmissionResult = { ok: true; submission: TaskSubmission } | { ok: false; errors: string[] };

const ids = (text: string): string[] =>
  text
    .split(/[\s,]+/)
    .map((part) => part.trim())
    .filter(Boolean);

/** Mirrors `MAX_RETRIES` and `MAX_REPAIRS` in the engine's contract limits. */
const MAX_ATTEMPTS = 10;

function limit(text: string, label: string, errors: string[]): number {
  const value = Number(text);
  if (text.trim() === "" || !Number.isInteger(value) || value < 0 || value > MAX_ATTEMPTS) {
    errors.push(`${label} must be a whole number from 0 to ${MAX_ATTEMPTS}.`);
    return 0;
  }
  return value;
}

/** Validates the form locally; the daemon remains the authority on the contract. */
export function buildSubmission(projectId: string, draft: SubmitDraft): SubmissionResult {
  const errors: string[] = [];
  if (draft.objective.trim() === "") errors.push("Objective is required.");
  if (draft.agent.trim() === "") errors.push("Agent is required; no agent is chosen automatically.");
  const dependencies = ids(draft.dependencies);
  const parent = draft.parentId.trim();
  if (dependencies.some(isFixtureId) || isFixtureId(parent)) {
    errors.push("Placeholder ids cannot be used as dependencies or parent.");
  }
  if (new Set(dependencies).size !== dependencies.length) errors.push("Dependencies must not repeat a task ID.");
  const maxRetries = limit(draft.maxRetries, "Max retries", errors);
  const maxRepairs = limit(draft.maxRepairs, "Max repairs", errors);
  let verification: TaskSubmission["verification"] = "None";
  if (draft.verification === "Commands") {
    const commands = draft.commands
      .split("\n")
      .map((line) => line.trim().split(/\s+/).filter(Boolean))
      .filter((argv) => argv.length > 0);
    if (commands.length === 0) errors.push("Add at least one verification command, or choose None.");
    verification = { Commands: { commands } };
  }
  if (errors.length > 0) return { ok: false, errors };
  return {
    ok: true,
    submission: {
      project_id: projectId,
      parent_id: parent === "" ? null : parent,
      objective: draft.objective.trim(),
      kind: draft.kind,
      agent: draft.agent.trim(),
      dependencies,
      verification,
      delivery: draft.delivery,
      autonomy: draft.autonomy,
      max_retries: maxRetries,
      max_repairs: maxRepairs,
    },
  };
}
