/** Reply box on the task window (DPES-1106): answers the agent's open question, or sends it a message while it waits. */
import { useState } from "react";
import { APPROVER_REQUIRED_NOTICE, type DaemonError } from "../daemon/errors";
import type { TaskState } from "../daemon/types";
import type { QuestionItem } from "../sources/surfaces/types";
import { ChatComposer } from "./chat/ChatComposer";
import { TERMINAL_STATES } from "./format";
import { type Gate, useGate, useServices } from "./services";
import { Button } from "./theme";
import { ErrorNotice } from "./widgets";
import { type QuestionsModel, useQuestions } from "./views/review/useQuestions";

/** One thing the person said (or the question they answered) in this window. The daemon keeps no message history to read back. */
export interface Turn {
  key: string;
  from: "agent" | "you";
  text: string;
}

/** What the reply box can do right now: why it is off, the open question it answers, and the question list behind both. */
export interface TaskReplyModel {
  reason: string | null;
  question: QuestionItem | undefined;
  questions: QuestionsModel;
}

/** The first reason the reply box is off, or null. Runs before the question list is read, so a non-approver never reaches `decisions.list`. */
function blockedReason(gate: Gate, state: TaskState): string | null {
  if (!gate.connected) return "Reconnect to the daemon to reply to the agent.";
  if (!gate.connection.approver) return APPROVER_REQUIRED_NOTICE;
  if (!gate.can("tasks")) return "This daemon does not support replies to tasks.";
  if (state === "Waiting") return null;
  if (state === "Running") return "The agent is working. You can reply when it waits for you. Live steering is not available yet.";
  if (TERMINAL_STATES.has(state)) return "This task is finished. The agent cannot be messaged.";
  return "The agent is not waiting for a reply, so it cannot be messaged yet.";
}

export function useTaskReply(taskId: string, state: TaskState): TaskReplyModel {
  const base = blockedReason(useGate(), state);
  const questions = useQuestions(base === null);
  if (base !== null) return { reason: base, question: undefined, questions };
  // Without the list the app cannot tell whether a question is open, so a message could queue behind one.
  const unknown = !questions.supported
    ? "This daemon does not report open questions, so the app cannot tell whether one is open."
    : (questions.unavailable ?? (questions.loading ? "Checking whether a question is open." : null));
  const question = unknown === null ? questions.items.find((item) => item.taskId === taskId && item.state === "open") : undefined;
  return { reason: unknown, question, questions };
}

let nextMessage = 0;

/**
 * The chat composer for one task. With an open question the text is the answer and the options only fill the box;
 * otherwise it is a free-form message. Either way the words go exactly as typed.
 */
export function TaskReply({ taskId, reply, onTurns }: { taskId: string; reply: TaskReplyModel; onTurns: (turns: Turn[]) => void }) {
  const { data } = useServices();
  const { reason, question, questions } = reply;
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<DaemonError | null>(null);
  const busy = question ? questions.busy !== null : sending;
  // A different text would mint a new key while the old send may still have queued, so the text stays put until it settles.
  const locked = sending || error?.indeterminate === true;
  const canSend = reason === null && !busy && text.trim() !== "";

  const send = async () => {
    if (question) {
      if (await questions.answer(question, text)) {
        onTurns([
          { key: question.id, from: "agent", text: question.question },
          { key: `${question.id}:answer`, from: "you", text },
        ]);
        setText("");
      }
      return;
    }
    setSending(true);
    setSent(false);
    setError(null);
    const result = await data.messageTask(taskId, text);
    setSending(false);
    if (result.ok) {
      onTurns([{ key: `message-${nextMessage++}`, from: "you", text }]);
      setText("");
      setSent(true);
    } else {
      setError(result.error);
    }
  };

  return (
    <>
      {question && question.options.length > 0 && (
        <span role="group" aria-label="Options" className="task-chat-options">
          {question.options.map((option) => (
            <Button key={option} variant="quiet" onClick={() => setText(option)}>
              {option}
            </Button>
          ))}
        </span>
      )}
      <ChatComposer
        label={question ? "Your answer" : "Message to the agent"}
        sendLabel={question ? "Send answer" : "Send message"}
        placeholder={question ? "Answer the agent's question…" : "Reply to the agent…"}
        value={text}
        onChange={setText}
        onSend={() => void send()}
        canSend={canSend}
        busy={busy}
        disabled={reason !== null || locked}
        blocked={reason}
      />
      <div aria-live="polite" className="ws-feedback">
        {sent && <p role="status">Sent. The agent resumes shortly.</p>}
        {error && <SendError error={error} />}
      </div>
    </>
  );
}

/** Plain words for a refused message; each says nothing was changed. Other failures use the shared notice. */
function SendError({ error }: { error: DaemonError }) {
  switch (error.kind) {
    case "invalid-params":
      return (
        <p role="alert">
          The message must not be blank, must be at most 4096 bytes, and must use allowed characters. Nothing was changed.
        </p>
      );
    case "not-found":
      return <p role="alert">That task is no longer known. Nothing was changed.</p>;
    case "conflict":
      return (
        <p role="alert">
          The task is no longer waiting, is waiting on a question, or already has the most messages it can hold. Nothing was
          changed.
        </p>
      );
    default:
      return (
        <>
          <ErrorNotice error={error} action="Sending the message" />
          {error.indeterminate && <p role="status">The outcome is unknown. Sending again reuses the same request key.</p>}
        </>
      );
  }
}
