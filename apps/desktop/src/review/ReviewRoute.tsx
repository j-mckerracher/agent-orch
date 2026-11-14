// Mounts S6 inside the app shell. The mode is explicit and never changes after a
// failed request. With `fixture=1` the fixture daemon, D7 and D8 adapters are used
// (the `evidence=` and `changes=` query scenarios apply), but only when no daemon
// bridge is bound (browser fixture demo): a native or bridged app ignores the
// query and can never enter fixture mode. Otherwise every source
// is the real daemon: the review client plus the D7/D8 daemon adapters.
import { useMemo } from "react";
import { useServices } from "../ui/services";
import { createReviewClient } from "./client";
import { createDaemonChangeRequestSource, createDaemonReviewEvidenceSource } from "./daemonSources";
import { createFixtureDaemonClient } from "./fixtureDaemon";
import { ReviewScreen } from "./ReviewScreen";
import { createFixtureChangeRequestSource, createFixtureReviewEvidenceSource } from "./sources";

export interface ReviewRouteProps {
  projectId: string;
  taskId: string | null;
  query?: string;
}

export function ReviewRoute({ projectId, taskId, query }: ReviewRouteProps) {
  const { connection, router, bridgeBound } = useServices();
  const params = useMemo(() => new URLSearchParams(query ?? ""), [query]);
  const fixtureMode = !bridgeBound && params.get("fixture") === "1";
  const client = useMemo(
    () => (fixtureMode ? createFixtureDaemonClient() : createReviewClient(connection)),
    [fixtureMode, connection],
  );
  const evidenceScenario = params.get("evidence") ?? undefined;
  const changesScenario = params.get("changes") ?? undefined;
  const evidenceSource = useMemo(
    () =>
      fixtureMode
        ? createFixtureReviewEvidenceSource(evidenceScenario)
        : createDaemonReviewEvidenceSource(() => connection.api()),
    [fixtureMode, evidenceScenario, connection],
  );
  const changeRequestSource = useMemo(
    () =>
      fixtureMode
        ? createFixtureChangeRequestSource(changesScenario)
        : createDaemonChangeRequestSource(() => connection.api()),
    [fixtureMode, changesScenario, connection],
  );
  const scope = useMemo(() => ({ projectId, taskId: taskId ?? undefined }), [projectId, taskId]);

  return (
    <ReviewScreen
      client={client}
      evidenceSource={evidenceSource}
      changeRequestSource={changeRequestSource}
      scope={scope}
      onBack={(back) =>
        router.navigate(
          back.taskId
            ? { name: "task", projectId: back.projectId ?? projectId, taskId: back.taskId }
            : { name: "project", projectId: back.projectId ?? projectId },
        )
      }
    />
  );
}
