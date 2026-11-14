// "Six separate facts" panel. Each fact is derived from what the daemon actually returned
// (tasks.get, review.evidence, the approval, a delivery receipt). A fact the payload does
// not carry reads "Not available" and is never drawn as success: one passing fact must not
// imply another.
import type { Approval, DeliveryResult, TaskRecord } from "../../../daemon/types";
import type { MutationView } from "../../../review/useReview";
import type { ReviewEvidenceResult } from "../../../review/sources";
import { Badge, Pill, type Tone } from "../../theme";

export interface Fact {
  id: string;
  label: string;
  tone: Tone;
  /** Short status word shown in the pill. */
  status: string;
  detail: string;
  /** Where the fact comes from; "none" facts are never available from the daemon today. */
  source: string;
}

export interface FactInputs {
  approval: Approval;
  task: TaskRecord | null;
  evidence: ReviewEvidenceResult | null;
  delivery: MutationView<DeliveryResult> | undefined;
  /** `evidenceBlockers` for the selected candidate; a pass that the Accept gate rejects is not drawn as success. */
  evidenceProblems?: readonly string[];
}

const NOT_AVAILABLE = "Not available";

export function deriveFacts({ approval, task, evidence, delivery, evidenceProblems = [] }: FactInputs): Fact[] {
  const execution: Fact = (() => {
    const base = { id: "execution", label: "Execution finished", source: "tasks.get" };
    if (!task) return { ...base, tone: "neutral", status: NOT_AVAILABLE, detail: "The task record could not be read." };
    switch (task.state) {
      case "Succeeded":
        return { ...base, tone: "ok", status: "Finished", detail: `Task state ${task.state}.` };
      case "VerificationFailed":
      case "Failed":
        return { ...base, tone: "bad", status: "Ended without success", detail: `Task state ${task.state}.` };
      case "Cancelled":
        return { ...base, tone: "warn", status: "Cancelled", detail: `Task state ${task.state}.` };
      default:
        return { ...base, tone: "info", status: "Not finished", detail: `Task state ${task.state}.` };
    }
  })();

  const claim: Fact = {
    id: "claim",
    label: "Agent claims completion",
    tone: "neutral",
    status: NOT_AVAILABLE,
    detail: "The daemon does not record a completion claim from the agent.",
    source: "none",
  };

  const checks: Fact = (() => {
    const base = { id: "checks", label: "Automated checks", source: "review.evidence" };
    if (evidence === null) return { ...base, tone: "neutral", status: "Loading", detail: "Reading the review evidence…" };
    if (evidence.state !== "available") {
      return { ...base, tone: "neutral", status: NOT_AVAILABLE, detail: `Evidence ${evidence.state}: ${evidence.reason}` };
    }
    const { verification } = evidence.evidence;
    const count = verification.checks.length;
    const detail = `Verification run ${verification.runId}${count > 0 ? `, ${count} check${count === 1 ? "" : "s"} listed` : ""}.`;
    if (verification.outcome === "passed") {
      if (evidenceProblems.length > 0) {
        return {
          ...base,
          tone: "warn",
          status: "Passed, not bound to this candidate",
          detail: `${detail} It does not authorize this candidate: ${evidenceProblems.join(" ")}`,
        };
      }
      return { ...base, tone: "ok", status: "Passed", detail };
    }
    if (verification.outcome === "failed") return { ...base, tone: "bad", status: "Failed", detail };
    return { ...base, tone: "warn", status: "Outcome unknown", detail };
  })();

  const review: Fact = {
    id: "review",
    label: "Independent review",
    tone: "neutral",
    status: NOT_AVAILABLE,
    detail: "The daemon has no independent reviewer record; your own decision below is separate from it.",
    source: "none",
  };

  const approved: Fact = (() => {
    const base = { id: "approved", label: "Delivery approved", source: "approvals.list" };
    const state = approval.state;
    switch (state.state) {
      case "approved":
        return { ...base, tone: "ok", status: "Approved", detail: `Approved by ${state.by.kind} ${state.by.id}. Approval does not deliver anything.` };
      case "rejected":
        return { ...base, tone: "bad", status: "Rejected", detail: "This candidate was rejected." };
      case "superseded":
        return { ...base, tone: "warn", status: "Superseded", detail: "A newer approval replaced this one." };
      default:
        return { ...base, tone: "info", status: "Awaiting your decision", detail: "No approval has been given." };
    }
  })();

  const integrated: Fact = (() => {
    const base = { id: "integrated", label: "Changes integrated", source: "delivery.request" };
    if (delivery?.phase === "succeeded") {
      return {
        ...base,
        tone: "info",
        status: "Request recorded",
        detail: `The daemon returned outcome "${delivery.receipt.outcome}" for the delivery request made in this session. That is a receipt, not proof of integration.`,
      };
    }
    return {
      ...base,
      tone: "neutral",
      status: NOT_AVAILABLE,
      detail: "The daemon does not report whether the changes were integrated.",
    };
  })();

  return [execution, claim, checks, review, approved, integrated];
}

export function SixFacts({ facts, fixture }: { facts: Fact[]; fixture: boolean }) {
  return (
    <section aria-labelledby="rd-facts-title" className="rd-card rd-facts">
      <h3 id="rd-facts-title">
        Six separate facts {fixture && <Badge tone="placeholder">PLACEHOLDER DATA</Badge>}
      </h3>
      <p className="rd-note">Each fact stands alone. A passing fact never implies another one.</p>
      <ol className="rd-fact-list">
        {facts.map((fact) => (
          <li key={fact.id} className="rd-fact" data-fact={fact.id}>
            <span className="rd-fact-label">{fact.label}</span>
            <Pill tone={fact.tone}>{fact.status}</Pill>
            <span className="rd-fact-detail">{fact.detail}</span>
            <span className="rd-fact-source">Source: {fact.source}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}
