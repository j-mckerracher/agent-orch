/** Read-only "Thinking" panel for a session: everything the agent's provider exposed, in full. */
import { CopyButton, Disclosure, Markdown } from "../../theme";
import { ErrorNotice } from "../../widgets";
import { useSessionThinking } from "./useSessionThinking";

export function SessionThinking({ sessionId, running }: { sessionId: string; running: boolean }) {
  const thinking = useSessionThinking(sessionId, running);
  return (
    <Disclosure summary="Thinking" className="agents-thinking">
      {thinking.error && <ErrorNotice error={thinking.error} action="Refreshing thinking" />}
      {!thinking.loaded ? (
        <p role="status">Loading thinking…</p>
      ) : thinking.text === "" ? (
        <p role="status">No thinking recorded</p>
      ) : (
        <>
          <Markdown text={thinking.text} />
          <CopyButton text={thinking.text} label="Copy thinking" />
        </>
      )}
      {thinking.truncated && <p role="status">Thinking log hit its size limit.</p>}
    </Disclosure>
  );
}
