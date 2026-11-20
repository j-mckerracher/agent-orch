//! Wake scheduler tests (DPES-1105 phase 3c). Time is a `ManualClock` that starts at the real
//! time (a session's start comes from the database clock) and moves only when a test says so.
//! The model runtime is a probe that records what it was asked and never starts a process.

use std::sync::atomic::{AtomicBool, Ordering};

use orchestrator_agents::UsageReport;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::permission::PermissionLevel;
use orchestrator_core::records::Project;
use orchestrator_core::supervision::{ManualClock, DEBOUNCE_MS, DEFAULT_WAKE_HOURLY_CAP};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};
use orchestrator_core::workstream::{Workstream, WorkstreamState};
use orchestrator_diagnostics::Diagnostics;
use orchestrator_store::{ConversationUsageReceipt, ReceiptOutcome, TurnTrigger};

use super::super::conversation_agent::build_prompt;
use super::super::conversation_runtime::{
    Choice, ConversationRuntime, ConversationService, ReplySink, RuntimeFailure, RuntimeIdentity,
    TurnControl, TurnRequest, WorkstreamDirectory,
};
use super::super::load::tests::{at, cpu};
use super::super::*;
use super::*;

const MINUTE: u64 = 60_000;
/// Slack past a whole interval: a session's start is the database clock cut to the second.
const SLACK: u64 = 2_000;

#[derive(Clone)]
struct Seen {
    level: PermissionLevel,
    wake: bool,
    actions: bool,
    context: String,
    prompt: String,
}

struct Control(Arc<AtomicBool>);

impl TurnControl for Control {
    fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Answers inside `start_turn` when it has a reply, else keeps the sink for the test.
struct Probe {
    identity: RuntimeIdentity,
    reply: Mutex<Option<String>>,
    tokens: Mutex<Option<u64>>,
    held: Mutex<Vec<ReplySink>>,
    seen: Mutex<Vec<Seen>>,
    cancelled: Arc<AtomicBool>,
    /// `prepare` answers `Conflict` while set: the agent is still being settled.
    unsettled: AtomicBool,
    prepared: Mutex<Vec<Option<Choice>>>,
    /// Each answered turn also records one thinking entry and one tool call.
    activity: AtomicBool,
}

impl Probe {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            identity: RuntimeIdentity {
                name: "probe".into(),
                version: "1.0.0".into(),
            },
            reply: Mutex::new(None),
            tokens: Mutex::new(None),
            held: Mutex::new(Vec::new()),
            seen: Mutex::new(Vec::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
            unsettled: AtomicBool::new(false),
            prepared: Mutex::new(Vec::new()),
            activity: AtomicBool::new(false),
        })
    }

    fn replying(&self, text: Option<&str>) {
        *self.reply.lock().unwrap() = text.map(str::to_owned);
    }

    fn reporting(&self, tokens: Option<u64>) {
        *self.tokens.lock().unwrap() = tokens;
    }

    fn started(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    fn last(&self) -> Seen {
        self.seen.lock().unwrap().last().cloned().expect("a turn")
    }

    /// Ends a held turn with `text`.
    fn finish(&self, index: usize, text: &str) {
        let sink = self.held.lock().unwrap()[index].clone();
        sink.delta(text).unwrap();
        sink.complete();
    }
}

impl ConversationRuntime for Probe {
    fn identity(&self, _choice: Option<&Choice>) -> RuntimeIdentity {
        self.identity.clone()
    }

    fn prepare(&self, choice: Option<&Choice>) -> Result<(), RpcError> {
        if self.unsettled.load(Ordering::SeqCst) {
            return Err(RpcError::Conflict);
        }
        self.prepared.lock().unwrap().push(choice.cloned());
        Ok(())
    }

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure> {
        self.seen.lock().unwrap().push(Seen {
            level: request.level,
            wake: request.wake.is_some(),
            actions: request.actions,
            context: request.context.clone().unwrap_or_default(),
            prompt: build_prompt(&request).joined(),
        });
        let reply = self.reply.lock().unwrap().clone();
        match reply {
            Some(text) => {
                if self.activity.load(Ordering::SeqCst) {
                    sink.thinking("pondering");
                    let start =
                        crate::activity::ToolStart::bounded("tool-1", "shell", "ls").unwrap();
                    sink.tool_started(start);
                    let end = crate::activity::ToolEnd::bounded("tool-1", true, "ok").unwrap();
                    sink.tool_finished(end);
                }
                let _ = sink.delta(&text);
                if let Some(tokens) = *self.tokens.lock().unwrap() {
                    sink.usage(vec![UsageReport {
                        receipt_key: "turn.completed:1".into(),
                        model: None,
                        input_tokens: Some(tokens),
                        cache_read_tokens: None,
                        cache_write_tokens: None,
                        output_tokens: None,
                        reasoning_tokens: None,
                        cost: None,
                        occurred_at_ms: None,
                    }]);
                }
                sink.complete();
            }
            None => self.held.lock().unwrap().push(sink),
        }
        Ok(Box::new(Control(self.cancelled.clone())))
    }
}

struct Directory;

impl WorkstreamDirectory for Directory {
    fn exists(&self, workstream_id: &str) -> Result<bool, RpcError> {
        Ok(workstream_id == "w1")
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    api: DaemonApi,
    clock: Arc<ManualClock>,
    pulse: Arc<ActivityBoard>,
    probe: Arc<Probe>,
    wake: Arc<WakeService>,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let mut api = DaemonApi::new(directory.path()).unwrap();
    let probe = Probe::new();
    api.conversations = ConversationService::new(
        api.store.clone(),
        probe.clone(),
        Arc::new(Directory),
        true,
        api.config.load.clone(),
    );
    api.store
        .register_project(&Project {
            id: "p".into(),
            root_path: "/repo".into(),
            settings: json!({}),
        })
        .unwrap();
    let clock = Arc::new(ManualClock::new(crate::executor::unix_now_ms()));
    let pulse = Arc::new(ActivityBoard::default());
    let wake = Arc::new(service(&api, &clock, &pulse));
    api.wake = Some(wake.clone());
    Fixture {
        _directory: directory,
        api,
        clock,
        pulse,
        probe,
        wake,
    }
}

fn service(api: &DaemonApi, clock: &Arc<ManualClock>, pulse: &Arc<ActivityBoard>) -> WakeService {
    WakeService::new(
        api.store.clone(),
        api.conversations.clone(),
        pulse.clone(),
        clock.clone(),
        api.config.load.clone(),
        Diagnostics::memory(),
    )
}

fn contract(id: &str, objective: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: "p".into(),
        parent_id: None,
        objective: objective.into(),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::Commands {
            commands: vec![vec!["true".into()]],
        },
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 1,
        max_repairs: 1,
        model_selection: None,
        permission: Default::default(),
        work_item: None,
    }
}

/// A task held Queued: no attempt, no session.
fn queue_task(f: &Fixture, id: &str, objective: &str) {
    f.api.store.create(&contract(id, objective)).unwrap();
    f.api.store.transition(id, TaskEvent::Admit).unwrap();
}

/// A task with a Running attempt and a Spawning session, the way the executor leaves it.
fn run_task(f: &Fixture, id: &str, objective: &str) {
    run_contract(f, contract(id, objective));
}

