// Review (#/review): the queue of everything that needs the user, and one candidate opened from it (`?task=`).
//
// Decisions on a candidate are made in the accepted ReviewScreen (approvals.resolve, review.request_changes,
// delivery.request, idempotency keys, stale handling), embedded without its own <main> because the app shell
// already provides one. Proposed actions and questions are decided in the queue's rows. The session is shared per
// daemon connection so an approved-but-undelivered candidate survives leaving and re-entering the view.
import { useMemo } from "react";
import type { ReviewClient } from "../../../review/client";
import { createReviewClient } from "../../../review/client";
import { createDaemonChangeRequestSource, createDaemonReviewEvidenceSource } from "../../../review/daemonSources";
import { createFixtureDaemonClient } from "../../../review/fixtureDaemon";
import { ReviewScreen } from "../../../review/ReviewScreen";
import { createReviewSession, sessionFor } from "../../../review/reviewSession";
import {
  createFixtureChangeRequestSource,
  createFixtureReviewEvidenceSource,
  type ChangeRequestSource,
  type ReviewEvidenceSource,
} from "../../../review/sources";
import type { ReviewState } from "../../../review/useReview";
import { useWorkstreamOf } from "../../../review/workstreamOf";
import { ActionStatus } from "../../action";
import { useServices, useStore } from "../../services";
import type { AttemptsLoader } from "./Attempts";
import { HoldControl } from "./HoldControl";
import { ReviewQueue } from "./ReviewQueue";
import { SixFactsRecord } from "./ReviewRecord";
import { useHolds } from "./useHolds";
import "./review-decisions.css";

export interface ReviewDecisionsViewProps {
  /** Explicit demo mode (no daemon bridge and `?fixture=1`). Never inferred from a failed request. */
  fixtureMode?: boolean;
  /** Fixture scenarios (`?evidence=`, `?changes=`); ignored outside fixture mode. */
  evidenceScenario?: string;
  changesScenario?: string;
  /** Test seams; the defaults are chosen from the boot mode. */
  client?: ReviewClient;
  evidenceSource?: ReviewEvidenceSource;
  changeRequestSource?: ChangeRequestSource;
  newKey?: () => string;
}

export function ReviewDecisionsView({
  fixtureMode = false,
  evidenceScenario,
  changesScenario,
  client: clientOverride,
  evidenceSource: evidenceOverride,
  changeRequestSource: changesOverride,
  newKey,
}: ReviewDecisionsViewProps) {
  const { connection, router } = useServices();
  const route = useStore(router);
  const holds = useHolds();
  const workstreamOf = useWorkstreamOf();
  const client = useMemo(
    () => clientOverride ?? (fixtureMode ? createFixtureDaemonClient() : createReviewClient(connection)),
    [clientOverride, fixtureMode, connection],
  );
  // The fixture client is rebuilt on every mount (its approvals come back pending), so its session is private too.
  const sessionOwner = clientOverride ?? (fixtureMode ? null : connection);
  const session = useMemo(() => (sessionOwner ? sessionFor(sessionOwner) : createReviewSession()), [sessionOwner]);
  const evidenceSource = useMemo(
    () =>
      evidenceOverride ??
      (fixtureMode
        ? createFixtureReviewEvidenceSource(evidenceScenario)
        : createDaemonReviewEvidenceSource(() => connection.api())),
    [evidenceOverride, fixtureMode, evidenceScenario, connection],
  );
  const changeRequestSource = useMemo(
    () =>
      changesOverride ??
      (fixtureMode
        ? createFixtureChangeRequestSource(changesScenario)
        : createDaemonChangeRequestSource(() => connection.api())),
    [changesOverride, fixtureMode, changesScenario, connection],
  );
  const loadAttempts = useMemo<AttemptsLoader | undefined>(
    () =>
      fixtureMode || evidenceOverride
        ? undefined
        : async (taskId, approvalId) => (await connection.api().reviewEvidence(taskId, approvalId)).attempts ?? [],
    [fixtureMode, evidenceOverride, connection],
  );
  const taskId = route.name === "global" && route.view === "review" ? route.taskId : undefined;
  const projectId = route.name === "global" && route.view === "review" ? route.projectId : undefined;

  return (
    <div className="review-decisions" data-testid="review-decisions">
      {taskId === undefined ? (
        <ReviewQueue client={client} session={session} projectId={projectId} />
      ) : (
        <ReviewScreen
          key={taskId}
          client={client}
          evidenceSource={evidenceSource}
          changeRequestSource={changeRequestSource}
          scope={{ taskId }}
          onBack={() => router.navigate({ name: "global", view: "review" })}
          newKey={newKey}
          session={session}
          workstreamOf={workstreamOf}
          hold={(state: ReviewState) =>
            holds.supported ? (
              <>
                {state.approval?.state.state === "requested" && (
                  <HoldControl taskId={state.approval.action.task_id} holds={holds} onStart={() => undefined} />
                )}
                {/* Outside the condition above: the result of a hold stays beside the control, whatever the approval does next. */}
                <ActionStatus action={holds.action} />
              </>
            ) : null
          }
          record={(state: ReviewState) => <SixFactsRecord state={state} loadAttempts={loadAttempts} />}
        />
      )}
    </div>
  );
}
