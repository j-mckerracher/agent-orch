// Review & Decisions (#/review): the global decision queue plus the accepted S6 review flow.
//
// Decisions are NOT made here. The detail pane is the accepted ReviewScreen (approvals.resolve,
// review.request_changes, delivery.request, idempotency keys, stale handling), embedded without
// its own <main> because the app shell already provides one. The queue lists every pending
// approval from approvals.list across projects. Priority and latest response time come from the
// D16 source and are PLACEHOLDER in fixture mode and "not available" from the daemon.
import { useMemo } from "react";
import type { ReviewClient } from "../../../review/client";
import { createReviewClient } from "../../../review/client";
import { createDaemonChangeRequestSource, createDaemonReviewEvidenceSource } from "../../../review/daemonSources";
import { createFixtureDaemonClient } from "../../../review/fixtureDaemon";
import { ReviewScreen, type ReviewEmbedSlots } from "../../../review/ReviewScreen";
import {
  createFixtureChangeRequestSource,
  createFixtureReviewEvidenceSource,
  type ChangeRequestSource,
  type ReviewEvidenceSource,
} from "../../../review/sources";
import type { ReviewState } from "../../../review/useReview";
import { Badge, Button, Pill } from "../../theme";
import { useServices, useSurface } from "../../services";
import { DecisionBehavior } from "./DecisionBehavior";
import { DecisionQueue, type QueueMeta } from "./DecisionQueue";
import { SixFacts, deriveFacts } from "./SixFacts";
import { indexMeta } from "./queue";
import "./review-decisions.css";

export interface ReviewDecisionsViewProps {
  /** Explicit demo mode (no daemon bridge and `?fixture=1`). Never inferred from a failed request. */
  fixtureMode?: boolean;
  /** Test seams; the defaults are chosen from the boot mode exactly as the S6 route does. */
  client?: ReviewClient;
  evidenceSource?: ReviewEvidenceSource;
  changeRequestSource?: ChangeRequestSource;
  newKey?: () => string;
}

const ALL_SCOPE = {};

function ScopeRow() {
  return (
    <div className="rd-scope" role="group" aria-label="Queue scope">
      <Pill tone="info" hideMark>
        All workstreams · all projects
      </Pill>
      <span className="rd-scope-item">
        <Button variant="quiet" disabled aria-describedby="rd-scope-owner">
          All owners
        </Button>
        <span id="rd-scope-owner" className="rd-reason">
          The daemon records no decision owner.
        </span>
      </span>
      <span className="rd-scope-item">
        <Button variant="quiet" disabled aria-describedby="rd-scope-sort">
          By useful response time
        </Button>
        <span id="rd-scope-sort" className="rd-reason">
          No response-time data from the daemon; the queue keeps the daemon's order.
        </span>
      </span>
      <span className="rd-scope-item">
        <Badge>Active silences: not available</Badge>
      </span>
    </div>
  );
}

export function ReviewDecisionsView({
  fixtureMode = false,
  client: clientOverride,
  evidenceSource: evidenceOverride,
  changeRequestSource: changesOverride,
  newKey,
}: ReviewDecisionsViewProps) {
  const { connection, surfaces } = useServices();
  const client = useMemo(
    () => clientOverride ?? (fixtureMode ? createFixtureDaemonClient() : createReviewClient(connection)),
    [clientOverride, fixtureMode, connection],
  );
  const evidenceSource = useMemo(
    () =>
      evidenceOverride ??
      (fixtureMode ? createFixtureReviewEvidenceSource() : createDaemonReviewEvidenceSource(() => connection.api())),
    [evidenceOverride, fixtureMode, connection],
  );
  const changeRequestSource = useMemo(
    () =>
      changesOverride ??
      (fixtureMode ? createFixtureChangeRequestSource() : createDaemonChangeRequestSource(() => connection.api())),
    [changesOverride, fixtureMode, connection],
  );

  const decisions = useSurface(() => surfaces.decisions.queue(), "review-decisions", surfaces.decisions.provenance);
  const fixture = client.provenance === "fixture";
  const meta: QueueMeta = useMemo(
    () => ({
      byId: decisions.status === "ok" ? indexMeta(decisions.data.items) : new Map(),
      fixture: decisions.status === "ok" && decisions.provenance.kind === "fixture",
    }),
    [decisions],
  );

  const slots: ReviewEmbedSlots = useMemo(
    () => ({
      header: (state: ReviewState) => (
        <>
          <Heading state={state} />
          <ScopeRow />
        </>
      ),
      queue: (state: ReviewState) => <DecisionQueue state={state} meta={meta} />,
      facts: (state: ReviewState) =>
        state.approval && (
          <SixFacts
            fixture={fixture}
            facts={deriveFacts({
              approval: state.approval,
              task: state.task,
              evidence: state.evidence,
              delivery: state.mutation.delivery,
            })}
          />
        ),
      behavior: (state: ReviewState) => <DecisionBehavior task={state.task} fixture={fixture} />,
    }),
    [meta, fixture],
  );

  return (
    <div className="review-decisions" data-testid="review-decisions">
      <ReviewScreen
        client={client}
        evidenceSource={evidenceSource}
        changeRequestSource={changeRequestSource}
        scope={ALL_SCOPE}
        onBack={() => undefined}
        newKey={newKey}
        embedded={slots}
      />
    </div>
  );
}

function Heading({ state }: { state: ReviewState }) {
  const waiting = state.items.filter((item) => item.state.state === "requested").length;
  return (
    <header className="rd-heading">
      <div>
        <p className="rd-eyebrow">Unified human decisions</p>
        <h1>Review &amp; Decisions</h1>
      </div>
      {state.listState === "ready" ? (
        <Badge tone={waiting > 0 ? "info" : "neutral"}>
          {waiting} independent decision{waiting === 1 ? "" : "s"}
        </Badge>
      ) : (
        <Badge>Decisions: loading</Badge>
      )}
    </header>
  );
}
