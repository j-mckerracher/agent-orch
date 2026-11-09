// Mounts S6 inside the app shell. The mode is explicit and never changes after a
// failed request: the real client unless the route query carries `fixture=1`.
import { useMemo } from "react";
import { useServices } from "../ui/services";
import { createReviewClient } from "./client";
import { createFixtureDaemonClient } from "./fixtureDaemon";
import { ReviewScreen } from "./ReviewScreen";
import { createFixtureChangeRequestSource, createFixtureReviewEvidenceSource } from "./sources";

export interface ReviewRouteProps {
  projectId: string;
  taskId: string | null;
  query?: string;
}

export function ReviewRoute({ projectId, taskId, query }: ReviewRouteProps) {
  const { connection, router } = useServices();
  const params = useMemo(() => new URLSearchParams(query ?? ""), [query]);
  const fixtureMode = params.get("fixture") === "1";
  const client = useMemo(
    () => (fixtureMode ? createFixtureDaemonClient() : createReviewClient(connection)),
    [fixtureMode, connection],
  );
  const evidenceSource = useMemo(
    () => createFixtureReviewEvidenceSource(params.get("evidence") ?? undefined),
    [params],
  );
  const changeRequestSource = useMemo(
    () => createFixtureChangeRequestSource(params.get("changes") ?? undefined),
    [params],
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
