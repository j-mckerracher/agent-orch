// Frontend boundary interfaces for the two unavailable read/write contracts
// (D7 complete review evidence, D8 request changes). A daemon-backed adapter
// (planned M5-02c) implements the same interfaces and replaces the fixture
// adapters at the ReviewScreen boundary without any component change.
import changeRequestFixture from "../fixtures/ui/change-request.json";
import reviewEvidenceFixture from "../fixtures/ui/review-evidence.json";

export type Provenance = "daemon" | "fixture";

export interface ReviewEvidence {
  provenance: Provenance;
  taskId: string;
  approvalId: string;
  workspaceId: string;
  candidate: { commit: string; baseCommit: string };
  action: {
    kind: string;
    label: string;
    target: string | null;
    parameters: Record<string, string>;
    /** Lowercase hex of the approval's parameter digest this evidence was built for. */
    digestHex: string;
  };
  verification: {
    runId: string;
    commit: string;
    outcome: "passed" | "failed" | "unknown";
    summary: string;
    checks: { name: string; outcome: "passed" | "failed" | "unknown" }[];
  };
}

export type ReviewEvidenceResult =
  | { state: "available"; evidence: ReviewEvidence }
  | { state: "missing" | "unavailable" | "error"; reason: string; provenance: Provenance };

export interface ReviewEvidenceRequest {
  taskId: string;
  approvalId: string;
  candidateCommit: string;
  digestHex: string;
}

export interface ReviewEvidenceSource {
  readonly provenance: Provenance;
  load(request: ReviewEvidenceRequest): Promise<ReviewEvidenceResult>;
}

export interface ChangeRequestReceipt {
  provenance: Provenance;
  requestId: string;
  taskId: string;
  candidateCommit: string;
  feedback: string;
  resultingState: string;
  submittedAt: string;
}

export type ChangeRequestHistory =
  | { state: "available"; items: ChangeRequestReceipt[] }
  | { state: "empty" | "unavailable" | "error"; reason: string };

export interface ChangeRequestInput {
  taskId: string;
  candidateCommit: string;
  feedback: string;
}

export interface ChangeRequestSource {
  readonly provenance: Provenance;
  loadHistory(taskId: string): Promise<ChangeRequestHistory>;
  /** Rejects with Error when the contract is unavailable. */
  submit(input: ChangeRequestInput): Promise<ChangeRequestReceipt>;
}

// ---- Fixture adapters -------------------------------------------------------

type EvidenceFixture = {
  default: string;
  scenarios: Record<
    string,
    { delayMs?: number; result: { state: string; reason?: string; evidence?: Omit<ReviewEvidence, "provenance"> } }
  >;
};

export function createFixtureReviewEvidenceSource(
  scenario?: string,
  data: EvidenceFixture = reviewEvidenceFixture as EvidenceFixture,
): ReviewEvidenceSource {
  const entry = data.scenarios[scenario ?? data.default] ?? data.scenarios[data.default];
  return {
    provenance: "fixture",
    async load() {
      if (entry.delayMs) await new Promise((resolve) => setTimeout(resolve, entry.delayMs));
      const { result } = entry;
      // Provenance is forced here so fixture JSON can never claim to be daemon evidence.
      if (result.state === "available" && result.evidence) {
        return { state: "available", evidence: { ...result.evidence, provenance: "fixture" } };
      }
      return {
        state: result.state as "missing" | "unavailable" | "error",
        reason: result.reason ?? "No reason supplied by fixture.",
        provenance: "fixture",
      };
    },
  };
}

type ChangeRequestFixture = {
  default: string;
  scenarios: Record<
    string,
    {
      history: { requestId: string; feedback: string; resultingState: string; submittedAt: string }[];
      historyError?: string;
      submitError?: string;
      resultingState?: string;
    }
  >;
};

export function createFixtureChangeRequestSource(
  scenario?: string,
  data: ChangeRequestFixture = changeRequestFixture as ChangeRequestFixture,
  now: () => Date = () => new Date(),
): ChangeRequestSource {
  const entry = data.scenarios[scenario ?? data.default] ?? data.scenarios[data.default];
  const submitted: ChangeRequestReceipt[] = [];
  return {
    provenance: "fixture",
    async loadHistory(taskId) {
      if (entry.historyError) return { state: "error", reason: entry.historyError };
      const items: ChangeRequestReceipt[] = [
        ...entry.history.map((h) => ({
          ...h,
          provenance: "fixture" as const,
          taskId,
          candidateCommit: "",
          feedback: h.feedback,
        })),
        ...submitted,
      ];
      return items.length === 0
        ? { state: "empty", reason: "No change requests recorded in the fixture dataset." }
        : { state: "available", items };
    },
    async submit({ taskId, candidateCommit, feedback }) {
      if (entry.submitError) throw new Error(entry.submitError);
      const receipt: ChangeRequestReceipt = {
        provenance: "fixture",
        requestId: `fixture-change-request-${submitted.length + 1}`,
        taskId,
        candidateCommit,
        feedback,
        resultingState: entry.resultingState ?? "changes requested (simulated)",
        submittedAt: now().toISOString(),
      };
      submitted.push(receipt);
      return receipt;
    },
  };
}