fn run_contract(f: &Fixture, contract: TaskContract) {
    let id = contract.id.as_str();
    f.api.store.create(&contract).unwrap();
    f.api.store.transition(id, TaskEvent::Admit).unwrap();
    let attempt = format!("{id}-a1");
    let store = &f.api.store;
    store
        .add_attempt(
            id,
            &Attempt {
                id: attempt.clone(),
                task_id: id.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    store
        .update_attempt_state(id, &attempt, AttemptState::Running)
        .unwrap();
    store.transition(id, TaskEvent::ExecutionStarted).unwrap();
    store
        .add_session(
            id,
            &Session {
                id: format!("{id}-s1"),
                attempt_id: attempt,
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
}

fn fail_task(f: &Fixture, id: &str, objective: &str) {
    run_task(f, id, objective);
    f.api
        .store
        .record_attempt_failure(id, &format!("{id}-a1"), "boom")
        .unwrap();
    f.api
        .store
        .transition(id, TaskEvent::ExecutionFailed)
        .unwrap();
}

fn finish_task(f: &Fixture, id: &str) {
    f.api
        .store
        .update_attempt_state(id, &format!("{id}-a1"), AttemptState::CandidateReady)
        .unwrap();
    f.api
        .store
        .transition(id, TaskEvent::CandidateReady)
        .unwrap();
    f.api
        .store
        .transition(id, TaskEvent::VerificationOutcome { passed: true })
        .unwrap();
}

fn finish_scout(f: &Fixture, id: &str, body: &str) {
    let mut scout = contract(id, "Find the slow query");
    scout.kind = TaskKind::Investigate;
    scout.delivery = DeliveryPolicy::ReportOnly;
    run_contract(f, scout);
    finish_task(f, id);
    crate::executor::record_report(
        &f.api.store,
        &Diagnostics::memory(),
        id,
        &format!("{id}-a1"),
        Some(body),
    );
}

fn tick(f: &Fixture) -> TickReport {
    f.wake.tick().unwrap()
}

fn advance(f: &Fixture, ms: u64) {
    f.clock.advance(ms);
}

/// Waits out the debounce and ticks: the batch starts.
fn start_batch(f: &Fixture) -> TickReport {
    advance(f, DEBOUNCE_MS);
    tick(f)
}

fn global_records(f: &Fixture) -> Vec<WakeRecord> {
    f.api.store.wake_records_page(GLOBAL_SCOPE, 50).unwrap().0
}

fn records_of(f: &Fixture, kind: WakeKind) -> Vec<WakeRecord> {
    global_records(f)
        .into_iter()
        .filter(|record| record.kind == kind)
        .collect()
}

fn global() -> Value {
    json!({"kind": "global"})
}

fn get(f: &Fixture) -> Value {
    f.api
        .dispatch(
            "conversations.get",
            json!({"target": global(), "after_sequence": 0}),
        )
        .unwrap()
}

fn monitoring(f: &Fixture) -> Value {
    f.api.dispatch("monitoring.get", json!({})).unwrap()
}

fn set_mode(f: &Fixture, mode: &str) -> Result<Value, RpcError> {
    f.api.dispatch(
        "monitoring.set",
        json!({"idempotency_key": format!("mode-{mode}"), "mode": mode}),
    )
}

fn set_settings(f: &Fixture, key: &str, patch: Value) -> Result<Value, RpcError> {
    let mut params = patch;
    params["idempotency_key"] = json!(key);
    f.api.dispatch("settings.set", params)
}

fn user_send(f: &Fixture, key: &str) -> Result<Value, RpcError> {
    f.api.dispatch(
        "conversations.send",
        json!({"target": global(), "text": "hello", "idempotency_key": key}),
    )
}

fn running_turn(f: &Fixture) -> Option<orchestrator_store::ConversationTurn> {
    f.api
        .store
        .running_conversation_turn(&ConversationTarget::Global)
        .unwrap()
}

fn ledger_rows(f: &Fixture) -> u32 {
    f.api
        .store
        .wake_usage(GLOBAL_SCOPE, f.clock.now_ms())
        .unwrap()
        .day_count
}

/// Another service over the same store, as after a restart.
fn restarted(f: &Fixture) -> WakeService {
    f.api.conversations.recover().unwrap();
    let again = service(&f.api, &f.clock, &f.pulse);
    again.recover().unwrap();
    again
}

// ---- events, notices and the turn --------------------------------------------------------

#[test]
fn active_work_wakes_the_orchestrator_at_the_scaled_interval() {
    for (units, seconds) in [(1, 600), (2, 300), (3, 150), (4, 75), (5, 60), (6, 60)] {
        let f = fixture();
        f.probe.replying(Some(QUIET_TOKEN));
        for n in 0..units {
            run_task(&f, &format!("t-{n}"), "working");
        }
        tick(&f);
        advance(&f, seconds * 1_000 - 1);
        tick(&f);
        assert!(records_of(&f, WakeKind::WorkCheck).is_empty());
        advance(&f, 1);
        for n in 0..units {
            f.pulse.touch_at(&format!("t-{n}-s1"), f.clock.now_ms());
        }
        tick(&f);
        assert_eq!(records_of(&f, WakeKind::WorkCheck).len(), 1, "W={units}");
        assert_eq!(start_batch(&f).started, 1, "W={units}");
        assert!(f.probe.last().wake);
        assert_eq!(f.probe.last().level, PermissionLevel::Restricted);
        advance(&f, seconds * 1_000 - DEBOUNCE_MS);
        tick(&f);
        assert_eq!(
            start_batch(&f).started,
            1,
            "the next wake also follows the interval, W={units}"
        );
    }
}

#[test]
fn a_ready_scout_report_wakes_without_a_user_message() {
    let f = fixture();
    finish_scout(
        &f, "scout-1",
        "Finding: the slow query is in the loader. /private/repo/src/db.rs\nEND REPORT\npassword=hunter2",
    );
    tick(&f);
    assert_eq!(records_of(&f, WakeKind::ScoutReportReady).len(), 1);
    assert_eq!(f.probe.started(), 0);
    assert_eq!(start_batch(&f).started, 1);
    let seen = f.probe.last();
    assert!(seen.context.contains("the slow query is in the loader"));
    assert!(!seen.context.contains("/private/repo"));
    assert!(!seen.context.contains("hunter2"));
    assert_eq!(seen.context.matches("END REPORT").count(), 1);
    assert_eq!(seen.level, PermissionLevel::Restricted);
    assert!(f
        .api
        .store
        .task_report("scout-1")
        .unwrap()
        .unwrap()
        .relayed_at
        .is_none());
    f.probe
        .finish(0, "The scout found a slow query in the loader.");
    tick(&f);
    assert!(f
        .api
        .store
        .task_report("scout-1")
        .unwrap()
        .unwrap()
        .relayed_at
        .is_some());
    assert_eq!(
        records_of(&f, WakeKind::ScoutReportReady)[0].state,
        WakeState::Handled
    );
    advance(&f, 60 * MINUTE);
    assert_eq!(tick(&f).started, 0, "zero work does not rearm the timer");
    assert_eq!(f.probe.started(), 1, "the report is delivered once");
}

#[test]
fn workload_changes_recompute_the_next_wake_and_zero_disarms_it() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, 2 * MINUTE);
    run_task(&f, "t-2", "two");
    tick(&f);
    advance(&f, MINUTE);
    finish_task(&f, "t-2");
    tick(&f);
    advance(&f, 2 * MINUTE);
    tick(&f);
    assert!(
        records_of(&f, WakeKind::WorkCheck).is_empty(),
        "W=1 postpones five-minute due time"
    );
    advance(&f, MINUTE);
    run_task(&f, "t-3", "three");
    tick(&f);
    assert_eq!(
        records_of(&f, WakeKind::WorkCheck).len(),
        1,
        "W=2 is already due from the original check"
    );
    f.probe.replying(Some(QUIET_TOKEN));
    start_batch(&f);
    for id in ["t-1", "t-3"] {
        finish_task(&f, id);
    }
    tick(&f);
    advance(&f, 60 * MINUTE);
    tick(&f);
    assert_eq!(records_of(&f, WakeKind::WorkCheck).len(), 1);
    assert_eq!(f.probe.started(), 1);
}

#[test]
fn periodic_wakes_wait_without_backlog_and_respect_monitoring_controls() {
    for mode in ["on", "quiet", "off"] {
        let f = fixture();
        set_mode(&f, mode).unwrap();
        run_task(&f, "t-1", "one");
        tick(&f);
        user_send(&f, "u-1").unwrap();
        for _ in 0..4 {
            advance(&f, 10 * MINUTE);
            tick(&f);
        }
        assert_eq!(f.probe.started(), 1, "user turn takes precedence");
        assert_eq!(
            records_of(&f, WakeKind::WorkCheck).len(),
            usize::from(mode != "off")
        );
        assert_eq!(ledger_rows(&f), 0);
        f.probe.finish(0, "done");
        tick(&f);
        assert_eq!(f.probe.started(), if mode == "off" { 1 } else { 2 });
        if mode != "off" {
            assert_eq!(
                records_of(&f, WakeKind::WorkCheck)[0].notify_reply,
                mode == "on"
            );
            f.probe.finish(1, "Still working.");
            tick(&f);
            set_settings(&f, "cap", json!({"wake_limits": {"hourly_cap": 1}})).unwrap();
            advance(&f, 10 * MINUTE);
            tick(&f);
            start_batch(&f);
            assert_eq!(f.probe.started(), 2, "periodic turns obey the cap");
        }
    }
}

#[test]
fn report_wakes_recover_and_relay_one_bounded_report_per_turn() {
    let f = fixture();
    f.api
        .store
        .create_workstream(
            &Workstream {
                id: "w1".into(),
                title: "Workstream".into(),
                summary: String::new(),
                state: WorkstreamState::Active,
                revision: 1,
            },
            "test",
        )
        .unwrap();
    finish_scout(&f, "scout-1", "First finding.");
    finish_scout(&f, "scout-2", "Second finding.");
    f.api.store.link_task("w1", "scout-1", None).unwrap();
    tick(&f);
    assert_eq!(records_of(&f, WakeKind::ScoutReportReady).len(), 2);
    start_batch(&f);
    let turns = f.probe.seen.lock().unwrap().clone();
    assert!(
        turns
            .iter()
            .all(|turn| turn.context.matches("BEGIN REPORT").count() <= 1),
        "one report per turn"
    );
    // A candidate's block shares the delimiters, so count the report header only.
    assert_eq!(
        turns
            .iter()
            .filter(|turn| turn.context.contains("may have read untrusted text"))
            .count(),
        1,
        "the second report waits for the next turn"
    );
    let again = restarted(&f);
    f.probe.replying(Some("Here are the findings."));
    advance(&f, DEBOUNCE_MS);
    again.tick().unwrap();
    advance(&f, DEBOUNCE_MS);
    again.tick().unwrap();
    again.tick().unwrap();
    for id in ["scout-1", "scout-2"] {
        assert!(f
            .api
            .store
            .task_report(id)
            .unwrap()
            .unwrap()
            .relayed_at
            .is_some());
    }
    assert_eq!(
        records_of(&f, WakeKind::ScoutReportReady)
            .iter()
            .filter(|r| r.state == WakeState::Handled)
            .count(),
        2
    );
}

#[test]
fn failed_or_quiet_report_wakes_do_not_mark_findings_relayed() {
    for reply in [None, Some(QUIET_TOKEN), Some("")] {
        let f = fixture();
        finish_scout(&f, "scout-1", "A finding.");
        tick(&f);
        start_batch(&f);
        if let Some(reply) = reply {
            f.probe.finish(0, reply);
        } else {
            let sink = f.probe.held.lock().unwrap()[0].clone();
            sink.fail(RuntimeFailure::Failed);
        }
        tick(&f);
        assert!(f
            .api
            .store
            .task_report("scout-1")
            .unwrap()
            .unwrap()
            .relayed_at
            .is_none());
    }
}

#[test]
fn a_failed_task_writes_a_notice_before_any_wake_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "Fix the login bug");

    tick(&f);

    let records = records_of(&f, WakeKind::TaskFailed);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].notice,
        "Task t-1 (Fix the login bug) failed: boom"
    );
    assert_eq!(records[0].state, WakeState::Pending);
    assert_eq!(f.probe.started(), 0, "no turn before the window ends");
    let anchor = records[0].anchor_sequence;

    assert_eq!(start_batch(&f).started, 1);
    let page = f
        .api
        .store
        .conversation_page(&ConversationTarget::Global, 0, 10)
        .unwrap();
    assert!(page.entries[0].sequence > anchor);
    assert_eq!(
        page.entries[0].body,
        "Task t-1 (Fix the login bug) failed: boom"
    );
}

#[test]
fn events_inside_sixty_seconds_share_one_wake_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, 20_000);
    fail_task(&f, "t-2", "two");
    tick(&f);
    advance(&f, 30_000);
    fail_task(&f, "t-3", "three");
    tick(&f);
    advance(&f, 9_000);
    tick(&f);
    assert_eq!(f.probe.started(), 0, "no turn at 59 s");

    advance(&f, 1_000);
    assert_eq!(tick(&f).started, 1);

    assert_eq!(f.probe.started(), 1);
    let page = f
        .api
        .store
        .conversation_page(&ConversationTarget::Global, 0, 10)
        .unwrap();
    assert_eq!(page.entries[0].body.lines().count(), 3);
}

#[test]
fn one_wake_turn_per_target_at_a_time() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    assert_eq!(f.probe.started(), 1);

    fail_task(&f, "t-2", "two");
    tick(&f);
    start_batch(&f);
    assert_eq!(f.probe.started(), 1, "the second batch waits");

    f.probe.finish(0, "noted");
    let report = tick(&f);

    assert!(report.settled >= 1);
    assert_eq!(f.probe.started(), 2);
}

