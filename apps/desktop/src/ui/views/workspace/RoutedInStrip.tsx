import { useEffect, useRef, useState } from "react";
import type { RoutedIn } from "../../../sources/surfaces/types";
import { ROUTED_IN_HEADING, ROUTED_IN_HELP, announcement, labelFor, rowsFrom } from "./routedInModel";

/** Tasks the global conversation routed to this workstream, above the composer. Nothing for the global conversation. */
export function RoutedInStrip({ snapshot, workstream }: { snapshot: { routedIn?: RoutedIn[] } | null; workstream: boolean }) {
  const rows = rowsFrom(snapshot);
  const previous = useRef<RoutedIn[] | null>(null);
  const [said, setSaid] = useState("");

  // The live text is always mounted, so a row that arrives later is spoken.
  useEffect(() => {
    const next = rowsFrom(snapshot);
    const text = announcement(previous.current, next);
    previous.current = next;
    if (text) setSaid(text);
  }, [snapshot]);

  if (!workstream) return null;
  return (
    <>
      <p aria-live="polite" className="ws-small">
        {said}
      </p>
      {rows.length > 0 && (
        <section aria-label={ROUTED_IN_HEADING} className="ws-routed-in">
          <h3>{ROUTED_IN_HEADING}</h3>
          <ul>
            {rows.map((row) => (
              <li key={`${row.taskId}:${row.routedAt}`}>{labelFor(row)}</li>
            ))}
          </ul>
          <p className="ws-small">{ROUTED_IN_HELP}</p>
        </section>
      )}
    </>
  );
}
