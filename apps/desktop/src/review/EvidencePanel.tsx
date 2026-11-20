import type { ReviewEvidenceResult } from "./sources";

/** D7: complete candidate, action and verification evidence preview. */
export function EvidencePanel({ evidence, loading }: { evidence: ReviewEvidenceResult | null; loading: boolean }) {
  return (
    <section aria-label="Action parameters and verification evidence" className="rv-panel">
      {loading || evidence === null ? (
        <p role="status">Loading evidence…</p>
      ) : evidence.state !== "available" ? (
        <p role="status">
          Evidence {evidence.state}: {evidence.reason}
        </p>
      ) : (
        <dl>
          <dt>Task / approval</dt>
          <dd>
            {evidence.evidence.taskId} / {evidence.evidence.approvalId}
          </dd>
          <dt>Candidate commit (base)</dt>
          <dd>
            <code>{evidence.evidence.candidate.commit}</code> (<code>{evidence.evidence.candidate.baseCommit}</code>)
          </dd>
          <dt>Workspace</dt>
          <dd>{evidence.evidence.workspaceId}</dd>
          <dt>Action</dt>
          <dd>
            {evidence.evidence.action.label} → {evidence.evidence.action.target ?? "no target"}
          </dd>
          <dt>Parameters</dt>
          <dd>
            <ul>
              {Object.entries(evidence.evidence.action.parameters).map(([k, v]) => (
                <li key={k}>
                  {k}: <code>{v}</code>
                </li>
              ))}
            </ul>
            Digest: <code>{evidence.evidence.action.digestHex}</code>
          </dd>
          <dt>Verification {evidence.evidence.verification.runId}</dt>
          <dd>
            {evidence.evidence.verification.outcome}: {evidence.evidence.verification.summary}
            <ul>
              {evidence.evidence.verification.checks.map((c) => (
                <li key={c.name}>
                  {c.name}: {c.outcome}
                </li>
              ))}
            </ul>
          </dd>
        </dl>
      )}
    </section>
  );
}