#[test]
fn a_user_turn_in_flight_holds_the_wake() {
    let f = fixture();
    user_send(&f, "u-1").unwrap();
    assert_eq!(f.probe.started(), 1);
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    assert_eq!(f.probe.started(), 1, "the wake waits for the user's turn");
    assert_eq!(
        records_of(&f, WakeKind::TaskFailed)[0].state,
        WakeState::Pending
    );

    f.probe.finish(0, "hi");
    tick(&f);

    assert_eq!(f.probe.started(), 2);
    assert!(f.probe.last().wake);
}

#[test]
fn a_user_message_cancels_a_running_wake_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    let wake_turn = running_turn(&f).unwrap();
    assert_eq!(wake_turn.trigger, TurnTrigger::Wake);

    let sent = user_send(&f, "u-1").unwrap();

    assert_eq!(sent["turn"]["trigger"], "user");
    let cancelled = f
        .api
        .store
        .conversation_turn(&wake_turn.id)
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.state, orchestrator_store::TurnState::Cancelled);
    assert_eq!(running_turn(&f).unwrap().trigger, TurnTrigger::User);
    tick(&f);
    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Pending);
    assert_eq!(record.attempts, 0);
}

#[test]
fn a_wake_turn_cancelled_by_a_user_message_runs_again_after_the_user_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    let first = records_of(&f, WakeKind::TaskFailed)[0].turn_id.clone();
    user_send(&f, "u-1").unwrap();
    tick(&f);
    f.probe.finish(1, "hi");
    f.probe.replying(Some("Task one failed."));

    start_batch(&f);
    tick(&f);

    assert_eq!(f.probe.started(), 3, "wake, user, then a second wake turn");
    assert!(f.probe.last().wake);
    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Handled);
    assert_ne!(record.turn_id, first, "the retry is a new turn");
    assert_eq!(ledger_rows(&f), 2, "one ledger row per started turn");
}

#[test]
fn a_user_message_over_a_running_user_turn_is_still_refused() {
    let f = fixture();
    user_send(&f, "u-1").unwrap();
    assert_eq!(user_send(&f, "u-2"), Err(RpcError::ConversationBusy));
}

#[test]
fn conversations_clear_cancels_a_running_wake_turn_first() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    assert!(running_turn(&f).is_some());

    let cleared = f
        .api
        .dispatch(
            "conversations.clear",
            json!({"target": global(), "idempotency_key": "c-1"}),
        )
        .unwrap();

    assert_eq!(cleared["cleared"], true);
    assert!(running_turn(&f).is_none());
}

#[test]
fn a_wake_turn_always_runs_at_restricted() {
    let f = fixture();
    // The user's last choice was Full.
    f.api
        .conversations
        .start(
            &ConversationTarget::Global,
            "u-1",
            "hello",
            None,
            PermissionLevel::Full,
            true,
        )
        .unwrap();
    assert_eq!(f.probe.last().level, PermissionLevel::Full);
    f.probe.finish(0, "hi");
    fail_task(&f, "t-1", "one");
    tick(&f);

    start_batch(&f);

    let seen = f.probe.last();
    assert!(seen.wake);
    assert_eq!(seen.level, PermissionLevel::Restricted);
    assert!(seen.actions, "a block becomes proposals that need the user");
    let turn = running_turn(&f).unwrap();
    assert_eq!(turn.trigger, TurnTrigger::Wake);
}

#[test]
fn a_wake_turn_gets_the_snapshot_and_no_paths() {
    let f = fixture();
    let dir = f._directory.path().display().to_string();
    fail_task(&f, "t-1", "Fix /Users/me/secret/repo/main.rs please");
    queue_task(&f, "t-2", "Also C:\\work\\private\\x.txt and ~/notes");
    tick(&f);

    start_batch(&f);

    let seen = f.probe.last();
    assert!(seen.context.contains("Monitoring: on"), "{}", seen.context);
    for needle in [
        "/Users/me",
        "secret/repo",
        "C:\\work",
        "~/notes",
        dir.as_str(),
    ] {
        assert!(!seen.prompt.contains(needle), "{needle} in {}", seen.prompt);
    }
    assert!(!seen.prompt.contains("daemon.token"));
}

#[test]
fn a_long_secret_like_word_in_another_tasks_title_never_reaches_the_wake_prompt() {
    let f = fixture();
    let secret = "sk-abcdefghijklmnopqrstuvwxyz0123456789ABCD";
    queue_task(&f, "t-2", &format!("Rotate {secret} soon"));
    fail_task(&f, "t-1", "one");
    tick(&f);

    start_batch(&f);

    let seen = f.probe.last();
    assert!(!seen.context.contains(secret), "{}", seen.context);
    assert!(!seen.prompt.contains(secret), "{}", seen.prompt);
    assert!(seen.context.contains("[redacted]"), "{}", seen.context);
    // Ids stay.
    assert!(seen.context.contains("t-2"), "{}", seen.context);
}

#[test]
fn a_path_in_a_task_title_never_reaches_a_notice_or_the_prompt() {
    let f = fixture();
    fail_task(&f, "t-1", "Fix /etc/passwd and /home/me/.ssh/id_rsa now");
    tick(&f);
    let notice = &records_of(&f, WakeKind::TaskFailed)[0].notice;
    assert!(
        !notice.contains("/etc") && !notice.contains(".ssh"),
        "{notice}"
    );

    start_batch(&f);

    let prompt = f.probe.last().prompt;
    assert!(
        !prompt.contains("/etc") && !prompt.contains(".ssh"),
        "{prompt}"
    );
    assert!(prompt.contains("[path]"));
}

#[test]
fn the_wake_turn_is_marked_and_labelled_as_an_update() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);

    let page = get(&f);

    assert_eq!(page["turn"]["trigger"], "wake");
    let opening = &page["entries"][0];
    assert_eq!(opening["kind"], "user_message");
    assert_eq!(opening["origin"], "wake");
    assert_eq!(opening["body"], "Task t-1 (one) failed: boom");
    assert_eq!(
        page["wake_records"][0]["notice"],
        "Task t-1 (one) failed: boom"
    );
    assert_eq!(page["wake_records"][0]["event"], "task_failed");
    assert_eq!(page["wake_records"][0]["state"], "running");
    // A user message carries no origin.
    f.probe.finish(0, "hello");
    tick(&f);
    user_send(&f, "u-1").unwrap();
    let page = get(&f);
    let user = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .rfind(|entry| entry["kind"] == "user_message")
        .unwrap();
    assert!(user.get("origin").is_none(), "{user}");
}

#[test]
fn the_wake_block_is_short_and_outside_the_preamble() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    let wake = f.probe.last().prompt;
    f.probe.finish(0, "x");
    tick(&f);
    user_send(&f, "u-1").unwrap();
    let user = f.probe.last().prompt;

    assert!(wake.contains("automatic update from the app"));
    assert!(!user.contains("automatic update from the app"));

    // A work check is the batch that is offered the quiet token. The block with the offer stays
    // under 600 bytes.
    let check = fixture();
    run_task(&check, "t-1", "working");
    tick(&check);
    advance(&check, 600 * 1_000 - 1);
    tick(&check);
    advance(&check, 1);
    check.pulse.touch_at("t-1-s1", check.clock.now_ms());
    tick(&check);
    start_batch(&check);
    let heartbeat = check.probe.last().prompt;
    let block_start = heartbeat.find("This is an automatic update").unwrap();
    let anchor = "MCAO_QUIET and nothing else.";
    let block_end = heartbeat[block_start..].find(anchor).unwrap() + anchor.len();
    assert!(block_end < 600, "{block_end}");
}

#[test]
fn the_quiet_token_closes_a_heartbeat_as_quiet() {
    let f = fixture();
    f.probe.replying(Some(QUIET_TOKEN));
    run_task(&f, "t-1", "working");
    tick(&f);
    advance(&f, 600 * 1_000 - 1);
    tick(&f);
    advance(&f, 1);
    f.pulse.touch_at("t-1-s1", f.clock.now_ms());
    tick(&f);
    start_batch(&f);
    tick(&f);

    let record = &records_of(&f, WakeKind::WorkCheck)[0];
    assert_eq!(record.state, WakeState::Handled);
    assert_eq!(record.outcome, Some(WakeOutcome::Quiet));
    let page = get(&f);
    assert!(
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["kind"] != "reply_chunk"),
        "{page}"
    );

    f.probe.replying(Some("Task two needs a look."));
    fail_task(&f, "t-2", "two");
    tick(&f);
    start_batch(&f);
    tick(&f);
    let second = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(second.outcome, Some(WakeOutcome::Replied));
}

#[test]
fn a_quiet_turn_hides_its_thinking_and_tool_entries_with_its_reply() {
    let f = fixture();
    f.probe.activity.store(true, Ordering::SeqCst);
    f.probe.replying(Some(QUIET_TOKEN));
    work_check(&f);
    start_batch(&f);
    tick(&f);
    let record = &records_of(&f, WakeKind::WorkCheck)[0];
    assert_eq!(record.outcome, Some(WakeOutcome::Quiet));
    let page = get(&f);
    for hidden in [
        "reply_chunk",
        "thinking_chunk",
        "tool_started",
        "tool_finished",
    ] {
        assert!(
            page["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["kind"] != hidden),
            "{hidden} shown: {page}"
        );
    }

    // A turn that answered keeps its activity on the wire.
    f.probe.replying(Some("Task two needs a look."));
    fail_task(&f, "t-2", "two");
    tick(&f);
    start_batch(&f);
    tick(&f);
    let page = get(&f);
    let kinds: Vec<&str> = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap())
        .collect();
    for shown in [
        "reply_chunk",
        "thinking_chunk",
        "tool_started",
        "tool_finished",
    ] {
        assert!(kinds.contains(&shown), "{shown} missing: {page}");
    }
}

/// Runs one work check of a single running task, so the batch is a heartbeat.
fn work_check(f: &Fixture) {
    run_task(f, "t-1", "working");
    tick(f);
    advance(f, 600 * 1_000 - 1);
    tick(f);
    advance(f, 1);
    f.pulse.touch_at("t-1-s1", f.clock.now_ms());
    tick(f);
}

