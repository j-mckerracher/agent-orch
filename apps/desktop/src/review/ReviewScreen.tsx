// S6 — Review one candidate: diff, evidence, request changes, Accept, Reject and delivery. REAL: approvals.list,
// workspaces.diff, approvals.resolve (approve/reject only), delivery.request. DUMMY behind injected interfaces:
// ReviewEvidenceSource (D7), ChangeRequestSource (D8). It sits inside the app shell's `<main>`, so it adds no landmark.
import { useState, type ReactNode } from "react";
import { DaemonError } from "../daemon/errors";
import { ActionStatus, useAction } from "../ui/action";
import { basename } from "../ui/format";
import { WsTabs } from "../ui/views/workstreams/WsTabs";
import { Button, Disclosure } from "../ui/theme";
import { Notice, failureText } from "../ui/widgets";
import { digestHex, deliveryKindOf } from "./binding";
import { ChangeRequestHistoryList, RequestChangesForm, useChangeRequests } from "./ChangeRequestPanel";
import { EvidencePanel } from "./EvidencePanel";
import type { ReviewClient } from "./client";
import type { ChangeRequestSource, ReviewEvidenceSource } from "./sources";
import type { ReviewSession } from "./reviewSession";
import { useReview, type MutationView, type ReviewScope, type ReviewState } from "./useReview";
import "./review.css";

export interface ReviewScreenProps {
  client: ReviewClient;
  /** Inject a stable instance (module level or memoized), not a new object per render. */
  evidenceSource: ReviewEvidenceSource;
  changeRequestSource: ChangeRequestSource;
  /** The candidate's task; narrows the pending decisions to it. */
  scope: ReviewScope;
  /** Back to the queue. */
  onBack: () => void;
  newKey?: () => string;
  /** Under Technical details: the six facts, attempts and audit transitions of the selected candidate. */
  record?: (state: ReviewState) => ReactNode;
  /** The Hold control, beside Reject. */
  hold?: (state: ReviewState) => ReactNode;
  /** Workstream title for a task; null or absent leaves the workstream out of the facts. */
  workstreamOf?: (taskId: string) => string | null;
  /** Keeps resolved decisions and receipts across remounts (the Review view shares one per connection). */
  session?: ReviewSession;
}

/**
 * The daemon's answer to a decision or a delivery. It lives in the review session, so it stays beside the controls until
 * the next request for that approval, even after the view is left and opened again, so it is not a useAction.
 */
function MutationStatus<T>({
  view,
  label,
  describe,
}: {
  view: MutationView<T> | undefined;
  label: string;
  describe: (receipt: T) => string;
}) {
  if (!view) return null;
  if (view.phase === "pending") return <Notice tone="info" role="status">{label}: waiting for the daemon…</Notice>;
  if (view.phase === "succeeded") {
    return (
      <Notice tone="ok" role="status">
        {label} receipt from daemon: {describe(view.receipt)}
      </Notice>
    );
  }
  return (
    <Notice tone="bad" role="alert">
      {failureText(label, new DaemonError(view.kind, view.message))} Refresh before trying again.
    </Notice>
  );
}

function checksText(state: ReviewState): string {
  const evidence = state.evidence;
  if (evidence === null) return "checks loading";
  if (evidence.state !== "available") return "checks not available";
  const outcome = evidence.evidence.verification.outcome;
  return outcome === "passed" ? "checks passed" : outcome === "failed" ? "checks failed" : "checks outcome unknown";
}

