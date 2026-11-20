import type { ReactNode } from "react";
import { agentDisplayName } from "../../../daemon/agentDisplayNames";
import type { WakeRecord } from "../../../sources/surfaces/types";
import type { Route } from "../../router";
import { NavLink } from "../../shell/NavLink";
import { Timestamp } from "../../chat/Timestamp";
import { CopyButton } from "../../theme";
import { WAKE_LABEL, isQuiet, isTaskEvent, quietLine, skipText } from "./monitoringModel";

/** The page of a task, or null when the app does not hold that task. */
export type TaskRoute = (taskId: string) => Route | null;

/** The agent an automatic turn ran with, named in the label when the turn is known. */
export interface WakeRuntime {
  name: string;
  version: string;
}

/**
 * The visible words that say an update is the orchestrator's own doing, so it is never read as an answer. With the
 * turn's runtime it also names the agent that ran the update.
 */
export function WakeLabel({ runtime }: { runtime?: WakeRuntime }) {
  const text = runtime ? `${WAKE_LABEL} · ${agentDisplayName(runtime.name)} ${runtime.version}` : WAKE_LABEL;
  return <span className="ws-wake-label">{text}</span>;
}

/** The opening entry of an automatic turn: the event lines, inside the update, never a "You" bubble. */
export function WakeLines({ text, at, runtime }: { text: string; at: string; runtime?: WakeRuntime }) {
  return (
    <div className="ws-wake ws-wake-item">
      <div className="ws-wake-body">
        <WakeLabel runtime={runtime} />
        <Timestamp at={at} />
        <p className="ws-wake-lines">{text}</p>
      </div>
      <CopyButton text={text} label="Copy update" />
    </div>
  );
}

/** What the record's state adds to its notice, in words. A deferred or waiting record adds nothing. */
function stateNote(record: WakeRecord): string | null {
  if (record.state === "skipped") return skipText(record.skipReason);
  if (record.state === "failed") return "The automatic reply failed.";
  if (record.state === "handled" && record.outcome === "unanswered") return "No automatic explanation was given. Open the task.";
  return null;
}

/**
 * One deterministic notice. It is written before any model turn starts, so a failed or skipped turn never hides it.
 * A task event also links to its task, when the app holds that task.
 */
export function WakeNotice({ record, taskRoute }: { record: WakeRecord; taskRoute?: TaskRoute }) {
  const note = stateNote(record);
  const route = taskRoute && isTaskEvent(record) ? taskRoute(record.subject) : null;
  return (
    <div className="ws-wake ws-wake-item">
      <div role="status" className="ws-wake-body" data-state={record.state}>
        <WakeLabel />
        <Timestamp at={record.createdAt} />
        <p className="ws-wake-lines">{record.notice}</p>
        {note && <p className="ws-wake-note">{note}</p>}
        {route && (
          <p className="ws-wake-link">
            <NavLink to={route} className="ws-chip-link">
              Open task
            </NavLink>
          </p>
        )}
      </div>
      <CopyButton text={note ? `${record.notice}\n${note}` : record.notice} label="Copy update" />
    </div>
  );
}

export function QuietLine({ count }: { count: number }) {
  return (
    <p role="status" className="ws-wake ws-wake-quiet">
      {quietLine(count)}
    </p>
  );
}

/** Records in order; a run of quiet ones becomes one collapsed line. */
export function WakeRecords({ records, taskRoute }: { records: readonly WakeRecord[]; taskRoute?: TaskRoute }) {
  const out: ReactNode[] = [];
  let quiet = 0;
  const flush = (key: string) => {
    if (quiet > 0) out.push(<QuietLine key={`quiet-${key}`} count={quiet} />);
    quiet = 0;
  };
  for (const record of records) {
    if (isQuiet(record)) {
      quiet += 1;
      continue;
    }
    flush(record.id);
    out.push(<WakeNotice key={record.id} record={record} taskRoute={taskRoute} />);
  }
  flush("end");
  return <>{out}</>;
}