#[test]
fn a_candidate_ready_task_writes_a_notice_and_wakes_with_its_summary_as_evidence() {
    let f = fixture();
    run_task(&f, "t-1", "Fix the login bug");
    f.api
        .store
        .record_attempt_summary("t-1", "t-1-a1", Some("Changed the login check."), Some(2))
        .unwrap();
    f.api
        .store
        .update_attempt_state("t-1", "t-1-a1", AttemptState::CandidateReady)
        .unwrap();
    f.api
        .store
        .transition("t-1", TaskEvent::CandidateReady)
        .unwrap();

    tick(&f);

    let records = records_of(&f, WakeKind::CandidateReady);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].notice,
        "Task t-1 (Fix the login bug) finished with a candidate (2 files changed) and waits for review."
    );
    assert_eq!(records[0].state, WakeState::Pending);
    assert_eq!(f.probe.started(), 0, "no turn before the window ends");

    assert_eq!(start_batch(&f).started, 1);
    let seen = f.probe.last();
    assert!(seen.wake);
    assert_eq!(seen.level, PermissionLevel::Restricted);
    assert!(
        seen.context.contains("Changed the login check."),
        "{}",
        seen.context
    );
    assert_eq!(seen.context.matches("BEGIN REPORT").count(), 1);
    assert!(
        seen.context.contains("Files changed: 2"),
        "{}",
        seen.context
    );
    // A candidate is not a report: the turn is not marked as one that read a report.
    let turn = running_turn(&f).unwrap();
    assert!(!f.api.store.turn_read_report(&turn.id).unwrap());
}

#[test]
fn a_candidate_and_a_report_in_one_batch_relay_only_the_report() {
    let f = fixture();
    finish_scout(&f, "scout-1", "The loader is slow.");
    run_task(&f, "t-1", "Fix the login bug");
    f.api
        .store
        .record_attempt_summary("t-1", "t-1-a1", Some("Changed the login check."), Some(2))
        .unwrap();
    f.api
        .store
        .update_attempt_state("t-1", "t-1-a1", AttemptState::CandidateReady)
        .unwrap();
    f.api
        .store
        .transition("t-1", TaskEvent::CandidateReady)
        .unwrap();
    f.probe.replying(Some("Here are the findings."));
    tick(&f);

    assert_eq!(start_batch(&f).started, 1);
    tick(&f);

    let seen = f.probe.last();
    assert_eq!(
        seen.context.matches("BEGIN REPORT").count(),
        1,
        "{}",
        seen.context
    );
    assert!(
        seen.context.contains("The loader is slow."),
        "{}",
        seen.context
    );
    assert!(
        !seen.context.contains("Changed the login check."),
        "{}",
        seen.context
    );
    assert!(f
        .api
        .store
        .task_report("scout-1")
        .unwrap()
        .unwrap()
        .relayed_at
        .is_some());
}

#[test]
fn a_report_only_task_reaching_verifying_gets_no_candidate_wake() {
    let f = fixture();
    finish_scout(&f, "scout-1", "A finding.");
    tick(&f);

    assert!(records_of(&f, WakeKind::CandidateReady).is_empty());
    assert_eq!(records_of(&f, WakeKind::ScoutReportReady).len(), 1);
    assert_eq!(start_batch(&f).started, 1);
    let turns = f.probe.seen.lock().unwrap().clone();
    assert_eq!(turns.len(), 1, "one turn, for the report");
    assert!(turns[0].context.contains("may have read untrusted text"));
    assert!(!turns[0].context.contains("Files changed"));
}

#[test]
fn a_quiet_reply_leaves_an_outcome_record_unanswered_and_notifying() {
    let f = fixture();
    f.probe.replying(Some(QUIET_TOKEN));
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    tick(&f);

    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Handled);
    assert_eq!(record.outcome, Some(WakeOutcome::Unanswered));
    let page = get(&f);
    assert_eq!(page["wake_records"][0]["outcome"], "unanswered");
    assert_eq!(page["wake_records"][0]["notify"], true);
    assert_eq!(
        page["wake_records"][0]["notice"],
        "Task t-1 (one) failed: boom"
    );
    assert!(
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["kind"] != "reply_chunk"),
        "{page}"
    );
    assert_eq!(
        monitoring(&f)["unseen"],
        json!([{"target": global(), "count": 1}])
    );
}

#[test]
fn the_quiet_offer_is_absent_when_the_batch_holds_an_outcome() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);

    let prompt = f.probe.last().prompt;

    assert!(prompt.contains("automatic update from the app"), "{prompt}");
    assert!(!prompt.contains("MCAO_QUIET"), "{prompt}");
}

#[test]
fn the_quiet_offer_is_present_for_a_pure_work_check() {
    let f = fixture();
    work_check(&f);
    start_batch(&f);

    let prompt = f.probe.last().prompt;

    assert!(prompt.contains("MCAO_QUIET"), "{prompt}");
}

#[test]
fn a_quiet_or_failed_report_wake_requeues_then_fails_with_a_deterministic_notice() {
    for quiet in [true, false] {
        let f = fixture();
        finish_scout(&f, "scout-1", "A finding.");
        tick(&f);
        for try_number in 1..=2 {
            start_batch(&f);
            // The held turn ends the way this case says: quiet, or failed.
            if quiet {
                f.probe.finish(f.probe.started() - 1, QUIET_TOKEN);
                tick(&f);
            } else {
                fail_newest_turn(&f);
            }
            assert!(
                f.api
                    .store
                    .task_report("scout-1")
                    .unwrap()
                    .unwrap()
                    .relayed_at
                    .is_none(),
                "try {try_number}, quiet={quiet}"
            );
        }

        let record = &records_of(&f, WakeKind::ScoutReportReady)[0];
        assert_eq!(record.state, WakeState::Failed, "quiet={quiet}");
        assert_eq!(
            record.notice,
            "Report from task scout-1 is ready; automatic relay failed. Open the task to read it.",
            "quiet={quiet}"
        );
        assert_eq!(
            f.probe.started(),
            2,
            "two tries, then no third: quiet={quiet}"
        );
    }
}

#[test]
fn a_work_check_notice_reads_as_a_user_line() {
    let f = fixture();
    work_check(&f);

    let record = &records_of(&f, WakeKind::WorkCheck)[0];
    assert_eq!(record.notice, "Checking on 1 active work unit.");
    start_batch(&f);

    let page = get(&f);
    assert_eq!(
        page["entries"][0]["body"],
        "Checking on 1 active work unit."
    );
    assert!(
        f.probe.last().prompt.contains("name anything failed"),
        "the instruction is in the prompt"
    );
}

#[test]
fn quiet_mode_stores_the_reply_without_a_badge() {
    let f = fixture();
    set_mode(&f, "quiet").unwrap();
    f.probe.replying(Some("Look at task one."));
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    tick(&f);

    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Handled);
    assert!(!record.notify_reply);
    assert_eq!(monitoring(&f)["unseen"], json!([]));
}

#[test]
fn mode_off_gives_notices_only() {
    let f = fixture();
    set_mode(&f, "off").unwrap();
    fail_task(&f, "t-1", "one");

    tick(&f);
    start_batch(&f);

    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Skipped);
    assert_eq!(record.skip_reason, Some(SkipReason::ModeOff));
    assert_eq!(f.probe.started(), 0);
    assert_eq!(ledger_rows(&f), 0);
    assert_eq!(monitoring(&f)["unseen"][0]["count"], 1);
}

#[test]
fn a_mode_set_to_off_between_the_settings_read_and_the_claim_starts_no_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, DEBOUNCE_MS);
    // The mode changes without `mode_changed`, as when the user sets Off right after a pass
    // read the settings: the record is still pending and its window is due.
    f.api
        .store
        .update_orchestrator_settings(&orchestrator_store::SettingsPatch {
            mode: Some(MonitoringMode::Off),
            ..orchestrator_store::SettingsPatch::default()
        })
        .unwrap();

    let mut report = TickReport::default();
    f.wake.start_ready(f.clock.now_ms(), &mut report).unwrap();

    assert_eq!(report.started, 0);
    assert_eq!(f.probe.started(), 0);
    assert_eq!(ledger_rows(&f), 0);
    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Skipped);
    assert_eq!(record.skip_reason, Some(SkipReason::ModeOff));
}

#[test]
fn a_store_error_while_claiming_re_offers_the_records() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, DEBOUNCE_MS);
    let ids: Vec<String> = f.wake.state().debounce.take(GLOBAL_SCOPE);
    assert!(!ids.is_empty());
    assert!(f.wake.state().debounce.due(f.clock.now_ms()).is_empty());

    // The path every error after `take` goes through.
    let result: Result<(), String> = f.wake.or_offer(
        Err("cannot claim wake batch".into()),
        GLOBAL_SCOPE,
        &ids,
        f.clock.now_ms(),
    );

    assert!(result.is_err());
    assert_eq!(start_batch(&f).started, 1, "the records were not stranded");
}

#[test]
fn turning_monitoring_off_skips_pending_records_and_cancels_the_running_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    fail_task(&f, "t-2", "two");
    tick(&f);

    set_mode(&f, "off").unwrap();
    tick(&f);

    assert!(running_turn(&f).is_none());
    let records = records_of(&f, WakeKind::TaskFailed);
    assert!(records
        .iter()
        .all(|record| record.state == WakeState::Skipped));
    assert!(records
        .iter()
        .all(|record| record.skip_reason == Some(SkipReason::ModeOff)));
}

#[test]
fn monitoring_set_accepts_away_and_rejects_unknown_words() {
    let f = fixture();
    for bad in ["sometimes", "", "Away"] {
        assert_eq!(set_mode(&f, bad), Err(RpcError::InvalidParams), "{bad}");
    }
    assert_eq!(
        f.api.dispatch(
            "monitoring.set",
            json!({"idempotency_key": "k", "mode": "on", "hourly_cap": 3})
        ),
        Err(RpcError::InvalidParams)
    );
    let away = set_mode(&f, "away").unwrap();
    assert_eq!(away["mode"], "away");
    assert_eq!(away["away_since_ms"], f.clock.now_ms());
    assert_eq!(away["brief_pending"], false);
    assert_eq!(set_mode(&f, "quiet").unwrap()["mode"], "quiet");
    assert_eq!(monitoring(&f)["authority"], "none");
}

#[test]
fn monitoring_ack_marks_the_scope_seen() {
    let f = fixture();
    set_mode(&f, "off").unwrap();
    fail_task(&f, "t-1", "one");
    tick(&f);
    assert_eq!(monitoring(&f)["unseen"][0]["count"], 1);

    let acked = f
        .api
        .dispatch(
            "monitoring.ack",
            json!({"target": global(), "idempotency_key": "a-1"}),
        )
        .unwrap();

    assert_eq!(acked["acked"], 1);
    assert_eq!(monitoring(&f)["unseen"], json!([]));
}

