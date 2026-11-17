import type { ReactNode } from "react";
import type { WakeRecord } from "../../../sources/surfaces/types";
import { WAKE_LABEL, isQuiet, quietLine, skipText } from "./monitoringModel";

/** The visible words that say an update is the orchestrator's own doing, so it is never read as an answer. */
export function WakeLabel() {
  return <span className="ws-wake-label">{WAKE_LABEL}</span>;
}

/** The opening entry of an automatic turn: the event lines, inside the update, never a "You" bubble. */
export function WakeLines({ text }: { text: string }) {
  return (
    <div className="ws-wake">
      <WakeLabel />
      <p className="ws-wake-lines">{text}</p>
    </div>
  );
}

/** What the record's state adds to its notice, in words. A deferred or waiting record adds nothing. */
function stateNote(record: WakeRecord): string | null {
  if (record.state === "skipped") return skipText(record.skipReason);
  if (record.state === "failed") return "The automatic reply failed.";
  return null;
}

/** One deterministic notice. It is written before any model turn starts, so a failed or skipped turn never hides it. */
export function WakeNotice({ record }: { record: WakeRecord }) {
  const note = stateNote(record);
  return (
    <div role="status" className="ws-wake" data-state={record.state}>
      <WakeLabel />
      <p className="ws-wake-lines">{record.notice}</p>
      {note && <p className="ws-wake-note">{note}</p>}
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
export function WakeRecords({ records }: { records: readonly WakeRecord[] }) {
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
    out.push(<WakeNotice key={record.id} record={record} />);
  }
  flush("end");
  return <>{out}</>;
}
