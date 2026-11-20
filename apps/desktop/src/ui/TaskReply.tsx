/** Reply box on the task window (DPES-1106): answers the agent's open question, or sends it a message while it waits. */
import { useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, type DaemonError } from "../daemon/errors";
import type { TaskState } from "../daemon/types";
import type { QuestionItem } from "../sources/surfaces/types";
import { TERMINAL_STATES } from "./format";
import { type Gate, useGate, useServices } from "./services";
import { Button } from "./theme";
import { ErrorNotice } from "./widgets";
import { type QuestionsModel, useQuestions } from "./views/review/useQuestions";

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

export function TaskReply({ taskId, state }: { taskId: string; state: TaskState }) {
  const reason = blockedReason(useGate(), state);
  if (reason === null) return <WaitingReply taskId={taskId} />;
  return <MessageForm taskId={taskId} reason={reason} />;
}

/** A Waiting task: the answer box when its question is open, otherwise the message box. Reads the question list only here. */
function WaitingReply({ taskId }: { taskId: string }) {
  const model = useQuestions();
  const question = model.items.find((item) => item.taskId === taskId && item.state === "open");
  // Without the list the app cannot tell whether a question is open, so a message could queue behind one.
  const unknown = !model.supported
    ? "This daemon does not report open questions, so the app cannot tell whether one is open."
    : model.unavailable ?? (model.loading ? "Checking whether a question is open." : null);
  return (
    <div>
      {model.announcement !== "" && <p role="status">{model.announcement}</p>}
      {unknown !== null ? (
        <MessageForm taskId={taskId} reason={unknown} />
      ) : question === undefined ? (
        <MessageForm taskId={taskId} reason={null} />
      ) : (
        <AnswerForm question={question} model={model} />
      )}
    </div>
  );
}

/** The open question: its options only fill the box; "Send answer" sends the words exactly as typed. */
function AnswerForm({ question, model }: { question: QuestionItem; model: QuestionsModel }) {
  const [text, setText] = useState("");
  const inputId = useId();
  return (
    <div>
      <p>{question.question}</p>
      {question.options.length > 0 && (
        <span role="group" aria-label="Options">
          {question.options.map((option) => (
            <Button key={option} variant="quiet" onClick={() => setText(option)}>
              {option}
            </Button>
          ))}
        </span>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void model.answer(question, text).then((sent) => {
            if (sent) setText("");
          });
        }}
      >
        <label htmlFor={inputId}>Your answer</label>
        <textarea id={inputId} rows={3} value={text} onChange={(event) => setText(event.target.value)} />
        <Button type="submit" variant="primary" disabled={model.busy !== null || text.trim() === ""}>
          Send answer
        </Button>
      </form>
    </div>
  );
}

/** Free-form message to the task's agent. A `reason` turns the box off and says why. */
function MessageForm({ taskId, reason }: { taskId: string; reason: string | null }) {
  const { data } = useServices();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<DaemonError | null>(null);
  const inputId = useId();
  const reasonId = useId();
  const off = reason !== null;
  const describedBy = off ? reasonId : undefined;
  // A different text would mint a new key while the old send may still have queued, so the text stays put until it settles.
  const locked = sending || error?.indeterminate === true;

  const send = async () => {
    setSending(true);
    setSent(false);
    setError(null);
    const result = await data.messageTask(taskId, text);
    setSending(false);
    if (result.ok) {
      setText("");
      setSent(true);
    } else {
      setError(result.error);
    }
  };

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <label htmlFor={inputId}>Message to the agent</label>
      <textarea
        id={inputId}
        rows={3}
        value={text}
        disabled={off || locked}
        aria-describedby={describedBy}
        onChange={(event) => setText(event.target.value)}
      />
      <Button type="submit" variant="primary" disabled={off || sending || text.trim() === ""} aria-describedby={describedBy}>
        Send message
      </Button>
      {reason !== null && (
        <p id={reasonId} className="muted">
          {reason}
        </p>
      )}
      {sent && <p role="status">Sent. The agent resumes shortly.</p>}
      {error && <SendError error={error} />}
    </form>
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