#[test]
fn a_workstream_event_also_notices_global() {
    let f = fixture();
    f.api
        .store
        .create_workstream(
            &Workstream {
                id: "w1".into(),
                title: "w1".into(),
                summary: String::new(),
                state: WorkstreamState::Active,
                revision: 1,
            },
            "test",
        )
        .unwrap();
    queue_task(&f, "t-1", "one");
    f.api.store.link_task("w1", "t-1", None).unwrap();
    f.api
        .store
        .add_attempt(
            "t-1",
            &Attempt {
                id: "t-1-a1".into(),
                task_id: "t-1".into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    f.api
        .store
        .update_attempt_state("t-1", "t-1-a1", AttemptState::Running)
        .unwrap();
    f.api
        .store
        .transition("t-1", TaskEvent::ExecutionStarted)
        .unwrap();
    f.api
        .store
        .transition("t-1", TaskEvent::ExecutionFailed)
        .unwrap();

    tick(&f);

    let own = f
        .api
        .store
        .wake_records_page("conversation:workstream:w1", 50)
        .unwrap()
        .0;
    assert_eq!(own.len(), 1);
    assert!(own[0].needs_model);
    let copy = records_of(&f, WakeKind::TaskFailed);
    assert_eq!(copy.len(), 1);
    assert!(!copy[0].needs_model);
    assert!(copy[0].dedupe_key.starts_with("up:"));
    assert_eq!(copy[0].state, WakeState::Noticed);
}

// ---- limits ------------------------------------------------------------------------------

#[test]
fn wake_turns_stop_at_the_default_hourly_cap() {
    let f = fixture();
    f.probe.replying(Some("ok"));
    let cap = DEFAULT_WAKE_HOURLY_CAP as usize;
    for index in 0..cap {
        fail_task(&f, &format!("t-{index}"), "Fix it");
        tick(&f);
        assert_eq!(start_batch(&f).started, 1, "turn {index}");
    }
    assert_eq!(f.probe.started(), cap);

    let over = format!("t-{cap}");
    fail_task(&f, &over, "Fix it");
    tick(&f);
    let report = start_batch(&f);

    assert_eq!(report.started, 0);
    assert_eq!(f.probe.started(), cap, "nothing spent past the cap");
    let seventh = records_of(&f, WakeKind::TaskFailed)
        .into_iter()
        .find(|record| record.subject == over)
        .unwrap();
    assert_eq!(seventh.state, WakeState::Skipped);
    assert_eq!(seventh.skip_reason, Some(SkipReason::LimitHourly));
    // The notice still reads in the transcript.
    let page = get(&f);
    let notices: Vec<&Value> = page["wake_records"].as_array().unwrap().iter().collect();
    assert!(notices
        .iter()
        .any(|record| record["subject"] == over.as_str()));
    assert_eq!(ledger_rows(&f), DEFAULT_WAKE_HOURLY_CAP);
    let paused = &monitoring(&f)["paused"][0];
    assert_eq!(paused["reason"], "hourly");
    let until = paused["until"].as_str().unwrap();
    assert!(until.ends_with('Z') && until.len() == 20, "{until}");
}

/// Work checks get half of each cap. One past that share is not recorded, so it leaves no
/// skipped line, and an outcome event still wakes with the rest of the cap (DPES-1108 phase 8).
#[test]
fn a_work_check_past_its_share_of_the_cap_is_not_recorded_and_an_outcome_still_wakes() {
    let f = fixture();
    set_settings(&f, "cap", json!({"wake_limits": {"hourly_cap": 2}})).unwrap();
    f.probe.replying(Some("ok"));
    work_check(&f);
    assert_eq!(start_batch(&f).started, 1);
    tick(&f);

    // The next check is due ten minutes later; the first one spent the heartbeat share.
    advance(&f, 10 * MINUTE + SLACK);
    f.pulse.touch_at("t-1-s1", f.clock.now_ms());
    tick(&f);
    assert_eq!(records_of(&f, WakeKind::WorkCheck).len(), 1);
    assert!(records_of(&f, WakeKind::WorkCheck)
        .iter()
        .all(|record| record.state != WakeState::Skipped));

    fail_task(&f, "t-2", "two");
    tick(&f);
    assert_eq!(
        start_batch(&f).started,
        1,
        "an outcome uses the rest of the cap"
    );
}

/// A completed wake turn's entry carries the recorded pull request of the task it reported,
/// built by the daemon from the pull request row and its bound repository (DPES-1108 phase 8).
#[test]
fn a_wake_turn_entry_carries_the_recorded_pull_request_link_of_its_task() {
    use orchestrator_core::landing::{CheckState, PrState};
    use orchestrator_store::{PullRequestSeen, StoredCheck};

    let f = fixture();
    f.probe.replying(Some("Fix it failed."));
    fail_task(&f, "t-1", "Fix it");
    f.api
        .store
        .observe_pull_request(
            &PullRequestSeen {
                task_id: "t-1".to_owned(),
                project_id: "p".to_owned(),
                number: 12,
                branch: "moa/t-1".to_owned(),
                base_branch: "main".to_owned(),
                state: PrState::Open,
                head_sha: "a".repeat(40),
                mergeable: Some(true),
                checks: vec![StoredCheck {
                    name: "build".to_owned(),
                    state: CheckState::Failed,
                }],
                title: "Fix it".to_owned(),
            },
            1,
        )
        .unwrap();
    f.api
        .store
        .bind_pull_request_repo("t-1", "octo", "demo")
        .unwrap();
    tick(&f);
    assert_eq!(start_batch(&f).started, 1);
    tick(&f);

    let page = get(&f);
    let completed: Vec<&Value> = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["kind"] == "turn_completed")
        .collect();
    assert_eq!(completed.len(), 1, "{page}");
    assert_eq!(
        completed[0]["pull_requests"],
        json!([{"task_id": "t-1", "number": 12, "url": "https://github.com/octo/demo/pull/12"}])
    );
    // An entry of a turn with no recorded pull request has no field at all.
    assert!(page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["kind"] != "turn_completed")
        .all(|entry| entry.get("pull_requests").is_none()));
}

#[test]
fn a_daily_token_budget_pauses_the_scope_and_a_null_clears_it() {
    let f = fixture();
    set_settings(&f, "s-1", json!({"wake_limits": {"daily_token_budget": 5}})).unwrap();
    f.probe.replying(Some("ok"));
    f.probe.reporting(Some(10));
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    fail_task(&f, "t-2", "two");
    tick(&f);

    start_batch(&f);

    assert_eq!(f.probe.started(), 1);
    let second = records_of(&f, WakeKind::TaskFailed)
        .into_iter()
        .find(|record| record.subject == "t-2")
        .unwrap();
    assert_eq!(second.skip_reason, Some(SkipReason::LimitTokens));
    let paused = &monitoring(&f)["paused"][0];
    assert_eq!(paused["reason"], "tokens");
    assert_eq!(paused["until"], Value::Null);

    set_settings(
        &f,
        "s-2",
        json!({"wake_limits": {"daily_token_budget": null}}),
    )
    .unwrap();
    assert_eq!(monitoring(&f)["paused"], json!([]));
}

#[test]
fn wake_turns_are_deferred_while_the_load_gate_is_closed() {
    let f = fixture();
    let load = f.api.config.load.clone();
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    fail_task(&f, "t-1", "one");

    tick(&f);
    for _ in 0..3 {
        advance(&f, DEBOUNCE_MS);
        assert_eq!(tick(&f).started, 0);
    }

    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(
        record.state,
        WakeState::Pending,
        "the notice stands, nothing claimed"
    );
    assert_eq!(f.probe.started(), 0);
    assert_eq!(ledger_rows(&f), 0, "no cap spent");
    assert!(running_turn(&f).is_none());
    assert!(f
        .api
        .store
        .conversation_page(&ConversationTarget::Global, 0, 5)
        .unwrap()
        .entries
        .is_empty());

    // A chat turn is never gated.
    user_send(&f, "u-1").unwrap();
    f.probe.finish(0, "hi");

    for n in 5..12 {
        load.record(cpu(0.0), at(t0, n));
    }
    assert_eq!(
        tick(&f).started,
        1,
        "the first tick after the gate opens starts it"
    );
    assert_eq!(
        ledger_rows(&f),
        1,
        "the cap is spent only when the turn starts"
    );
}

#[test]
fn a_wake_turn_starts_only_once_the_default_agent_is_settled() {
    let f = fixture();
    f.probe.unsettled.store(true, Ordering::SeqCst);
    fail_task(&f, "t-1", "one");
    tick(&f);

    assert_eq!(start_batch(&f).started, 0);
    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Pending, "nothing claimed");
    assert_eq!(ledger_rows(&f), 0, "no cap spent");

    f.probe.unsettled.store(false, Ordering::SeqCst);
    assert_eq!(tick(&f).started, 1, "the next tick starts it");
    assert_eq!(
        *f.probe.prepared.lock().unwrap(),
        [None],
        "the default agent"
    );
}

#[test]
fn a_task_held_queued_by_a_closed_gate_is_not_stalled() {
    let f = fixture();
    let load = f.api.config.load.clone();
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    queue_task(&f, "held", "waiting for the machine");

    for _ in 0..18 {
        advance(&f, 10 * MINUTE);
        tick(&f);
    }

    assert!(records_of(&f, WakeKind::TaskStalled).is_empty());
    assert!(f.api.store.supervision_load().unwrap().is_empty());
}

#[test]
fn held_tasks_add_no_work_unit() {
    let f = fixture();
    for index in 0..30 {
        queue_task(&f, &format!("held-{index}"), "waiting");
    }
    run_task(&f, "r-1", "running");
    run_task(&f, "r-2", "running");

    let load = f.api.store.supervision_load().unwrap();
    let global = load.iter().find(|load| load.scope == GLOBAL_SCOPE).unwrap();

    assert_eq!(global.work_units, 2);
    let policy = f
        .api
        .store
        .orchestrator_settings()
        .unwrap()
        .interval_policy();
    assert_eq!(policy.interval(2), Some(Duration::from_secs(300)));
}

#[test]
fn a_gate_that_closes_while_an_attempt_runs_still_flags_a_silent_attempt() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    let load = f.api.config.load.clone();
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }

    advance(&f, 10 * MINUTE + SLACK);
    tick(&f);

    assert_eq!(records_of(&f, WakeKind::TaskStalled).len(), 1);
}

