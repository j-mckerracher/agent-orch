// S6 — Diff review and approval. REAL: approvals.list, workspaces.diff,
// approvals.resolve (approve/reject only), delivery.request. DUMMY behind
// injected interfaces: ReviewEvidenceSource (D7), ChangeRequestSource (D8).
import type { ReviewClient } from "./client";
import { digestHex, deliveryKindOf } from "./binding";
import { ChangeRequestPanel } from "./ChangeRequestPanel";
import { EvidencePanel, PlaceholderTag } from "./EvidencePanel";
import type { ChangeRequestSource, ReviewEvidenceSource } from "./sources";
import { useReview, type MutationView, type ReviewScope } from "./useReview";
import "./review.css";

export interface ReviewScreenProps {
  client: ReviewClient;
  /** Inject a stable instance (module level or memoized), not a new object per render. */
  evidenceSource: ReviewEvidenceSource;
  changeRequestSource: ChangeRequestSource;
  /** Where the user came from; also narrows pending decisions. */
  scope: ReviewScope;
  onBack: (scope: ReviewScope) => void;
  newKey?: () => string;
}

function receiptTag(client: ReviewClient) {
  return <PlaceholderTag show={client.provenance === "fixture"} />;
}

function MutationStatus<T>({
  view,
  label,
  client,
  describe,
}: {
  view: MutationView<T> | undefined;
  label: string;
  client: ReviewClient;
  describe: (receipt: T) => string;
}) {
  if (!view) return null;
  if (view.phase === "pending") return <p role="status">{label}: waiting for the daemon…</p>;
  if (view.phase === "succeeded") {
    return (
      <p role="status">
        {label} receipt from daemon: {describe(view.receipt)} {receiptTag(client)}
      </p>
    );
  }
  return (
    <p role="alert">
      {label} failed ({view.kind}): {view.message}.{" "}
      {view.indeterminate
        ? "The daemon may or may not have applied it. "
        : ""}
      Refresh to reconcile before retrying; a retry reuses the same idempotency key. {receiptTag(client)}
    </p>
  );
}

