//! User decisions (DPES-1108 phase 3): `decisions.list`, `decisions.answer`, `decisions.defer`
//! and `decisions.reconcile`, and the write an approved `decision.ask` runs.
//!
//! A question is held for the user and bound to a task; while it is open the task does not
//! start. All four methods are user-only (routed in the ipc crate), so no agent or orchestrator
//! turn can read or close a question through the socket. An answer is the user's exact words.
//! A deferral names a day (today to today plus 60 days, the hold rule) and only hides the
//! question until that day ends; it stays open and keeps its task held. A reconcile closes it
//! with evidence that it no longer matters. The store refuses every other close.
//!
//! A worker's own question (a later phase) is raised with `SqliteStore::raise_question` and
//! `QuestionRaiser::Worker`; it needs nothing from this module.

use orchestrator_store::{ClosingKind, DecisionQuestion, QuestionError, QuestionRaiser};

use super::holds::hold_end_ms;
use super::reports::report_title;
use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AnswerParams {
    question_id: String,
    answer: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeferParams {
    question_id: String,
    until: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReconcileParams {
    question_id: String,
    evidence: String,
    idempotency_key: String,
}

impl DaemonApi {
    /// `decisions.list`: every open question, oldest first, deferred ones included and marked.
    pub(super) fn decisions_list(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let questions = self.store.open_questions().map_err(map_question_error)?;
        let now = self.holds_now_ms();
        let wire: Vec<Value> = questions
            .iter()
            .map(|question| self.question_wire(question, now))
            .collect();
        Ok(json!({"questions": wire}))
    }

    /// `decisions.answer`: closes a question with the user's words, stored as sent.
    pub(super) fn decisions_answer(&self, params: Value) -> Result<Value, RpcError> {
        let request: AnswerParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "decisions.answer",
            &key,
            &request,
            || {
                let question = self
                    .store
                    .answer_question(&request.question_id, &request.answer)
                    .map_err(map_question_error)?;
                Ok(json!({"question": self.question_wire(&question, self.holds_now_ms())}))
            },
        )
    }

    /// `decisions.defer`: hides an open question until the end of a day. It stays open.
    pub(super) fn decisions_defer(&self, params: Value) -> Result<Value, RpcError> {
        let request: DeferParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "decisions.defer",
            &key,
            &request,
            || {
                let now = self.holds_now_ms();
                let until_ms = hold_end_ms(&request.until, now).ok_or(RpcError::InvalidParams)?;
                let question = self
                    .store
                    .defer_question(&request.question_id, &request.until, until_ms, now)
                    .map_err(map_question_error)?;
                Ok(json!({"question": self.question_wire(&question, now)}))
            },
        )
    }

    /// `decisions.reconcile`: closes a question as moot, with the evidence for that.
    pub(super) fn decisions_reconcile(&self, params: Value) -> Result<Value, RpcError> {
        let request: ReconcileParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "decisions.reconcile",
            &key,
            &request,
            || {
                let question = self
                    .store
                    .reconcile_question(&request.question_id, &request.evidence)
                    .map_err(map_question_error)?;
                Ok(json!({"question": self.question_wire(&question, self.holds_now_ms())}))
            },
        )
    }

    /// The question write an approved `decision.ask` runs. The caller holds the idempotency
    /// gate; `question_id` derives from its key, so a rerun finds the question it raised.
    pub(super) fn ask_inner(
        &self,
        question_id: &str,
        task_id: &str,
        question: &str,
        options: &[String],
        key: Option<&str>,
    ) -> Result<DecisionQuestion, RpcError> {
        self.store
            .raise_question(
                question_id,
                task_id,
                question,
                options,
                key,
                QuestionRaiser::Orchestrator,
            )
            .map_err(map_question_error)
    }

    fn question_wire(&self, question: &DecisionQuestion, now_ms: i64) -> Value {
        // A task that cannot be read still lists its question: the user may need to reconcile it.
        let (task_title, project) = self.store.get(&question.task_id).map_or_else(
            |_| (String::new(), String::new()),
            |task| {
                (
                    report_title(&task.contract.objective),
                    super::actions::project_code(&task.contract.project_id),
                )
            },
        );
        let state = match question.closing.as_ref().map(|closing| closing.kind) {
            None => "open",
            Some(ClosingKind::Answer) => "answered",
            Some(ClosingKind::Reconcile) => "reconciled",
        };
        json!({
            "id": question.id,
            "task_id": question.task_id,
            "task_title": task_title,
            "project": project,
            "question": question.question,
            "options": question.options,
            "key": question.key,
            "raised_by": question.raised_by.as_str(),
            "state": state,
            "deferred_until": question
                .deferred_until
                .as_ref()
                .filter(|_| question.is_deferred_at(now_ms))
                .map(|(date, _)| date),
            "created_at": question.created_at,
            "closing": question.closing.as_ref().map(|closing| json!({
                "kind": closing.kind.as_str(),
                "text": closing.text,
                "recorded_at": closing.recorded_at,
            })),
        })
    }
}

/// A question refusal on the wire: no task or no question is `NotFound`, a closed question or
/// a taken id is `Conflict`, and text or a date the rules refuse is `InvalidParams`.
pub(super) fn map_question_error(error: StoreError) -> RpcError {
    match QuestionError::of(&error) {
        Some(QuestionError::NoTask) => RpcError::NotFound,
        Some(
            QuestionError::Closed | QuestionError::IdInUse | QuestionError::WorkerQuestionOpen,
        ) => RpcError::Conflict,
        Some(_) => RpcError::InvalidParams,
        None => map_store_error(error),
    }
}