#[test]
fn a_wake_turn_that_waited_for_the_gate_counts_its_cap_only_when_it_starts() {
    // Covered with the deferral test above; this pins the ledger on its own.
    let f = fixture();
    let load = f.api.config.load.clone();
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    fail_task(&f, "t-1", "one");
    tick(&f);
    for _ in 0..3 {
        start_batch(&f);
    }
    assert_eq!(ledger_rows(&f), 0);

    for n in 5..12 {
        load.record(cpu(0.0), at(t0, n));
    }
    tick(&f);

    assert_eq!(ledger_rows(&f), 1);
}

// ---- the stall check ---------------------------------------------------------------------

#[test]
fn a_stalled_task_raises_a_notice_without_a_model_turn() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);

    advance(&f, 9 * MINUTE);
    tick(&f);
    assert!(records_of(&f, WakeKind::TaskStalled).is_empty());

    advance(&f, MINUTE + SLACK);
    tick(&f);

    let stalled = records_of(&f, WakeKind::TaskStalled);
    assert_eq!(stalled.len(), 1);
    assert!(
        stalled[0].notice.contains("silent"),
        "{}",
        stalled[0].notice
    );
    assert_eq!(
        stalled[0].state,
        WakeState::Noticed,
        "a stall is a notice only"
    );
    assert_eq!(f.probe.started(), 0, "the scan starts no turn");
    assert_eq!(ledger_rows(&f), 0, "and writes no ledger row");

    // The separate periodic check, not the stall notice, starts one bounded wake.
    start_batch(&f);
    assert_eq!(f.probe.started(), 1);
    assert_eq!(ledger_rows(&f), 1);
    assert!(!records_of(&f, WakeKind::TaskStalled)[0].needs_model);
    assert_eq!(
        records_of(&f, WakeKind::WorkCheck)[0].state,
        WakeState::Running
    );
}

#[test]
fn a_stall_never_costs_a_wake_turn_or_a_cap() {
    let f = fixture();
    set_settings(
        &f,
        "long-interval",
        json!({"supervision": {
            "base_interval_secs": 14_400, "attempt_wall_minutes": 5
        }}),
    )
    .unwrap();
    run_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, 10 * MINUTE + SLACK);
    tick(&f);
    assert_eq!(records_of(&f, WakeKind::TaskStalled).len(), 1);

    // Many debounce windows later: no turn, no ledger row, no pause.
    for _ in 0..8 {
        start_batch(&f);
    }

    assert_eq!(f.probe.started(), 0);
    assert_eq!(ledger_rows(&f), 0);
    assert_eq!(monitoring(&f)["paused"], json!([]));
    // The caps are all still there for a real failure.
    fail_task(&f, "t-2", "two");
    tick(&f);
    assert_eq!(start_batch(&f).started, 1);
    assert_eq!(ledger_rows(&f), 1);
}

#[test]
fn an_attempt_over_its_wall_time_is_flagged_when_crossed() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    let touch = |f: &Fixture| f.pulse.touch_at("t-1-s1", f.clock.now_ms());
    tick(&f);
    for _ in 0..23 {
        advance(&f, 5 * MINUTE);
        touch(&f);
        tick(&f);
    }
    // 115 minutes in, then 119.
    advance(&f, 4 * MINUTE);
    touch(&f);
    tick(&f);
    assert!(records_of(&f, WakeKind::TaskStalled).is_empty());

    advance(&f, MINUTE + SLACK);
    touch(&f);
    tick(&f);

    let stalled = records_of(&f, WakeKind::TaskStalled);
    assert_eq!(
        stalled.len(),
        1,
        "flagged on the tick it crosses, not the next interval"
    );
    assert!(
        stalled[0].notice.contains("time limit"),
        "{}",
        stalled[0].notice
    );
}

#[test]
fn an_event_resets_the_quiet_period() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, 9 * MINUTE);
    f.pulse.touch_at("t-1-s1", f.clock.now_ms());
    tick(&f);

    advance(&f, MINUTE + SLACK);
    tick(&f);
    assert!(
        records_of(&f, WakeKind::TaskStalled).is_empty(),
        "one minute quiet"
    );

    advance(&f, 10 * MINUTE);
    tick(&f);
    assert_eq!(
        records_of(&f, WakeKind::TaskStalled).len(),
        1,
        "eleven minutes quiet"
    );
}

#[test]
fn stall_fires_once_per_attempt_and_rule() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    for _ in 0..6 {
        advance(&f, 10 * MINUTE + SLACK);
        tick(&f);
    }

    assert_eq!(records_of(&f, WakeKind::TaskStalled).len(), 1);
}

#[test]
fn the_stall_check_runs_at_the_scaled_interval() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    run_task(&f, "t-2", "two");
    tick(&f);

    advance(&f, 299_000);
    tick(&f);
    assert!(
        records_of(&f, WakeKind::TaskStalled).is_empty(),
        "W = 2 is not due at 299 s"
    );

    advance(&f, 3_000);
    tick(&f);
    assert_eq!(
        records_of(&f, WakeKind::TaskStalled).len(),
        2,
        "W = 2 is due at 5 minutes"
    );
}

#[test]
fn a_rise_in_work_brings_the_next_check_earlier() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, 4 * MINUTE);
    tick(&f);
    run_task(&f, "t-2", "two");
    // The session started on the real clock; the test clock is ahead, so mark it alive now.
    f.pulse.touch_at("t-2-s1", f.clock.now_ms());

    advance(&f, MINUTE + SLACK);
    tick(&f);

    let stalled = records_of(&f, WakeKind::TaskStalled);
    assert_eq!(stalled.len(), 1, "the check ran at 5 minutes, not 10");
    assert_eq!(stalled[0].subject, "t-1");
}

#[test]
fn a_changed_setting_applies_at_the_next_check() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    set_settings(
        &f,
        "s-1",
        json!({"supervision": {"base_interval_secs": 300}}),
    )
    .unwrap();

    advance(&f, 5 * MINUTE + SLACK);
    tick(&f);

    assert_eq!(
        records_of(&f, WakeKind::TaskStalled).len(),
        1,
        "no restart, the new base applies"
    );
}

#[test]
fn event_wakes_are_not_delayed_by_the_interval() {
    let f = fixture();
    set_settings(
        &f,
        "s-1",
        json!({"supervision": {"base_interval_secs": 14_400}}),
    )
    .unwrap();
    fail_task(&f, "t-1", "one");
    tick(&f);

    assert_eq!(
        start_batch(&f).started,
        1,
        "60 s debounce, never the interval"
    );
}

#[test]
fn a_finding_never_cancels_or_revises_a_task() {
    let f = fixture();
    run_task(&f, "t-1", "one");
    tick(&f);
    advance(&f, 10 * MINUTE + SLACK);
    tick(&f);
    start_batch(&f);
    assert_eq!(
        f.probe.started(),
        1,
        "only the periodic work check starts a turn"
    );

    let task = f.api.store.get("t-1").unwrap();
    assert_eq!(task.state, orchestrator_core::task::TaskState::Running);
    assert!(f
        .api
        .store
        .action_proposals(&ConversationTarget::Global, 10)
        .unwrap()
        .is_empty());
}

// ---- restart -----------------------------------------------------------------------------

#[test]
fn a_recover_error_is_logged_and_does_not_stop_startup() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    let ids = f.wake.state().debounce.take(GLOBAL_SCOPE);
    assert!(!ids.is_empty());

    let result = f.wake.after_recover(Err("cannot read wake records".into()));

    assert_eq!(result, Ok(()));
    let lines = f.wake.diagnostics.lines();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("wake_recover_failed"))
            .count(),
        1,
        "{lines:?}"
    );
    assert_eq!(
        start_batch(&f).started,
        1,
        "the pending records were re-offered"
    );
}

#[test]
fn restart_requeues_a_claimed_batch_without_a_second_live_turn() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    let first = records_of(&f, WakeKind::TaskFailed)[0].clone();
    assert_eq!(first.state, WakeState::Running);

    let again = restarted(&f);

    let record = records_of(&f, WakeKind::TaskFailed)[0].clone();
    assert_eq!(record.state, WakeState::Pending);
    assert_eq!(record.attempts, 1);
    assert_eq!(record.batch_id, None);
    assert!(running_turn(&f).is_none(), "the old turn is terminal");

    advance(&f, DEBOUNCE_MS);
    assert_eq!(again.tick().unwrap().started, 1);
    let second = records_of(&f, WakeKind::TaskFailed)[0].clone();
    assert_ne!(
        second.turn_id, first.turn_id,
        "a new batch has a new turn id"
    );
    assert!(running_turn(&f).is_some());
    assert_eq!(f.probe.started(), 2);
}

#[test]
fn restart_after_a_completed_turn_settles_it_handled() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    f.probe.finish(0, "Task one failed; retry it?");

    let again = restarted(&f);
    advance(&f, DEBOUNCE_MS);
    again.tick().unwrap();

    let record = &records_of(&f, WakeKind::TaskFailed)[0];
    assert_eq!(record.state, WakeState::Handled);
    assert_eq!(record.outcome, Some(WakeOutcome::Replied));
    assert_eq!(f.probe.started(), 1, "no new turn");
}

#[test]
fn a_batch_that_failed_twice_is_not_tried_a_third_time() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);

    let again = restarted(&f);
    advance(&f, DEBOUNCE_MS);
    again.tick().unwrap();
    assert_eq!(f.probe.started(), 2);

    let third = restarted(&f);
    advance(&f, DEBOUNCE_MS);
    third.tick().unwrap();

    assert_eq!(
        records_of(&f, WakeKind::TaskFailed)[0].state,
        WakeState::Failed
    );
    assert_eq!(f.probe.started(), 2, "never a third try");
}

#[test]
fn a_crash_between_event_and_record_loses_nothing() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    // The events are committed; the wake cursor never moved and nothing was recorded.
    assert_eq!(f.api.store.wake_cursor().unwrap(), 0);
    assert!(global_records(&f).is_empty());

    let again = restarted(&f);
    again.tick().unwrap();
    again.tick().unwrap();

    assert_eq!(
        records_of(&f, WakeKind::TaskFailed).len(),
        1,
        "recorded once"
    );
    assert!(f.api.store.wake_cursor().unwrap() > 0);
}

#[test]
fn a_restart_that_finds_changes_records_one_catch_up_notice() {
    let f = fixture();
    fail_task(&f, "t-1", "one");

    let _again = restarted(&f);

    let starts = records_of(&f, WakeKind::DaemonStart);
    assert_eq!(starts.len(), 1);
    assert_eq!(
        starts[0].notice,
        "The daemon restarted. Checking what changed."
    );
}

