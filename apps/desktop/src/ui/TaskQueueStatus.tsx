import { useState } from "react";
import { agentDisplayName } from "../daemon/agentDisplayNames";
import { QUEUE_RECHECK_MS, useQueueActivity } from "./QueueWaitNote";
import { useGate, useServices } from "./services";
import { Button } from "./theme";
import { Notice } from "./widgets";

/** Queue diagnostics are observations, not proof of an agent launch or provider quota. */
export function TaskQueueStatus({ taskId, agent }: { taskId: string; agent: string }) {
  const { data } = useServices();
  const gate = useGate();
  const [revision, setRevision] = useState(0);
  // A manual refresh changes the key and shows "Checking"; automatic re-reads keep the last answer on screen.
  const activity = useQueueActivity(`task-queue:${taskId}:${revision}`);
  const live = activity.status !== "loading" && activity.provenance.kind === "daemon";
  const waiting = activity.status === "ok" && live
    ? activity.data.queued.find((entry) => entry.taskId === taskId)?.waiting
    : null;
  let detail: string;
  let nextStep: string | undefined;
  if (!gate.connected) {
    detail = "Queue status unavailable while disconnected. The task's current launch status is unknown.";
    nextStep = "Reconnect to the daemon to check why this task is waiting.";
  } else if (activity.status === "loading") {
    detail = "Checking why this task is queued...";
  } else if (!live) {
    detail = "Live queue diagnostics are unavailable for this task.";
  } else if (activity.status !== "ok") {
    detail = `Queue status unavailable: ${activity.reason}`;
    nextStep = "Refresh queue status to try again. If it remains unavailable, check the daemon connection and log.";
  } else if (!waiting) {
    detail = "This task is not in the latest queue snapshot. Its task status may have changed.";
    nextStep = "Refresh queue status to reload the task. This is not confirmation that an agent launched.";
  } else {
    detail = waiting.detail;
    nextStep = waiting.nextStep;
  }

  return (
    <section aria-label="Queue status" className="ps-section">
      <h2>Waiting to start</h2>
      <Notice tone={activity.status === "loading" && gate.connected ? "info" : "warn"} role="status">
        {detail}
      </Notice>
      {nextStep && <p><strong>Next step:</strong> {nextStep}</p>}
      <p className="muted">
        Selected agent: {agentDisplayName(agent)}. No provider quota check is available from queue status.
      </p>
      <div className="ps-actions">
        <Button
          disabled={!gate.connected || activity.status === "loading"}
          onClick={() => {
            setRevision((value) => value + 1);
            void data.watchTask(taskId);
          }}
        >
          Refresh queue status
        </Button>
        {activity.status === "ok" && live && gate.connected && (
          <span className="muted">
            Last checked: <time dateTime={activity.observedAt}>{new Date(activity.observedAt).toLocaleTimeString()}</time>.
            {" "}Checks again every {QUEUE_RECHECK_MS / 1000} seconds.
          </span>
        )}
      </div>
    </section>
  );
}