export function ReviewScreen({
  client,
  evidenceSource,
  changeRequestSource,
  scope,
  onBack,
  newKey,
  record,
  hold,
  workstreamOf,
  session,
}: ReviewScreenProps) {
  const r = useReview({ client, evidenceSource, scope, newKey, session });
  const { approval, task } = r;
  const requests = useChangeRequests(changeRequestSource, approval);
  const refreshing = useAction();
  // Which decision button was pressed: the session keeps one decision per approval, not per button.
  const [chosen, setChosen] = useState<"approve" | "reject" | null>(null);
  const deciding = (kind: "approve" | "reject") => r.mutation.decision?.phase === "pending" && chosen === kind;
  const delivering = r.mutation.delivery?.phase === "pending";
  const approver = client.provenance === "fixture" || r.connection.approver;
  const fixture = client.provenance === "fixture";
  const accepted = approval?.state.state === "approved";
  const workstream = approval ? workstreamOf?.(approval.action.task_id) : null;

  const facts = approval
    ? [
        task ? basename(task.contract.project_id) : null,
        workstream ?? null,
        `commit ${approval.action.candidate_commit.slice(0, 7)}`,
        checksText(r),
      ].filter((fact): fact is string => fact !== null)
    : [];

  const changes = (
    <section aria-label="Diff" className="diff-region">
      <aside aria-label="Files">
        {r.diff.status === "ok" && (
          <ul>
            {r.files.map((f) => (
              <li key={f.path}>
                <button type="button" aria-pressed={r.selectedPath === f.path} onClick={() => void r.loadPath(f.path)}>
                  {f.path}
                </button>{" "}
                {f.status} +{f.additions} −{f.deletions}
              </li>
            ))}
          </ul>
        )}
        {r.diff.status === "too_large" && <PathRequest onRequest={(p) => void r.loadPath(p)} />}
        {(r.diff.status === "loading" || r.diff.status === "empty" || r.diff.status === "unavailable") && <p>No file list.</p>}
      </aside>
      <div className="diff-view">
        {r.selectedPath && <h3>{r.selectedPath}</h3>}
        {r.diff.status === "loading" && <p role="status">Loading diff…</p>}
        {r.diff.status === "empty" && <p role="status">The candidate has no changes against its base.</p>}
        {r.diff.status === "unavailable" && <p role="alert">Diff unavailable: {r.diff.reason}</p>}
        {r.diff.status === "too_large" && (
          <p role="alert">
            The diff is too large for the daemon to return in one response (result too large). Open a single file by relative
            path; Accept stays disabled because the whole candidate cannot be reviewed.
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
  );

  return (
    <div className="review-screen" data-testid="review-candidate">
      <div className="rv-back">
        <Button variant="quiet" onClick={onBack}>
          ← Review
        </Button>
        <Button
          variant="quiet"
          pending={refreshing.pending}
          disabled={r.loading && !refreshing.pending}
          onClick={() =>
            void refreshing.run("Refreshing…", async () => {
              if (!(await r.refresh())) throw new Error("The pending decisions could not be read, so nothing was refreshed. Try Refresh again.");
              return { kind: "saved", text: "Candidate refreshed from the daemon." };
            })
          }
        >
          Refresh
        </Button>
        <ActionStatus action={refreshing} />
      </div>

      {!approval && <h1>Review changes</h1>}
      {r.listState === "loading" && <p role="status">Loading pending decisions…</p>}
      {r.listState === "error" && r.listError && (
        <p role="alert">
          Pending decisions are unavailable ({r.listError.kind}): {r.listError.message}. No data is shown instead; retry with
          Refresh.
        </p>
      )}
      {r.listState === "ready" && r.items.length === 0 && <p role="status">This candidate is no longer waiting for a decision.</p>}
      {r.notice && <p role="alert">{r.notice}</p>}
      {r.stale && (
        <p role="alert">
          Stale review: the task changed or the connection was interrupted since this loaded. Actions are disabled until you
          Refresh. Earlier acceptance is not reused.
        </p>
      )}

      {approval && (
        <>
          <header className="rv-head">
            <div className="rv-title">
              <h1>Review changes: {task?.contract.objective ?? `task ${approval.action.task_id}`}</h1>
              <p className="rv-sub">{facts.join(" · ")}</p>
            </div>
            <div className="rv-actions">
              {hold?.(r)}
              <Button
                disabled={r.rejectBlockers.length > 0 || (r.pendingRequest && !deciding("reject"))}
                pending={deciding("reject")}
                onClick={() => {
                  setChosen("reject");
                  void r.decide("reject");
                }}
              >
                Reject
              </Button>
              <Button
                variant="primary"
                disabled={r.acceptBlockers.length > 0 || (r.pendingRequest && !deciding("approve"))}
                pending={deciding("approve")}
                onClick={() => {
                  setChosen("approve");
                  void r.decide("approve");
                }}
              >
                Accept
              </Button>
            </div>
          </header>
          {r.acceptBlockers.length > 0 && !accepted && (
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
            describe={(a) => `${a.id} is now ${a.state.state}. Approval does not deliver anything.`}
          />

          <WsTabs
            label="Candidate"
            tabs={[
              { id: "changes", label: `Changes${r.diff.status === "ok" ? ` ${r.files.length}` : ""}`, content: changes },
              {
                id: "evidence",
                label: "Evidence",
                content: <EvidencePanel evidence={r.evidence} loading={r.loading && r.evidence === null} />,
              },
              {
                id: "request",
                label: "Request changes",
                content: (
                  <RequestChangesForm source={changeRequestSource} requests={requests} approval={approval} approver={approver} />
                ),
              },
              { id: "history", label: "History", content: <ChangeRequestHistoryList requests={requests} /> },
            ]}
          />

          <section aria-label="Delivery" className="rv-delivery">
            <p>
              After accepting: <strong>Request delivery</strong> (separate step, opens the PR)
            </p>
            <Button
              variant={accepted ? "primary" : "secondary"}
              disabled={!r.canDeliver || (r.pendingRequest && !delivering)}
              pending={delivering}
              onClick={() => void r.requestDelivery()}
            >
              Request delivery
            </Button>
            {accepted && !r.canDeliver && (
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
              describe={(d) => `daemon outcome "${d.outcome}" (step ${d.step_id})`}
            />
          </section>

          <Disclosure summary="Technical details" className="rv-technical">
            <section aria-label="Candidate identity">
              <dl>
                <dt>Project</dt>
                <dd>{task?.contract.project_id ?? "not available"}</dd>
                <dt>Workstream</dt>
                <dd>{workstream ?? "not available"}</dd>
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
            {fixture && (
              <p className="rd-note" data-testid="fixture-note">
                Fixture data: no daemon is contacted, so every value here is placeholder data. {changeRequestSource.provenance === "fixture" && changeRequestSource.labels.description}
              </p>
            )}
            {record?.(r)}
          </Disclosure>
        </>
      )}
    </div>
  );
}

function DiffText({ text }: { text: string }) {
  return (
    <pre aria-label="Diff content" role="region" tabIndex={0} className="diff-text">
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
      className="rv-path"
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