// ---- settings ----------------------------------------------------------------------------

#[test]
fn settings_get_carries_the_supervision_and_wake_limit_groups() {
    let f = fixture();
    let body = f.api.dispatch("settings.get", Value::Null).unwrap();

    assert_eq!(
        body["supervision"],
        json!({"base_interval_secs": 600, "min_interval_secs": 60, "attempt_wall_minutes": 120})
    );
    assert_eq!(
        body["wake_limits"],
        json!({"hourly_cap": 20, "daily_cap": 160, "daily_token_budget": null})
    );
    assert!(body.get("mode").is_none(), "the mode is monitoring.get's");
}

#[test]
fn settings_set_refuses_a_minimum_above_the_base_in_one_call_unless_both_move() {
    let f = fixture();
    set_settings(
        &f,
        "s-1",
        json!({"supervision": {"min_interval_secs": 600}}),
    )
    .unwrap();

    let refused = set_settings(
        &f,
        "s-2",
        json!({"supervision": {"base_interval_secs": 300}}),
    );

    assert_eq!(refused, Err(RpcError::InvalidParams));
    assert_eq!(
        f.api
            .store
            .orchestrator_settings()
            .unwrap()
            .supervision
            .base_interval_secs,
        600
    );
    let both = set_settings(
        &f,
        "s-3",
        json!({"supervision": {"base_interval_secs": 300, "min_interval_secs": 120}}),
    )
    .unwrap();
    assert_eq!(both["supervision"]["base_interval_secs"], 300);
    assert_eq!(both["supervision"]["min_interval_secs"], 120);
}

#[test]
fn settings_set_null_clears_a_wake_cap_and_absent_leaves_it() {
    let f = fixture();
    let cleared = set_settings(&f, "s-1", json!({"wake_limits": {"hourly_cap": null}})).unwrap();
    assert_eq!(cleared["wake_limits"]["hourly_cap"], Value::Null);
    assert_eq!(
        f.api
            .store
            .orchestrator_settings()
            .unwrap()
            .wake_limits
            .hourly,
        None
    );

    let changed = set_settings(&f, "s-2", json!({"wake_limits": {"daily_cap": 50}})).unwrap();

    assert_eq!(changed["wake_limits"]["daily_cap"], 50);
    assert_eq!(
        changed["wake_limits"]["hourly_cap"],
        Value::Null,
        "absent leaves it as it was"
    );
}

#[test]
fn settings_set_with_only_a_supervision_group_leaves_the_load_limits_and_the_gate_alone() {
    let f = fixture();
    let load = f.api.config.load.clone();
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    let before = load.read(Instant::now());

    let body = set_settings(
        &f,
        "s-1",
        json!({"supervision": {"attempt_wall_minutes": 30}}),
    )
    .unwrap();

    assert_eq!(body["supervision"]["attempt_wall_minutes"], 30);
    let after = load.read(Instant::now());
    assert_eq!(after.limits, before.limits);
    assert_eq!(body["gate"]["state"], "closed");
}

#[test]
fn settings_set_refuses_an_empty_patch_an_empty_group_and_the_mode() {
    let f = fixture();
    for patch in [
        json!({}),
        json!({"supervision": {}}),
        json!({"wake_limits": {}}),
        json!({"mode": "off"}),
        json!({"wake_limits": {"hourly_cap": 0}}),
        json!({"wake_limits": {"daily_token_budget": 0}}),
        json!({"supervision": {"attempt_wall_minutes": 4}}),
    ]
    .into_iter()
    .enumerate()
    {
        let (index, patch) = patch;
        assert_eq!(
            set_settings(&f, &format!("s-{index}"), patch.clone()),
            Err(RpcError::InvalidParams),
            "{patch}"
        );
    }
    assert_eq!(
        f.api.store.orchestrator_settings().unwrap().mode,
        MonitoringMode::On
    );
}

#[test]
fn the_limits_survive_a_restart_for_the_new_groups_too() {
    let directory = tempfile::tempdir().unwrap();
    {
        let api = DaemonApi::new(directory.path()).unwrap();
        api.dispatch(
            "settings.set",
            json!({
                "idempotency_key": "s-1",
                "supervision": {"base_interval_secs": 900, "min_interval_secs": 30, "attempt_wall_minutes": 60},
                "wake_limits": {"hourly_cap": 3, "daily_cap": null, "daily_token_budget": 1000},
            }),
        )
        .unwrap();
        api.dispatch(
            "monitoring.set",
            json!({"idempotency_key": "m-1", "mode": "quiet"}),
        )
        .unwrap();
    }

    let api = DaemonApi::new(directory.path()).unwrap();

    let body = api.dispatch("settings.get", Value::Null).unwrap();
    assert_eq!(
        body["supervision"],
        json!({"base_interval_secs": 900, "min_interval_secs": 30, "attempt_wall_minutes": 60})
    );
    assert_eq!(
        body["wake_limits"],
        json!({"hourly_cap": 3, "daily_cap": null, "daily_token_budget": 1000})
    );
    assert_eq!(
        api.dispatch("monitoring.get", json!({})).unwrap()["mode"],
        "quiet"
    );
}

// ---- usage -------------------------------------------------------------------------------

#[test]
fn chat_turns_write_usage_receipts_for_user_and_wake() {
    let f = fixture();
    f.probe.replying(Some("ok"));
    f.probe.reporting(Some(7));
    let sent = user_send(&f, "u-1").unwrap();
    let user_turn = sent["turn"]["id"].as_str().unwrap().to_owned();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    let wake_turn = records_of(&f, WakeKind::TaskFailed)[0]
        .turn_id
        .clone()
        .unwrap();

    let receipt = |turn: &str, trigger| ConversationUsageReceipt {
        conversation_id: GLOBAL_SCOPE.into(),
        turn_id: turn.into(),
        trigger,
        receipt_key: "turn.completed:1".into(),
        reported_at_ms: 0,
        input_tokens: Some(7),
        cache_read_tokens: None,
        cache_write_tokens: None,
        output_tokens: None,
        reasoning_tokens: None,
        cost: None,
        model: None,
    };
    // An older observation under the same key changes nothing when the receipt exists.
    for (turn, trigger) in [
        (&user_turn, TurnTrigger::User),
        (&wake_turn, TurnTrigger::Wake),
    ] {
        let outcome = f
            .api
            .store
            .record_conversation_usage(&receipt(turn, trigger))
            .unwrap();
        assert!(matches!(outcome, ReceiptOutcome::Unchanged), "{turn}");
    }
    // Only the wake turn's tokens count toward the wake budget.
    assert_eq!(f.api.store.wake_tokens_since(GLOBAL_SCOPE, 0).unwrap(), 7);
}

// ---- away mode and the return brief (phase 5b) -------------------------------------------

fn return_brief(f: &Fixture) -> Value {
    f.api.dispatch("monitoring.brief", json!({})).unwrap()["brief"].clone()
}

fn brief_ack(f: &Fixture, key: &str) -> Value {
    f.api
        .dispatch("monitoring.brief_ack", json!({"idempotency_key": key}))
        .unwrap()
}

#[test]
fn the_return_brief_is_null_then_open_while_away_then_closed_until_acknowledged() {
    let f = fixture();
    assert_eq!(return_brief(&f), Value::Null);
    assert_eq!(monitoring(&f)["brief_pending"], false);
    assert_eq!(monitoring(&f)["away_since_ms"], Value::Null);

    set_mode(&f, "away").unwrap();
    fail_task(&f, "t-1", "Fix /Users/me/secret/repo/main.rs now");
    advance(&f, MINUTE);
    let open = return_brief(&f);
    assert_eq!(open["open"], true);
    assert_eq!(open["counts"]["tasks_failed"], 1);
    assert_eq!(open["attention"][0]["id"], "t-1");
    assert_eq!(open["attention"][0]["kind"], "task_failed");
    let title = open["attention"][0]["title"].as_str().unwrap();
    assert!(
        title.contains("[path]") && !title.contains("/Users"),
        "{title}"
    );
    let project = open["attention"][0]["project"].as_str().unwrap();
    assert!(
        project.starts_with("p-") && project.len() == 10,
        "{project}"
    );

    set_mode(&f, "on").unwrap();
    let closed = return_brief(&f);
    assert_eq!(closed["open"], false);
    assert_eq!(closed["counts"]["tasks_failed"], 1);
    assert_eq!(monitoring(&f)["brief_pending"], true);
    assert_eq!(monitoring(&f)["away_since_ms"], Value::Null);

    // The brief is read-only: reading it again changes nothing.
    assert_eq!(return_brief(&f), closed);
    assert_eq!(brief_ack(&f, "ack-1")["authority"], "none");
    assert_eq!(return_brief(&f), Value::Null);
    assert_eq!(monitoring(&f)["brief_pending"], false);
    // A second acknowledgement, under a new key, is a no-op.
    brief_ack(&f, "ack-2");
    assert_eq!(return_brief(&f), Value::Null);
}

#[test]
fn a_brief_acknowledgement_leaves_the_live_away_window_in_place() {
    let f = fixture();
    set_mode(&f, "away").unwrap();
    brief_ack(&f, "ack-live");
    assert_eq!(return_brief(&f)["open"], true);
    assert_eq!(monitoring(&f)["mode"], "away");
}

