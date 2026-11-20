/**
 * Questions for the user (DPES-1108): `decisions.list`, `decisions.answer` and `decisions.defer`.
 *
 * Rules:
 * - The live source never imports fixtures. A failed read is `unavailable`, and `unsupported` when the daemon has
 *   no `holds` capability; it is never answered with fixture data.
 * - Answer and defer are user-only. Each call carries an idempotency key that is reused only while the previous
 *   outcome is unknown. The answer is sent exactly as given.
 * - The fixture source never calls a daemon. It keeps its questions in memory for the session.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { QuestionWire } from "../../daemon/types";
import type { SurfaceFixtureScenario } from "./fixtures";
import { liveRead } from "./liveSupport";
import { notOk, observation, okResult } from "./result";
import type { Provenance, QuestionItem, QuestionReceipt, QuestionsSource } from "./types";

export const QUESTIONS_METHODS = ["decisions.list", "decisions.answer", "decisions.defer"] as const;

export function mapQuestion(wire: QuestionWire): QuestionItem {
  return {
    id: wire.id,
    taskId: wire.task_id,
    taskTitle: wire.task_title,
    question: wire.question,
    options: wire.options,
    deferredUntil: wire.deferred_until,
    state: wire.state,
    askedAt: wire.created_at,
  };
}

export function createLiveQuestions(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): QuestionsSource {
  /** One user-only write: the key survives only an outcome the app cannot know. */
  async function write(scope: string, fingerprint: string, send: (key: string) => Promise<{ question: QuestionWire }>): Promise<QuestionReceipt> {
    const key = keys.keyFor(scope, fingerprint);
    try {
      const result = await send(key);
      keys.settle(scope, null);
      return { simulated: false, provenance, question: mapQuestion(result.question) };
    } catch (error) {
      keys.settle(scope, asDaemonError(error));
      throw error;
    }
  }
  return {
    provenance,
    list: () => liveRead(provenance, now, async () => (await connection.api().decisionsList()).questions.map(mapQuestion)),
    answer: (questionId, answer) =>
      write(`decisions:answer:${questionId}`, answer, (key) => connection.api().decisionsAnswer(questionId, answer, key)),
    defer: (questionId, until) =>
      write(`decisions:defer:${questionId}`, until, (key) => connection.api().decisionsDefer(questionId, until, key)),
  };
}

const FIXTURE_QUESTION: QuestionItem = {
  id: "fixture:question-1",
  taskId: "fixture:task-auth-1",
  taskTitle: "Fix the login bug",
  question: "Should existing sessions be migrated to the new format, or expire at their next refresh?",
  options: ["Migrate them now", "Let them expire"],
  deferredUntil: null,
  state: "open",
  askedAt: "2026-11-18T09:00:00Z",
};

export function createFixtureQuestions(scenario: SurfaceFixtureScenario, now: () => number): QuestionsSource {
  const provenance: Provenance = { kind: "fixture", id: "D16", fixture: "questions" };
  let open: QuestionItem[] = [FIXTURE_QUESTION];
  const find = (id: string) => {
    const question = open.find((item) => item.id === id);
    if (question === undefined) throw new DaemonError("not-found", "Fixture data has no such question.");
    return question;
  };
  return {
    provenance,
    list() {
      if (scenario === "loading") return new Promise(() => undefined);
      if (scenario === "unavailable" || scenario === "unsupported" || scenario === "unknown") {
        return Promise.resolve(notOk(provenance, scenario, `Fixture scenario: questions are ${scenario}.`, observation(now())));
      }
      return Promise.resolve(okResult(provenance, scenario === "empty" ? [] : open.map((item) => ({ ...item })), observation(now())));
    },
    async answer(questionId) {
      const question = { ...find(questionId), state: "answered" };
      open = open.filter((item) => item.id !== questionId);
      return { simulated: true, provenance, question };
    },
    async defer(questionId, until) {
      const question = { ...find(questionId), deferredUntil: until };
      open = open.map((item) => (item.id === questionId ? question : item));
      return { simulated: true, provenance, question };
    },
  };
}
