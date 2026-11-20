/** Read-only "Activity" panel for a session: the thinking the agent's provider exposed and its tool calls, in order. */
import { ActivityLog } from "../../activity/ActivityLog";
import { ActivityStatus } from "../../activity/ActivityStatus";
import { Disclosure } from "../../theme";
import { ErrorNotice } from "../../widgets";
import { useSessionActivity } from "./useSessionActivity";

export function SessionActivity({ sessionId, running }: { sessionId: string; running: boolean }) {
  const activity = useSessionActivity(sessionId, running);
  return (
    <Disclosure summary="Activity">
      {activity.error && <ErrorNotice error={activity.error} action="Refreshing activity" />}
      {!activity.loaded ? (
        <p role="status">Loading activity…</p>
      ) : activity.items.length === 0 ? (
        <p role="status">No activity recorded</p>
      ) : (
        <ActivityLog items={activity.items} streaming={running} active={running} />
      )}
      {running && activity.live && <ActivityStatus live={activity.live} />}
      {activity.truncated && <p role="status">Thinking log hit its size limit.</p>}
    </Disclosure>
  );
}
