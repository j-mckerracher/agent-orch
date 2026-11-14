/**
 * Review-evidence refinement of the "required next evidence" column (M7-11 F-3).
 *
 * `model.requiredEvidence` derives a text from task state alone. For tasks that may hold a
 * candidate (Running, Verifying, VerificationFailed), the view also reads `review.evidence`
 * and replaces that text with what the daemon reports as missing. The read is bounded and
 * best effort: any failure keeps the state-derived text, and nothing is invented.
 */
import { useEffect, useRef, useState } from "react";
import type { ReviewEvidenceWire, TaskState } from "../../../daemon/types";
import { useGate, useServices } from "../../services";
import type { WsTask } from "./model";

/** States in which a candidate (and so review evidence) can exist but no decision is pending. */
const EVIDENCE_STATES: readonly TaskState[] = ["Running", "Verifying", "VerificationFailed"];
export const MAX_EVIDENCE_READS = 30;
const CONCURRENCY = 4;

const short = (commit: string | null) => (commit ? commit.slice(0, 8) : "unknown commit");
const reasonsOf = (wire: ReviewEvidenceWire) => (wire.reasons.length > 0 ? wire.reasons.join(", ") : "no reason reported");

/** Whether a verification run for the candidate passed; the daemon's status vocabulary is open. */
function passed(wire: ReviewEvidenceWire): boolean {
  return wire.verification.some(
    (run) => run.matches_candidate && ["passed", "pass", "success", "succeeded"].includes(run.status.toLowerCase()),
  );
}

/** The next evidence a task needs, from a `review.evidence` answer. */
export function evidenceFromReview(wire: ReviewEvidenceWire): string {
  const commit = short(wire.candidate.commit);
  switch (wire.review_state) {
    case "approved":
      return `None: review approved for candidate ${commit}`;
    case "changes_requested":
      return "New candidate that addresses the requested changes";
    case "rejected":
      return "New candidate: the previous one was rejected";
    case "superseded":
      return "Evidence for the superseding candidate";
    case "awaiting_decision":
      if (wire.decision_ready && wire.approval) {
        return `Operator decision on ${wire.approval.kind} approval for candidate ${commit}`;
      }
      break;
    case "not_requested":
      break;
  }
  if (wire.state === "missing") return `Candidate evidence missing (${reasonsOf(wire)})`;
  if (wire.state === "stale") return `Fresh candidate evidence; the recorded evidence is stale (${reasonsOf(wire)})`;
  return passed(wire)
    ? `Approval request for candidate ${commit}`
    : `Passing verification run for candidate ${commit}`;
}

/**
 * Per-task evidence text read from `review.evidence` for the given tasks (live mode only).
 * Tasks outside the evidence states, with a pending approval, or past the read cap keep
 * their state-derived text. Entries refresh when a task changes state or the connection
 * generation changes.
 */
export function useReviewEvidence(tasks: readonly WsTask[], enabled: boolean): ReadonlyMap<string, string> {
  const { connection } = useServices();
  const gate = useGate();
  const canRead = enabled && gate.can("review");
  const generation = gate.connection.generation;
  const [notes, setNotes] = useState<ReadonlyMap<string, string>>(new Map());
  const done = useRef(new Map<string, string>());

  const wanted = canRead
    ? tasks
        .filter((task) => EVIDENCE_STATES.includes(task.state) && !task.pendingDecision)
        .slice(0, MAX_EVIDENCE_READS)
    : [];
  const signature = wanted.map((task) => `${task.id}:${task.state}`).join("|");

  useEffect(() => {
    let live = true;
    const queue = wanted.filter((task) => !done.current.has(`${generation}:${task.id}:${task.state}`));
    const run = async () => {
      while (live) {
        const task = queue.shift();
        if (!task) return;
        try {
          const wire = await connection.api().reviewEvidence(task.id);
          if (!live) return;
          done.current.set(`${generation}:${task.id}:${task.state}`, evidenceFromReview(wire));
        } catch {
          // A conflict, a missing method or a lost connection keeps the state-derived text.
          if (!live) return;
          done.current.set(`${generation}:${task.id}:${task.state}`, "");
        }
        setNotes(snapshot(done.current, wanted, generation));
      }
    };
    setNotes(snapshot(done.current, wanted, generation));
    void Promise.all(Array.from({ length: Math.min(CONCURRENCY, queue.length) }, run));
    return () => {
      live = false;
    };
    // `signature` and `generation` capture every input of `wanted` that matters.
  }, [signature, generation, canRead]);

  return canRead ? notes : EMPTY;
}

const EMPTY: ReadonlyMap<string, string> = new Map();

function snapshot(done: ReadonlyMap<string, string>, wanted: readonly WsTask[], generation: number) {
  const notes = new Map<string, string>();
  for (const task of wanted) {
    const text = done.get(`${generation}:${task.id}:${task.state}`);
    if (text) notes.set(task.id, text);
  }
  return notes;
}
