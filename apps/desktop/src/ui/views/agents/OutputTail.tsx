/** Read-only "Recent output" panel: plain-text worker output with no input and no controls. */
import { ErrorNotice } from "../../widgets";
import { useGate } from "../../services";
import { useOutputTail } from "./useOutputTail";

export function OutputTail({ taskId, sessionId, running }: { taskId: string; sessionId: string; running: boolean }) {
  const tail = useOutputTail(taskId, sessionId, running);
  const { connected } = useGate();
  return (
    <section aria-label="Recent output" className="agents-controls agents-output">
      <h4>Recent output</h4>
      <p className="agents-sub">
        Read-only, shown as plain text. Output is not redacted and may contain secrets.
        {running ? " Refreshes about every 2 seconds while the session runs." : ""}
      </p>
      {!connected && <p role="status">Not connected to the daemon: recent output is not refreshing.</p>}
      {tail.error && <ErrorNotice error={tail.error} action="Refreshing recent output" />}
      {!tail.loaded ? (
        <p role="status">Loading recent output…</p>
      ) : !tail.available ? (
        <p role="status">
          {tail.ended ? "The session ended and no output is retained. " : ""}
          No output is available for this session: it has not started, its output was evicted after it ended, or the
          daemon restarted.
        </p>
      ) : (
        <>
          {tail.olderDropped && <p role="status">Older lines were dropped and are not shown.</p>}
          {tail.lines.length === 0 ? (
            <p role="status">{tail.ended ? "The session ended without producing output." : "No output yet."}</p>
          ) : (
            <div role="log" aria-label="Recent worker output" tabIndex={0} className="agents-output-log">
              {tail.lines.map((line) => (
                <div key={line.sequence} className="agents-output-line" data-stream={line.stream}>
                  <span className="agents-output-stream">{line.stream}</span> <span className="agents-sub">#{line.sequence}</span>{" "}
                  <code>{line.text}</code>
                  {line.line_truncated && <span className="agents-flag"> [line truncated]</span>}
                </div>
              ))}
            </div>
          )}
          {tail.ended && tail.lines.length > 0 && <p role="status">The session ended; this is its final retained output.</p>}
        </>
      )}
    </section>
  );
}
