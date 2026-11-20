/** The task window's Conversation section: the workspace chat (thread and composer) bound to one task's session (DPES-1088). */
import { useState, type ReactNode } from "react";
import type { OutputLine, TaskState } from "../daemon/types";
import { ChatLog } from "./chat/ChatLog";
import { useGate } from "./services";
import { Bubble, CopyButton, EmptyState, Markdown } from "./theme";
import { type Turn, TaskReply, useTaskReply } from "./TaskReply";
import { ThinkingBlock } from "./views/workspace/ThinkingBlock";
import { type OutputTailState, useOutputTail } from "./views/agents/useOutputTail";
import { useSessionThinking } from "./views/agents/useSessionThinking";
import { ErrorNotice } from "./widgets";

interface Props {
  taskId: string;
  state: TaskState;
  /** The session shown; null when the task has none yet. */
  sessionId: string | null;
  running: boolean;
  /** The agent's name, shown on its bubbles. */
  agent: string;
}

export function TaskConversation({ taskId, state, sessionId, running, agent }: Props) {
  const reply = useTaskReply(taskId, state);
  const [turns, setTurns] = useState<Turn[]>([]);
  // An answered question stays in the thread as a turn, so the open copy of it is hidden once it is one.
  const open = reply.question && !turns.some((turn) => turn.key === reply.question?.id) ? reply.question : undefined;
  const announcement = reply.questions.announcement;
  // The session's output and thinking carry no times, so they come first and what was said in this window follows.
  const said = (
    <>
      {turns.map((turn) => (
        <TurnBubble key={turn.key} turn={turn} agent={agent} />
      ))}
      {open && (
        <Bubble from="coordinator" author={agent} actions={<CopyButton text={open.question} />}>
          <Markdown text={open.question} />
        </Bubble>
      )}
      {announcement !== "" && (
        <div className="ws-state">
          <p role="status">{announcement}</p>
        </div>
      )}
    </>
  );
  const scrollKey = `${turns.length}:${open?.id ?? ""}:${announcement}`;
  const label = `Conversation with ${agent}`;
  return (
    <section aria-label="Agent conversation" className="ps-section task-chat">
      <h2>Conversation</h2>
      {sessionId === null ? (
        <ChatLog label={label} scrollKey={scrollKey}>
          <EmptyState title="No messages yet" description="No session is recorded for this task, so the agent has not said anything." />
          {said}
        </ChatLog>
      ) : (
        <SessionThread key={sessionId} taskId={taskId} sessionId={sessionId} running={running} agent={agent} label={label} scrollKey={scrollKey}>
          {said}
        </SessionThread>
      )}
      <TaskReply key={reply.question?.id ?? "message"} taskId={taskId} reply={reply} onTurns={(added) => setTurns((current) => [...current, ...added])} />
    </section>
  );
}

function TurnBubble({ turn, agent }: { turn: Turn; agent: string }) {
  return (
    <Bubble from={turn.from === "you" ? "human" : "coordinator"} author={turn.from === "you" ? "You" : agent} actions={<CopyButton text={turn.text} />}>
      <Markdown text={turn.text} />
    </Bubble>
  );
}

/** The session's thinking and retained output as thread entries, then `children` (what was said in this window). */
function SessionThread({
  taskId,
  sessionId,
  running,
  agent,
  label,
  scrollKey,
  children,
}: Omit<Props, "state"> & { sessionId: string; label: string; scrollKey: string; children: ReactNode }) {
  const tail = useOutputTail(taskId, sessionId, running);
  const thinking = useSessionThinking(sessionId, running);
  return (
    <ChatLog label={label} scrollKey={`${tail.lines.length}:${thinking.text.length}:${scrollKey}`}>
      {thinking.error && <ErrorNotice error={thinking.error} action="Refreshing thinking" />}
      {thinking.text !== "" && <ThinkingBlock text={thinking.text} streaming={running} />}
      {thinking.truncated && <p className="ws-small">Thinking log hit its size limit.</p>}
      <Output tail={tail} running={running} agent={agent} />
      {children}
    </ChatLog>
  );
}

function Output({ tail, running, agent }: { tail: OutputTailState; running: boolean; agent: string }) {
  const { connected } = useGate();
  return (
    <>
      {!connected && (
        <p role="status" className="ws-small">
          Not connected to the daemon, so output is not refreshing.
        </p>
      )}
      {tail.error && <ErrorNotice error={tail.error} action="Refreshing recent output" />}
      {!tail.loaded ? (
        <p role="status" className="ws-small">
          Loading the conversation…
        </p>
      ) : !tail.available ? (
        <EmptyState
          title="No messages yet"
          description={
            tail.ended
              ? "The session ended and no output is retained."
              : "No output is available for this session: it has not started, or the daemon restarted."
          }
        />
      ) : tail.lines.length === 0 ? (
        <EmptyState title="No messages yet" description={tail.ended ? "The session ended without producing output." : "The agent has not produced output yet."} />
      ) : (
        <Bubble from="coordinator" author={agent} actions={<CopyButton text={tail.lines.map((line) => line.text).join("\n")} label="Copy output" />}>
          {tail.olderDropped && <p className="ws-small">Older lines were dropped and are not shown.</p>}
          <div className="ws-output">
            {tail.lines.map((line) => (
              <OutputRow key={line.sequence} line={line} />
            ))}
          </div>
          <p className="ws-small">
            Output is shown as plain text and is not redacted. It may contain secrets.
            {running ? " It refreshes about every 2 seconds." : ""}
            {tail.ended ? " The session ended; this is its final retained output." : ""}
          </p>
        </Bubble>
      )}
    </>
  );
}

function OutputRow({ line }: { line: OutputLine }) {
  return (
    <div data-stream={line.stream}>
      {line.stream === "stderr" && <span className="ws-output-stream">stderr </span>}
      {line.text}
      {line.line_truncated && <span className="ws-output-stream"> [line truncated]</span>}
    </div>
  );
}
