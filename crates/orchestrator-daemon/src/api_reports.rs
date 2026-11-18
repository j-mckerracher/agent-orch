//! Scout reports (DPES-1105 phase 4a): `reports.list`, `reports.get` and `reports.relay`.
//!
//! A scout's report is untrusted agent text, stored redacted by the executor. It reaches a
//! prompt through the user's `reports.relay` or the report-ready wake: one Restricted turn in
//! the global conversation whose opening text the daemon writes and whose context carries the
//! report between fixed delimiters, labelled as data. The turn can only propose, and every
//! proposal still needs `actions.resolve`. No method here changes a task or a project. All
//! three sit under the `coordinator` capability; the desktop is the user's own app, and the
//! text it reads here was redacted before it was stored.

use orchestrator_store::{
    ConversationTarget, ReportScope, SqliteStore, TaskReport, TaskReportSummary, TurnTrigger,
};

use super::conversation_agent::sanitize;
use super::conversations::map_conversation_error;
use super::report::{relay_block, utc_text};
use super::*;

/// Newest summaries `reports.list` answers with.
const REPORT_LIST_LIMIT: usize = 50;
/// Bytes of a task title the wire carries.
const REPORT_TITLE_BYTES: usize = 80;
/// Characters of the task id the relay's opening text names.
const SHORT_ID_CHARS: usize = 12;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportGetParams {
    task_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReportRelayParams {
    task_id: String,
    idempotency_key: String,
}

/// The title of a task on the wire and in the snapshot: its objective on one line, cut to
/// 80 bytes on a character boundary.
pub(super) fn report_title(objective: &str) -> String {
    let flat = sanitize(&objective.replace('\r', " "))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut end = flat.len().min(REPORT_TITLE_BYTES);
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    flat[..end].to_owned()
}

/// The newest reports of every project, newest first, without their text. At most `limit`.
pub(super) fn newest_reports(
    store: &SqliteStore,
    limit: usize,
) -> Result<Vec<TaskReportSummary>, StoreError> {
    let mut all = Vec::new();
    for project in store.list_projects()? {
        all.extend(store.list_task_reports(ReportScope::Project(&project.id), limit)?);
    }
    all.sort_by(|a, b| {
        b.captured_at
            .cmp(&a.captured_at)
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    all.truncate(limit);
    Ok(all)
}

impl DaemonApi {
    /// `reports.list`: the newest 50 reports with no text: the task, its project code and
    /// title, whether the text was cut, when it was captured and when it was relayed.
    pub(super) fn reports_list(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let summaries = newest_reports(&self.store, REPORT_LIST_LIMIT).map_err(map_store_error)?;
        let mut reports = Vec::new();
        for summary in &summaries {
            let task = match self.store.get(&summary.task_id) {
                Ok(task) => task,
                // A report whose task is gone is not listed; nothing can act on it.
                Err(StoreError::NotFound(_)) => continue,
                Err(error) => return Err(map_store_error(error)),
            };
            reports.push(json!({
                "task_id": summary.task_id,
                "project": super::actions::project_code(&task.contract.project_id),
                "title": report_title(&task.contract.objective),
                "truncated": summary.truncated,
                "captured_at": summary.captured_at,
                "relayed_at": summary.relayed_at,
            }));
        }
        Ok(json!({"reports": reports}))
    }

    /// `reports.get`: one report with its text, as stored (redacted and capped).
    pub(super) fn reports_get(&self, params: Value) -> Result<Value, RpcError> {
        let request: ReportGetParams = parse_params(params)?;
        let report = self.report_of(&request.task_id)?;
        let task = self.store.get(&report.task_id).map_err(map_store_error)?;
        Ok(json!({"report": {
            "task_id": report.task_id,
            "title": report_title(&task.contract.objective),
            "body": report.body,
            "truncated": report.truncated,
            "captured_at": report.captured_at,
            "relayed_at": report.relayed_at,
        }}))
    }

    /// `reports.relay`: starts a chat turn that reads one report. The turn is Restricted
    /// with the daemon's default model whatever the chat was last set to. It is a normal
    /// turn of the global conversation: it fails with `conflict` while another turn runs.
    /// The report goes into this turn's context only, never into the stored history, and
    /// `relayed_at` is set as the turn starts. A second relay of the same report is
    /// a new turn and leaves `relayed_at` as it was. `actions` is the session's negotiated
    /// `orchestrator_actions`, as for `conversations.send`.
    pub(super) fn reports_relay(&self, params: Value, actions: bool) -> Result<Value, RpcError> {
        let request: ReportRelayParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        // A relay runs the daemon's default agent, which a message that named its own agent
        // never settled. Settled before the request gate, like `conversations.send`.
        self.conversations.prepare(None)?;
        let reference = idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "reports.relay",
            &key,
            &request,
            || {
                let report = self.report_of(&request.task_id)?;
                let target = ConversationTarget::Global;
                self.conversations.require_target(&target)?;
                let turn_id = idempotency::stable_id("report-relay-turn", &request.idempotency_key);
                let short: String = request.task_id.chars().take(SHORT_ID_CHARS).collect();
                let text =
                    format!("Relay the report of task {short} and list the decisions I must make.");
                let block = relay_block(&report.task_id, &report.body, report.truncated);
                // A running user turn refuses the relay (`start_relay`), so that is known
                // before anything is marked: a refused relay leaves the report unrelayed, and
                // a promotion stays locked. A running wake turn is cancelled by the relay.
                if self
                    .store
                    .running_conversation_turn(&target)
                    .map_err(map_conversation_error)?
                    .is_some_and(|running| running.trigger != TurnTrigger::Wake)
                {
                    return Err(RpcError::Conflict);
                }
                // Marked before the turn starts, so a proposal the turn makes is checked
                // against a report that counts as relayed. Marking is safe to repeat, which
                // also finishes a relay a stop interrupted, and it never changes the time.
                self.store
                    .mark_task_report_relayed(&report.task_id, &utc_text(self.reports_now_ms()))
                    .map_err(map_store_error)?;
                let begun = self
                    .conversations
                    .start_relay(&target, &turn_id, &text, &block, actions)
                    .map_err(map_conversation_error)?;
                Ok(json!({"turn_id": begun.turn.id}))
            },
        )?;
        let turn_id = super::conversations::reference_str(&reference, "turn_id")?;
        let turn = self
            .store
            .conversation_turn(turn_id)
            .map_err(map_conversation_error)?
            .ok_or(RpcError::NotFound)?;
        Ok(json!({"authority": "none", "turn": self.turn_wire(&turn)}))
    }

    /// The report of a task that still exists.
    fn report_of(&self, task_id: &str) -> Result<TaskReport, RpcError> {
        self.store
            .task_report(task_id)
            .map_err(map_store_error)?
            .ok_or(RpcError::NotFound)
    }

    /// The daemon clock in Unix milliseconds: the wake scheduler's, so a test moves it by hand.
    fn reports_now_ms(&self) -> u64 {
        self.wake
            .as_ref()
            .map_or_else(crate::executor::unix_now_ms, |wake| wake.now_ms())
    }
}
