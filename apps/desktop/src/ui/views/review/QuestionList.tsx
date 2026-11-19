// "Needs your decision" (DPES-1108): open questions read from `decisions.list` only, never from chat text. An answer
// is sent exactly as typed; an option only fills the text box. A deferred question stays open and still blocks its task.
import { useId, useMemo, useState } from "react";
import type { QuestionItem } from "../../../sources/surfaces/types";
import { Badge, Button, EmptyState, Pill } from "../../theme";
import { MAX_HOLD_DAYS, formatHoldDay, holdRange } from "./holdDates";
import { MAX_ANSWER_CHARS, useQuestions, type QuestionsModel } from "./useQuestions";

const REASON_ID = "rd-question-reason";

function QuestionRow({ question, model }: { question: QuestionItem; model: QuestionsModel }) {
  const [text, setText] = useState("");
  const [day, setDay] = useState("");
  const answerId = useId();
  const deferId = useId();
  const range = useMemo(() => holdRange(new Date()), []);
  const off = model.blocked !== null || model.busy !== null;
  const reason = model.blocked !== null ? REASON_ID : undefined;
  const label = question.taskTitle || question.taskId;

  return (
    <li className="rd-row-item rd-question">
      <span className="rd-row-title">
        {question.question}
        {question.deferredUntil !== null && <Pill tone="warn">Deferred</Pill>}
      </span>
      <span className="rd-row-sub">
        Task: {label}
        {question.deferredUntil !== null && ` · Deferred until ${formatHoldDay(question.deferredUntil, false)}`}
      </span>
      {question.options.length > 0 && (
        <span className="rd-held-actions" role="group" aria-label={`Options for ${label}`}>
          {question.options.map((option) => (
            <Button key={option} variant="quiet" onClick={() => setText(option)}>
              {option}
            </Button>
          ))}
        </span>
      )}
      <form
        className="rd-hold-form"
        onSubmit={(event) => {
          event.preventDefault();
          void model.answer(question, text).then((sent) => {
            if (sent) setText("");
          });
        }}
      >
        <label htmlFor={answerId}>Your answer</label>
        <textarea id={answerId} rows={3} maxLength={MAX_ANSWER_CHARS} value={text} onChange={(event) => setText(event.target.value)} />
        <Button type="submit" variant="primary" disabled={off || text.trim() === ""} aria-describedby={reason}>
          Send answer
        </Button>
      </form>
      <form
        className="rd-hold-form"
        onSubmit={(event) => {
          event.preventDefault();
          void model.defer(question, day);
        }}
      >
        <label htmlFor={deferId}>Defer until</label>
        <input id={deferId} type="date" required min={range.min} max={range.max} value={day} onChange={(event) => setDay(event.target.value)} />
        <Button type="submit" disabled={off || day === ""} aria-describedby={reason}>
          Defer
        </Button>
        <span className="rd-reason">
          Pick a day from today to {MAX_HOLD_DAYS} days from today. The task stays blocked until you answer.
        </span>
      </form>
    </li>
  );
}

export function QuestionList() {
  const model = useQuestions();
  const headingId = useId();
  if (!model.supported) return null;
  return (
    <section aria-labelledby={headingId} className="rd-card" data-testid="questions">
      <header className="rd-card-head">
        <h2 id={headingId}>Needs your decision</h2>
        {model.fixture && <Badge tone="placeholder">Fixture data</Badge>}
      </header>
      <p className="rd-note" role="status" aria-live="polite" data-testid="questions-status">
        {model.announcement}
      </p>
      {model.blocked !== null && (
        <p id={REASON_ID} className="rd-note">
          {model.blocked}
        </p>
      )}
      {model.unavailable !== null ? (
        <EmptyState title="Questions are not available" description={model.unavailable} />
      ) : model.items.length === 0 ? (
        <p className="rd-note">{model.loading ? "Loading questions…" : "No question is waiting for your answer."}</p>
      ) : (
        <ul className="rd-rows" aria-label="Questions waiting for your answer">
          {model.items.map((question) => (
            <QuestionRow key={question.id} question={question} model={model} />
          ))}
        </ul>
      )}
    </section>
  );
}
