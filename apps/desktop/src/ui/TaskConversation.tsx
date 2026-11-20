/**
 * The Chat of one task (DPES-1088, DPES-1044): the workspace chat (thread and composer) bound to one task's session.
 * Order of the thread: the session's activity (thinking and tool calls, in the daemon's order), then its retained output,
 * then what was said in this window. The daemon gives activity and output separate sequences and no times, so they cannot
 * be interleaved with each other or with messages; the older entries sit first and the live status line sits last.
 * Sending goes to the task (`tasks.message`, or `decisions.answer` for an open question): the daemon reads either only
 * from a Waiting task and cannot address one session of several.
 */
import { useState, type ReactNode } from "react";
import type { OutputLine, TaskState } from "../daemon/types";
import { ChatLog } from "./chat/ChatLog";
import { Timestamp } from "./chat/Timestamp";
import { useGate } from "./services";
import { Bubble, CopyButton, EmptyState, Markdown } from "./theme";
import { type Turn, TaskReply, useTaskReply } from "./TaskReply";
import { ActivityLog } from "./activity/ActivityLog";
import { ActivityStatus } from "./activity/ActivityStatus";
import { type OutputTailState, useOutputTail } from "./views/agents/useOutputTail";
import { useSessionActivity } from "./views/agents/useSessionActivity";
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
        <Bubble from="coordinator" author={agent} time={<Timestamp at={open.askedAt} />} actions={<CopyButton text={open.question} />}>
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
    <Bubble
      from={turn.from === "you" ? "human" : "coordinator"}
      author={turn.from === "you" ? "You" : agent}
      time={<Timestamp at={turn.at} />}
      actions={<CopyButton text={turn.text} />}
    >
      <Markdown text={turn.text} />
    </Bubble>
  );
}

/** The session's activity (thinking and tool calls) and retained output as thread entries, then `children` (what was said in this window). */
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
  const activity = useSessionActivity(sessionId, running);
  const last = activity.items[activity.items.length - 1];
  // Grows with every line, with every chunk of the newest thinking, and when the newest tool call ends.
  const grown = `${activity.items.length}:${last?.kind === "thinking" ? last.text.length : last?.kind === "tool" ? last.state : ""}`;
  return (
    <>
      <ChatLog label={label} scrollKey={`${tail.lines.length}:${grown}:${scrollKey}`}>
        {activity.error && <ErrorNotice error={activity.error} action="Refreshing activity" />}
        {activity.items.length > 0 && <ActivityLog items={activity.items} streaming={running} active={running} />}
        {activity.truncated && <p className="ws-small">Thinking log hit its size limit.</p>}
        <Output tail={tail} running={running} agent={agent} />
        {children}
      </ChatLog>
      {running && <ActivityStatus live={activity.live} />}
    </>
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
        <Bubble
          from="coordinator"
          author={agent}
          // Output lines carry no times, so the bubble says when the app last read them.
          time={tail.readAt && <>as of <Timestamp at={tail.readAt} /></>}
          actions={<CopyButton text={tail.lines.map((line) => line.text).join("\n")} label="Copy output" />}
        >
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