#[test]
fn the_brief_methods_take_no_unknown_parameters() {
    let f = fixture();
    assert_eq!(
        f.api.dispatch("monitoring.brief", json!({"from": 0})),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        f.api.dispatch(
            "monitoring.brief_ack",
            json!({"idempotency_key": "k", "to": 5})
        ),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn an_away_wake_turn_still_stops_at_the_hourly_cap() {
    let f = fixture();
    set_mode(&f, "away").unwrap();
    f.probe.replying(Some("ok"));
    let cap = DEFAULT_WAKE_HOURLY_CAP as usize;
    for index in 0..cap {
        fail_task(&f, &format!("t-{index}"), "Fix it");
        tick(&f);
        assert_eq!(start_batch(&f).started, 1, "turn {index}");
    }
    let over = format!("t-{cap}");
    fail_task(&f, &over, "Fix it");
    tick(&f);
    let report = start_batch(&f);

    assert_eq!(report.started, 0);
    assert_eq!(f.probe.started(), cap, "away spends no turn past the cap");
    let seventh = records_of(&f, WakeKind::TaskFailed)
        .into_iter()
        .find(|record| record.subject == over)
        .unwrap();
    assert_eq!(seventh.skip_reason, Some(SkipReason::LimitHourly));
    assert_eq!(monitoring(&f)["paused"][0]["reason"], "hourly");
}

#[test]
fn away_runs_wake_turns_and_changing_to_it_cancels_nothing() {
    let f = fixture();
    fail_task(&f, "t-1", "one");
    tick(&f);
    start_batch(&f);
    assert!(running_turn(&f).is_some());

    set_mode(&f, "away").unwrap();
    tick(&f);

    assert!(running_turn(&f).is_some(), "away is not off");
    assert!(records_of(&f, WakeKind::TaskFailed)
        .iter()
        .all(|record| record.skip_reason.is_none()));
}

#[test]
fn the_return_brief_never_enters_a_wake_snapshot_or_prompt() {
    let f = fixture();
    set_mode(&f, "away").unwrap();
    fail_task(&f, "t-1", "one");
    advance(&f, MINUTE);
    tick(&f);
    start_batch(&f);

    let seen = f.probe.last();
    assert!(
        seen.context.contains("Monitoring: away"),
        "{}",
        seen.context
    );
    for needle in [
        "tasks_failed",
        "approved_by_grant",
        "proposals_waiting",
        "While you were away",
        "away_since",
        "brief_pending",
    ] {
        assert!(!seen.context.contains(needle), "{needle} in context");
        assert!(!seen.prompt.contains(needle), "{needle} in prompt");
    }
}

/// Records one failing task, starts its wake turn and fails that turn.
fn failing_wake_turn(f: &Fixture, id: &str) {
    fail_task(f, id, "Fix it");
    tick(f);
    assert_eq!(start_batch(f).started, 1, "{id} starts a wake turn");
    fail_newest_turn(f);
}

/// Fails the newest wake turn the probe holds, the way an agent that cannot answer does.
fn fail_newest_turn(f: &Fixture) {
    let sink = f
        .probe
        .held
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("a held turn");
    sink.fail(RuntimeFailure::Failed);
    tick(f);
}

/// Records one failing task and waits out its debounce. Returns how many turns started.
fn event_then_start(f: &Fixture, id: &str) -> usize {
    fail_task(f, id, "Fix it");
    tick(f);
    start_batch(f).started
}

#[test]
fn three_failed_wake_turns_pause_automatic_updates_with_one_notice() {
    let f = fixture();
    for id in ["t-0", "t-1", "t-2"] {
        failing_wake_turn(&f, id);
    }
    assert_eq!(f.probe.started(), 3);

    assert_eq!(event_then_start(&f, "t-3"), 0, "the fourth turn is held");
    assert_eq!(f.probe.started(), 3);
    assert_eq!(ledger_rows(&f), 3, "a held turn spends no cap");
    let held = records_of(&f, WakeKind::TaskFailed)
        .into_iter()
        .find(|record| record.subject == "t-3")
        .unwrap();
    assert_eq!(held.state, WakeState::Skipped);
    assert_eq!(held.skip_reason, Some(SkipReason::AgentFailing));

    let notices = records_of(&f, WakeKind::AgentFailing);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].state, WakeState::Noticed);
    assert_eq!(
        notices[0].notice,
        "Automatic updates are paused: probe failed 3 times in a row (runtime_failed). \
         Fix the agent or change the default agent in Settings."
    );

    // A later event inside the same pause is held too, and adds no notice.
    assert_eq!(event_then_start(&f, "t-4"), 0);
    assert_eq!(f.probe.started(), 3);
    assert_eq!(records_of(&f, WakeKind::AgentFailing).len(), 1);
    let logged = f
        .wake
        .diagnostics
        .lines()
        .iter()
        .filter(|line| line.contains("wake_paused_agent_failing"))
        .count();
    assert_eq!(logged, 1, "one log line per streak");
}

#[test]
fn a_completed_wake_turn_resets_the_agent_failure_streak() {
    let f = fixture();
    failing_wake_turn(&f, "t-0");
    failing_wake_turn(&f, "t-1");
    f.probe.replying(Some("Done."));
    assert_eq!(event_then_start(&f, "t-2"), 1);
    f.probe.replying(None);
    tick(&f);
    failing_wake_turn(&f, "t-3");
    failing_wake_turn(&f, "t-4");

    assert_eq!(
        event_then_start(&f, "t-5"),
        1,
        "two failures after the reset do not pause"
    );
    assert_eq!(f.probe.started(), 6);
    assert!(records_of(&f, WakeKind::AgentFailing).is_empty());
}

#[test]
fn a_settings_change_lifts_the_pause_for_one_wake_turn() {
    let f = fixture();
    for id in ["t-0", "t-1", "t-2"] {
        failing_wake_turn(&f, id);
    }
    assert_eq!(event_then_start(&f, "t-3"), 0);

    f.wake.settings_changed();
    assert_eq!(event_then_start(&f, "t-4"), 1, "the lifted turn runs");
    assert_eq!(f.probe.started(), 4);
    fail_newest_turn(&f);

    assert_eq!(event_then_start(&f, "t-5"), 0, "its failure pauses again");
    assert_eq!(f.probe.started(), 4);
    assert_eq!(
        records_of(&f, WakeKind::AgentFailing).len(),
        2,
        "one notice for each streak"
    );
}

#[test]
fn the_pause_lifts_thirty_minutes_after_the_newest_failure() {
    let f = fixture();
    for id in ["t-0", "t-1", "t-2"] {
        failing_wake_turn(&f, id);
    }
    assert_eq!(event_then_start(&f, "t-3"), 0);

    advance(&f, 27 * MINUTE);
    assert_eq!(
        event_then_start(&f, "t-4"),
        0,
        "still paused 29 minutes after the failure"
    );
    advance(&f, MINUTE);
    assert_eq!(
        event_then_start(&f, "t-5"),
        1,
        "resumes once the cooldown has passed"
    );
    assert_eq!(f.probe.started(), 4);
}

// ---- project coordinators (DPES-1108 phase 7) ---------------------------------------------

const COORDINATOR_SCOPE: &str = "conversation:project:p";

fn coordinator_records(f: &Fixture) -> Vec<WakeRecord> {
    f.api
        .store
        .wake_records_page(COORDINATOR_SCOPE, 50)
        .unwrap()
        .0
}

/// Adds project `p`'s coordinator and makes it the owner of `task_id`.
fn own(f: &Fixture, task_id: &str) {
    f.api.store.restore_coordinator("p").unwrap();
    f.api
        .store
        .set_task_coordinator(task_id, "p", None)
        .unwrap();
}

#[test]
fn a_coordinator_task_wakes_its_coordinator_and_reaches_global_only_as_a_report() {
    let f = fixture();
    f.probe.replying(Some("The export task failed."));
    queue_task(&f, "t-c", "Fix the export");
    own(&f, "t-c");
    // The rest of a run, from the queued task the owner was written on.
    let store = &f.api.store;
    store
        .add_attempt(
            "t-c",
            &Attempt {
                id: "t-c-a1".into(),
                task_id: "t-c".into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    store
        .update_attempt_state("t-c", "t-c-a1", AttemptState::Running)
        .unwrap();
    store
        .transition("t-c", TaskEvent::ExecutionStarted)
        .unwrap();
    store
        .record_attempt_failure("t-c", "t-c-a1", "boom")
        .unwrap();
    store.transition("t-c", TaskEvent::ExecutionFailed).unwrap();
    tick(&f);

    let own_records = coordinator_records(&f);
    assert_eq!(own_records.len(), 1, "{own_records:?}");
    assert_eq!(own_records[0].kind, WakeKind::TaskFailed);
    assert!(own_records[0].needs_model);
    // Global gets no copy of the task's record, only the parent channel's report.
    assert!(records_of(&f, WakeKind::TaskFailed).is_empty());
    let reports = records_of(&f, WakeKind::CoordinatorReport);
    assert_eq!(reports.len(), 1, "{:?}", global_records(&f));
    assert!(reports[0].needs_model);
    assert!(
        reports[0].notice.contains("task t-c failed"),
        "{}",
        reports[0].notice
    );
    let stored = store.coordinator_reports("p", 10).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].verb, "task_failed");
    assert_eq!(stored[0].author, "daemon");

    // A replay of the same events writes nothing twice.
    tick(&f);
    assert_eq!(store.coordinator_reports("p", 10).unwrap().len(), 1);
    assert_eq!(records_of(&f, WakeKind::CoordinatorReport).len(), 1);

    // Both conversations get their own turn: the coordinator with its charter, Global with the
    // report procedure and never the coordinator's conversation.
    assert_eq!(start_batch(&f).started, 2);
    let seen = f.probe.seen.lock().unwrap().clone();
    let coordinator = seen
        .iter()
        .find(|turn| {
            turn.prompt
                .contains(orchestrator_core::charter::CHARTER_BEGIN)
        })
        .expect("a coordinator turn");
    assert!(coordinator.actions && coordinator.wake);
    assert!(coordinator
        .prompt
        .contains("Procedure, a failed or stalled task"));
    let global = seen
        .iter()
        .find(|turn| {
            !turn
                .prompt
                .contains(orchestrator_core::charter::CHARTER_BEGIN)
        })
        .expect("a global turn");
    assert!(global.prompt.contains("Procedure, a coordinator report"));
    assert!(
        global.context.contains("Coordinator reports"),
        "{}",
        global.context
    );
}

#[test]
fn an_idle_coordinator_gets_no_work_check_and_a_busy_one_gets_its_own() {
    let f = fixture();
    f.probe.replying(Some(QUIET_TOKEN));
    // Owned but queued work runs nothing: no scope holds work.
    queue_task(&f, "t-q", "Waiting for capacity");
    own(&f, "t-q");
    tick(&f);
    advance(&f, 11 * MINUTE);
    tick(&f);
    assert!(coordinator_records(&f).is_empty());
    assert!(records_of(&f, WakeKind::WorkCheck).is_empty());

    // A running owned task is the coordinator's work, not Global's.
    run_task(&f, "t-r", "Working");
    f.api.store.set_task_coordinator("t-r", "p", None).unwrap();
    tick(&f);
    advance(&f, 600 * 1_000 + SLACK);
    f.pulse.touch_at("t-r-s1", f.clock.now_ms());
    tick(&f);
    let checks: Vec<WakeRecord> = coordinator_records(&f)
        .into_iter()
        .filter(|record| record.kind == WakeKind::WorkCheck)
        .collect();
    assert_eq!(checks.len(), 1, "{:?}", coordinator_records(&f));
    assert!(records_of(&f, WakeKind::WorkCheck).is_empty());
}
