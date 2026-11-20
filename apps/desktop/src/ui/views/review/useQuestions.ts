/** Open questions for the user in the Review view, and the two user-only changes: answer and defer (DPES-1108). */
import { useCallback, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError, type DaemonError } from "../../../daemon/errors";
import { FIXTURE_HOLD_NOTICE } from "../../../sources/surfaces/holds";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { QuestionItem, QuestionReceipt } from "../../../sources/surfaces/types";
import { type Action, type Outcome, useAction } from "../../action";
import { useGate, useServices, useSurface } from "../../services";
import { useRetained } from "../workspace/useRetained";
import { formatHoldDay, holdErrorText } from "./holdDates";
import { HOLDS_POLL_MS } from "./useHolds";

/** The longest answer the daemon accepts. */
export const MAX_ANSWER_CHARS = 4096;

const NO_QUESTIONS: QuestionItem[] = [];

export interface QuestionsModel {
  /** False for a live daemon without the `holds` capability: no list, no controls, no error. */
  supported: boolean;
  loading: boolean;
  /** Why the list could not be read; null while it can. */
  unavailable: string | null;
  items: QuestionItem[];
  fixture: boolean;
  /** The visible reason the controls are off; null when they work. */
  blocked: string | null;
  /** The question being changed now; null when idle. */
  busy: string | null;
  /** The last answer or deferral: pending, then what happened. Render it with ActionStatus beside the controls. */
  action: Action;
  /** The last result's words, or "" before the first change. Read by TaskConversation (no failure role there yet). */
  announcement: string;
  /** Sends `text` exactly as given. Resolves true when the daemon confirmed it. */
  answer: (question: QuestionItem, text: string) => Promise<boolean>;
  defer: (question: QuestionItem, until: string) => Promise<boolean>;
}

/** Plain words for a refused answer or deferral. Each says nothing was changed, except when the outcome is unknown. */
function questionErrorText(error: DaemonError, operation: "answer" | "defer"): string {
  switch (error.kind) {
    case "invalid-params":
      return operation === "answer"
        ? `The daemon did not accept that answer. It must have text and at most ${MAX_ANSWER_CHARS} characters. Nothing was changed.`
        : holdErrorText(error, "hold");
    case "not-found":
      return "That question is no longer known. Nothing was changed.";
    case "conflict":
      return "That question is already closed. Nothing was changed.";
    default:
      return holdErrorText(error, "hold");
  }
}

export function useQuestions(enabled = true): QuestionsModel {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.questions;
  const live = source.provenance.kind === "daemon";
  const supported = !live || gate.connection.capabilities.includes("holds");
  // A caller that cannot yet use the list (the task window, before its reply box is on) reads nothing.
  const active = enabled && supported;
  const [nonce, setNonce] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const action = useAction();

  const loaded = useSurface(
    () =>
      active
        ? source.list()
        : Promise.resolve(notOk<QuestionItem[]>(source.provenance, "unsupported", "This daemon has no questions.", observation(Date.now()))),
    `questions${active ? "" : "-off"}#${nonce}`,
    source.provenance,
    active ? { pollMs: HOLDS_POLL_MS, onFocus: true } : undefined,
  );
  const result = useRetained(loaded, "questions");

  const blocked = !live
    ? null
    : !gate.connected
      ? "Reconnect to the daemon to answer or defer a question."
      : !gate.connection.approver
        ? APPROVER_REQUIRED_NOTICE
        : null;

  const change = useCallback(
    async (id: string, operation: "answer" | "defer", send: () => Promise<QuestionReceipt>, done: string): Promise<boolean> => {
      if (busy !== null) return false;
      let confirmed = false;
      await action.run(operation === "answer" ? "Sending answer…" : "Deferring…", async (): Promise<Outcome> => {
        setBusy(id);
        try {
          const receipt = await send();
          setNonce((value) => value + 1);
          confirmed = true;
          return receipt.simulated ? { kind: "fixture", text: `${done} ${FIXTURE_HOLD_NOTICE}` } : { kind: "saved", text: done };
        } catch (error) {
          throw new Error(questionErrorText(asDaemonError(error), operation));
        } finally {
          setBusy(null);
        }
      });
      return confirmed;
    },
    [action.run, busy],
  );

  const answer = useCallback(
    (question: QuestionItem, text: string) =>
      text.trim() === ""
        ? Promise.resolve(false)
        : change(question.id, "answer", () => source.answer(question.id, text), `Answered: ${question.taskTitle || question.taskId}.`),
    [change, source],
  );

  const defer = useCallback(
    (question: QuestionItem, until: string) =>
      change(
        question.id,
        "defer",
        () => source.defer(question.id, until),
        `Deferred: ${question.taskTitle || question.taskId} until ${formatHoldDay(until, true)}.`,
      ),
    [change, source],
  );

  return {
    supported,
    loading: active && result.status === "loading",
    unavailable: active && (result.status === "unavailable" || result.status === "unknown") ? result.reason : null,
    items: active && result.status === "ok" ? result.data : NO_QUESTIONS,
    fixture: !live,
    blocked,
    busy,
    action,
    announcement: action.state.phase === "done" ? action.state.outcome.text : "",
    answer,
    defer,
  };
}