export function ReviewScreen({ client, evidenceSource, changeRequestSource, scope, onBack, newKey }: ReviewScreenProps) {
  const r = useReview({ client, evidenceSource, scope, newKey });
  const { approval, task } = r;
  const back: ReviewScope = scope.taskId ? scope : { projectId: task?.contract.project_id ?? scope.projectId, taskId: scope.taskId };
  const backLabel = scope.taskId ? `Back to task ${scope.taskId}` : scope.projectId ? `Back to project ${scope.projectId}` : "Back";

  return (
    <main className="review-screen">
      {client.provenance === "fixture" && (
        <p className="fixture-daemon">
          <span className="placeholder-tag">FIXTURE DAEMON — PLACEHOLDER DATA</span> No daemon is contacted; the
          connection strip above does not describe this data.
        </p>
      )}
      <header>
        <button type="button" onClick={() => onBack(back)}>
          ← {backLabel}
        </button>
        <h1>Review candidate</h1>
        <button type="button" onClick={() => void r.refresh()} disabled={r.loading}>
          Refresh
        </button>
      </header>

      {r.listState === "loading" && <p role="status">Loading pending decisions…</p>}
      {r.listState === "error" && r.listError && (
        <p role="alert">
          Pending decisions are unavailable ({r.listError.kind}): {r.listError.message}. No data is shown instead; retry
          with Refresh.
        </p>
      )}
      {r.listState === "ready" && r.items.length === 0 && (
        <p role="status">
          No pending decisions{scope.taskId ? " for this task" : scope.projectId ? " for this project" : ""}.
          {r.hiddenCount > 0 ? ` ${r.hiddenCount} non-delivery decision(s) are not reviewed here.` : ""}
        </p>
      )}
      {r.notice && <p role="alert">{r.notice}</p>}
      {r.stale && (
        <p role="alert">
          Stale review: the task changed or the connection was interrupted since this loaded. Actions are disabled until
          you Refresh. Earlier acceptance is not reused.
        </p>
      )}

      {r.items.length > 0 && (
        <label>
          Pending decision{" "}
          <select aria-label="Pending decision" value={approval?.id ?? ""} onChange={(e) => r.select(e.target.value)}>
            {r.items.map((a) => (
              <option key={a.id} value={a.id}>
                {a.id} — task {a.action.task_id} — {a.state.state}
              </option>
            ))}
          </select>
        </label>
      )}

      {approval && (
        <section aria-label="Candidate identity" className="panel">
          <dl>
            <dt>Task</dt>
            <dd>
              {approval.action.task_id}
              {task ? ` — ${task.contract.objective} (${task.state}, v${task.version})` : " (task context unavailable)"}
            </dd>
            <dt>Approval</dt>
            <dd>
              {approval.id} — status {approval.state.state}
            </dd>
            <dt>Candidate commit</dt>
            <dd>
              <code>{approval.action.candidate_commit}</code>
            </dd>
            <dt>Action</dt>
            <dd>
              {deliveryKindOf(approval) ?? approval.action.kind.type}; parameter digest{" "}
              <code>{digestHex(approval.action.parameter_digest)}</code>
            </dd>
          </dl>
        </section>
      )}

      {approval && (
        <section aria-label="Diff" className="diff-region">
          <aside aria-label="Files">
            <h3>Files</h3>
            {r.diff.status === "ok" && (
              <ul>
                {r.files.map((f) => (
                  <li key={f.path}>
                    <button
                      type="button"
                      aria-pressed={r.selectedPath === f.path}
                      onClick={() => void r.loadPath(f.path)}
                    >
                      {f.path}
                    </button>{" "}
                    {f.status} +{f.additions} −{f.deletions}
                  </li>
                ))}
              </ul>
            )}
            {r.diff.status === "too_large" && <PathRequest onRequest={(p) => void r.loadPath(p)} />}
            {(r.diff.status === "loading" || r.diff.status === "empty" || r.diff.status === "unavailable") && (
              <p>No file list.</p>
            )}
          </aside>
          <div className="diff-view">
            <h3>Unified diff{r.selectedPath ? ` — ${r.selectedPath}` : ""}</h3>
            {r.diff.status === "loading" && <p role="status">Loading diff…</p>}
            {r.diff.status === "empty" && <p role="status">The candidate has no changes against its base.</p>}
            {r.diff.status === "unavailable" && <p role="alert">Diff unavailable: {r.diff.reason}</p>}
            {r.diff.status === "too_large" && (
              <p role="alert">
                The diff is too large for the daemon to return in one response (result too large). Open a single file by
                relative path; Accept stays disabled because the whole candidate cannot be reviewed.
              </p>
            )}
            {r.scoped?.state === "loading" && <p role="status">Loading file diff…</p>}
            {r.scoped?.state === "error" && <p role="alert">File diff unavailable: {r.scoped.error}</p>}
            {r.scoped?.note && <p>{r.scoped.note}</p>}
            {(r.scoped?.state === "ok" || (!r.scoped && r.diff.status === "ok")) && (
              <DiffText text={r.scoped?.state === "ok" ? (r.scoped.text ?? "") : r.diff.status === "ok" ? r.diff.text : ""} />
            )}
          </div>
        </section>
      )}

      {approval && (
        <>
          <EvidencePanel
            evidence={r.evidence}
            loading={r.loading && r.evidence === null}
            fixtureSource={evidenceSource.provenance === "fixture"}
          />

          <section aria-label="Decision" className="panel">
            <button
              type="button"
              disabled={r.acceptBlockers.length > 0 || r.pendingRequest}
              onClick={() => void r.decide("approve")}
            >
              Accept
            </button>{" "}
            <button
              type="button"
              disabled={r.rejectBlockers.length > 0 || r.pendingRequest}
              onClick={() => void r.decide("reject")}
            >
              Reject
            </button>
            {r.acceptBlockers.length > 0 && (
              <div role="note" className="blockers">
                <strong>Accept is disabled:</strong>
                <ul>
                  {r.acceptBlockers.map((b) => (
                    <li key={b}>{b}</li>
                  ))}
                </ul>
              </div>
            )}
            <MutationStatus
              view={r.mutation.decision}
              label="Decision"
              client={client}
              describe={(a) => `${a.id} is now ${a.state.state}. Approval does not deliver anything.`}
            />
          </section>

          <ChangeRequestPanel source={changeRequestSource} approval={approval} />

          <section aria-label="Delivery" className="panel">
            <h3>Delivery</h3>
            <p>Delivery is a separate request; approving does not deliver.</p>
            <button type="button" disabled={!r.canDeliver || r.pendingRequest} onClick={() => void r.requestDelivery()}>
              Request delivery
            </button>
            {!r.canDeliver && (
              <div role="note" className="blockers">
                <strong>Delivery is unavailable:</strong>
                <ul>
                  {r.deliveryBlockers.map((b) => (
                    <li key={b}>{b}</li>
                  ))}
                </ul>
              </div>
            )}
            <MutationStatus
              view={r.mutation.delivery}
              label="Delivery request"
              client={client}
              describe={(d) => `daemon outcome "${d.outcome}" (step ${d.step_id})`}
            />
          </section>
        </>
      )}
    </main>
  );
}

function DiffText({ text }: { text: string }) {
  return (
    <pre aria-label="Diff content" className="diff-text">
      {text.split("\n").map((line, i) => (
        <span key={i} className={line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : undefined}>
          {line}
          {"\n"}
        </span>
      ))}
    </pre>
  );
}

function PathRequest({ onRequest }: { onRequest: (path: string) => void }) {
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        const value = new FormData(e.currentTarget).get("path");
        if (typeof value === "string") onRequest(value);
      }}
    >
      <label>
        Relative path <input name="path" />
      </label>
      <button type="submit">Show file diff</button>
    </form>
  );
}
