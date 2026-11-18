use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use orchestrator_store::Store;

use super::super::conversation_runtime::{
    Choice, ConversationRuntime, ConversationService, NoWorkstreams, ReplySink, RuntimeFailure,
    RuntimeIdentity, TurnControl, TurnRequest, WorkstreamDirectory,
};
use super::*;

/// History pairs and workstream context a turn was started with.
type Seen = (Vec<(String, String)>, Option<String>);

enum Mode {
    /// Answer inside `start_turn`, then complete.
    Reply(Vec<String>),
    /// Keep the sink so the test drives output and completion.
    Hold,
    Fail(RuntimeFailure),
}

struct TestRuntime {
    identity: RuntimeIdentity,
    mode: Mode,
    held: Mutex<Option<ReplySink>>,
    cancelled: Arc<AtomicBool>,
    started: AtomicUsize,
    seen: Mutex<Vec<String>>,
    /// History pairs and workstream context each turn was started with.
    requests: Mutex<Vec<Seen>>,
}

impl TestRuntime {
    fn new(mode: Mode) -> Arc<Self> {
        Self::versioned("1.0.0", mode)
    }

    fn versioned(version: &str, mode: Mode) -> Arc<Self> {
        Arc::new(Self {
            identity: RuntimeIdentity {
                name: "test-runtime".into(),
                version: version.into(),
            },
            mode,
            held: Mutex::new(None),
            cancelled: Arc::new(AtomicBool::new(false)),
            started: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn sink(&self) -> ReplySink {
        self.held.lock().unwrap().clone().expect("held sink")
    }
}

struct Control(Arc<AtomicBool>);

impl TurnControl for Control {
    fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl ConversationRuntime for TestRuntime {
    fn identity(&self, _choice: Option<&Choice>) -> RuntimeIdentity {
        self.identity.clone()
    }

    /// Only the version the tests pin (`1.0.0`) is verified; any other is not.
    fn verified(&self, identity: &RuntimeIdentity) -> bool {
        identity.version == "1.0.0"
    }

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(request.text.clone());
        self.requests.lock().unwrap().push((
            request
                .history
                .iter()
                .map(|turn| (turn.user.clone(), turn.reply.clone()))
                .collect(),
            request.context.clone(),
        ));
        match &self.mode {
            Mode::Fail(failure) => return Err(failure.clone()),
            Mode::Hold => *self.held.lock().unwrap() = Some(sink),
            Mode::Reply(parts) => {
                for part in parts {
                    if sink.delta(part).is_err() {
                        break;
                    }
                }
                sink.complete();
            }
        }
        Ok(Box::new(Control(self.cancelled.clone())))
    }
}

struct Directory(&'static [&'static str]);

impl WorkstreamDirectory for Directory {
    fn exists(&self, workstream_id: &str) -> Result<bool, RpcError> {
        Ok(self.0.contains(&workstream_id))
    }
}

fn api_with(
    runtime: Arc<TestRuntime>,
    pinned: bool,
    workstreams: Arc<dyn WorkstreamDirectory>,
) -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let mut api = DaemonApi::new(directory.path()).unwrap();
    api.conversations = ConversationService::new(
        api.store.clone(),
        runtime,
        workstreams,
        pinned,
        api.config.load.clone(),
    );
    (directory, api)
}

fn pinned(runtime: Arc<TestRuntime>) -> (tempfile::TempDir, DaemonApi) {
    api_with(runtime, true, Arc::new(NoWorkstreams))
}

fn global() -> Value {
    json!({"kind": "global"})
}

fn send(api: &DaemonApi, target: Value, text: &str, key: &str) -> Result<Value, RpcError> {
    api.dispatch(
        "conversations.send",
        json!({"target": target, "text": text, "idempotency_key": key}),
    )
}

fn get(api: &DaemonApi, target: Value, after: i64) -> Value {
    api.dispatch(
        "conversations.get",
        json!({"target": target, "after_sequence": after}),
    )
    .unwrap()
}

fn kinds(page: &Value) -> Vec<String> {
    page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_closed_load_gate_does_not_block_a_chat_turn() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["Hello".into()]));
    let (_directory, api) = pinned(runtime.clone());
    crate::api::load::tests::close_gate(&api.config.load);

    let sent = send(&api, global(), "hi", "k1").unwrap();

    assert_eq!(sent["turn"]["state"], "completed");
    assert_eq!(runtime.started.load(Ordering::SeqCst), 1);
    let requests = runtime.requests.lock().unwrap();
    let context = requests[0].1.as_deref().unwrap();
    assert!(context.contains("Machine load: gate closed."), "{context}");
}

#[test]
fn a_reply_streams_in_order_and_is_correlated_with_its_message() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["Hel".into(), "lo".into()]));
    let (_directory, api) = pinned(runtime.clone());
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(sent["authority"], "none");
    assert_eq!(sent["turn"]["state"], "completed");
    assert_eq!(sent["turn"]["runtime"]["name"], "test-runtime");
    assert_eq!(sent["turn"]["runtime"]["version"], "1.0.0");
    assert_eq!(sent["entry"]["author"], "user");

    let page = get(&api, global(), 0);
    assert_eq!(
        kinds(&page),
        [
            "user_message",
            "reply_chunk",
            "reply_chunk",
            "turn_completed"
        ]
    );
    let turn_id = sent["turn"]["id"].as_str().unwrap();
    assert!(page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["turn_id"] == turn_id));
    let sequences: Vec<_> = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["sequence"].as_i64().unwrap())
        .collect();
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(page["authority"], "none");
    assert_eq!(page["turn"]["state"], "completed");
    assert_eq!(page["latest_sequence"], *sequences.last().unwrap());
}

#[test]
fn a_large_delta_is_split_without_breaking_characters() {
    let text = "é".repeat(20_000);
    let runtime = TestRuntime::new(Mode::Reply(vec![text.clone()]));
    let (_directory, api) = pinned(runtime);
    send(&api, global(), "hi", "k1").unwrap();
    let page = get(&api, global(), 0);
    let joined: String = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["kind"] == "reply_chunk")
        .map(|entry| {
            let body = entry["body"].as_str().unwrap();
            assert!(body.len() <= orchestrator_store::MAX_REPLY_CHUNK_BYTES);
            body.to_owned()
        })
        .collect();
    assert_eq!(joined, text);
}

#[test]
fn a_reply_over_the_turn_bound_fails_the_turn_and_stops_the_runtime() {
    let part = "x".repeat(orchestrator_store::MAX_REPLY_CHUNK_BYTES);
    let parts = vec![part; orchestrator_store::MAX_REPLY_BYTES_PER_TURN / part_len() + 2];
    let runtime = TestRuntime::new(Mode::Reply(parts));
    let (_directory, api) = pinned(runtime);
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(sent["turn"]["state"], "failed");
    assert_eq!(sent["turn"]["error_code"], "reply_too_large");
}

fn part_len() -> usize {
    orchestrator_store::MAX_REPLY_CHUNK_BYTES
}

#[test]
fn a_running_turn_blocks_the_next_message_until_it_ends() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (_directory, api) = pinned(runtime.clone());
    let sent = send(&api, global(), "first", "k1").unwrap();
    assert_eq!(sent["turn"]["state"], "running");
    assert_eq!(
        send(&api, global(), "second", "k2"),
        Err(RpcError::ConversationBusy)
    );
    assert_eq!(runtime.started.load(Ordering::SeqCst), 1);

    let sink = runtime.sink();
    sink.delta("partial").unwrap();
    let cursor = get(&api, global(), 0)["next_after_sequence"]
        .as_i64()
        .unwrap();
    sink.delta(" more").unwrap();
    sink.complete();
    let replay = get(&api, global(), cursor);
    assert_eq!(kinds(&replay), ["reply_chunk", "turn_completed"]);
    // A rejected attempt stays rejected for its key; the retry needs a new one.
    assert_eq!(
        send(&api, global(), "second", "k2"),
        Err(RpcError::ConversationBusy)
    );
    send(&api, global(), "second", "k3").unwrap();
}

#[test]
fn cancel_closes_the_turn_stops_the_runtime_and_discards_late_output() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (_directory, api) = pinned(runtime.clone());
    let sent = send(&api, global(), "first", "k1").unwrap();
    let turn_id = sent["turn"]["id"].as_str().unwrap().to_owned();
    let sink = runtime.sink();
    sink.delta("before").unwrap();

    let cancel = |key: &str| {
        api.dispatch(
            "conversations.cancel",
            json!({"target": global(), "turn_id": turn_id, "idempotency_key": key}),
        )
    };
    let cancelled = cancel("c1").unwrap();
    assert_eq!(cancelled["turn"]["state"], "cancelled");
    assert_eq!(cancelled["cancel_applied"], true);
    assert!(runtime.cancelled.load(Ordering::SeqCst));
    assert!(sink.delta("late").is_err());
    sink.complete();

    let second = cancel("c2").unwrap();
    assert_eq!(second["cancel_applied"], false);
    assert_eq!(second["turn"]["state"], "cancelled");
    assert_eq!(cancel("c1").unwrap(), cancelled);

    let page = get(&api, global(), 0);
    assert_eq!(
        kinds(&page),
        ["user_message", "reply_chunk", "turn_cancelled"]
    );
    send(&api, global(), "next", "k2").unwrap();
}

#[test]
fn cancel_is_scoped_to_its_conversation_and_turn() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (_directory, api) = api_with(runtime, true, Arc::new(Directory(&["w1"])));
    let sent = send(&api, global(), "first", "k1").unwrap();
    let turn_id = sent["turn"]["id"].as_str().unwrap();
    for (target, id, key) in [
        (
            json!({"kind": "workstream", "workstream_id": "w1"}),
            turn_id,
            "c1",
        ),
        (global(), "missing", "c2"),
    ] {
        assert_eq!(
            api.dispatch(
                "conversations.cancel",
                json!({"target": target, "turn_id": id, "idempotency_key": key}),
            ),
            Err(RpcError::NotFound)
        );
    }
    assert_eq!(get(&api, global(), 0)["turn"]["state"], "running");
}

#[test]
fn a_retried_send_returns_the_first_result_and_runs_once() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = pinned(runtime.clone());
    let first = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(send(&api, global(), "hi", "k1").unwrap(), first);
    assert_eq!(
        send(&api, global(), "different", "k1"),
        Err(RpcError::IdempotencyConflict)
    );
    assert_eq!(runtime.started.load(Ordering::SeqCst), 1);
    let page = get(&api, global(), 0);
    assert_eq!(
        kinds(&page).iter().filter(|k| *k == "user_message").count(),
        1
    );
}

#[test]
fn a_replayed_send_shows_the_turn_as_it_is_now_not_as_it_was_recorded() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (_directory, api) = pinned(runtime.clone());
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(sent["turn"]["state"], "running");
    assert_eq!(send(&api, global(), "hi", "k1").unwrap(), sent);

    runtime.sink().delta("done").unwrap();
    runtime.sink().complete();
    let replayed = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(replayed["turn"]["state"], "completed");
    assert_eq!(replayed["turn"]["id"], sent["turn"]["id"]);
    assert_eq!(replayed["turn"]["reply_bytes"], 4);
    assert_eq!(replayed["entry"], sent["entry"]);
    assert_eq!(runtime.started.load(Ordering::SeqCst), 1);
}

#[test]
fn restart_keeps_the_transcript_fails_open_turns_and_resumes_replay() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (directory, api) = pinned(runtime.clone());
    let sent = send(&api, global(), "first", "k1").unwrap();
    runtime.sink().delta("partial").unwrap();
    let before = get(&api, global(), 0);
    let cursor = before["next_after_sequence"].as_i64().unwrap();
    let epoch = before["epoch"].clone();
    drop(api);

    let reopened = without_agents(directory.path());
    let replay = reopened
        .dispatch(
            "conversations.get",
            json!({"target": global(), "after_sequence": cursor, "epoch": epoch}),
        )
        .unwrap();
    assert_eq!(kinds(&replay), ["turn_failed"]);
    assert_eq!(replay["entries"][0]["error_code"], "interrupted_by_restart");
    assert_eq!(replay["turn"]["state"], "failed");
    assert_eq!(replay["epoch"], before["epoch"]);
    assert_eq!(
        kinds(&get(&reopened, global(), 0)),
        ["user_message", "reply_chunk", "turn_failed"]
    );
    // The stored result for the original request survives and still replays,
    // with the turn shown as it is now rather than as it was recorded.
    assert_eq!(sent["turn"]["state"], "running");
    let replayed = send(&reopened, global(), "first", "k1").unwrap();
    assert_eq!(replayed["turn"]["state"], "failed");
    assert_eq!(replayed["turn"]["error_code"], "interrupted_by_restart");
    assert_eq!(replayed["turn"]["id"], sent["turn"]["id"]);
    assert_eq!(replayed["entry"], sent["entry"]);
    assert_eq!(replayed["epoch"], sent["epoch"]);
    // A fresh message is accepted; the default runtime is unavailable.
    let next = send(&reopened, global(), "again", "k2").unwrap();
    assert_eq!(next["turn"]["error_code"], "runtime_unavailable");
}

#[test]
fn runtime_failures_fail_the_turn_with_a_stable_code_and_keep_the_message() {
    for (failure, code) in [
        (RuntimeFailure::CredentialsMissing, "credentials_missing"),
        (RuntimeFailure::PermissionDenied, "permission_denied"),
        (
            RuntimeFailure::MissingFlag("--ephemeral".into()),
            "version_mismatch",
        ),
        (RuntimeFailure::Unpinned, "runtime_unpinned"),
        (RuntimeFailure::RuntimeChanged, "runtime_changed"),
        (RuntimeFailure::Failed, "runtime_failed"),
        (RuntimeFailure::Unavailable, "runtime_unavailable"),
    ] {
        let (_directory, api) = pinned(TestRuntime::new(Mode::Fail(failure)));
        let sent = send(&api, global(), "hi", "k1").unwrap();
        assert_eq!(sent["turn"]["state"], "failed");
        assert_eq!(sent["turn"]["error_code"], code);
        let page = get(&api, global(), 0);
        assert_eq!(kinds(&page), ["user_message", "turn_failed"]);
        assert_eq!(page["entries"][1]["error_code"], code);
        send(&api, global(), "retry", "k2").unwrap();
    }
}

#[test]
fn every_refused_choice_has_its_own_stable_code() {
    use orchestrator_agents::SelectionReason;
    for (reason, code) in [
        (
            SelectionReason::AgentUnavailable,
            "selection_agent_unavailable",
        ),
        (
            SelectionReason::AuthenticationFailed,
            "selection_authentication_failed",
        ),
        (
            SelectionReason::AgentStateUnknown,
            "selection_agent_state_unknown",
        ),
        (
            SelectionReason::AgentUnsupported,
            "selection_agent_unsupported",
        ),
        (
            SelectionReason::ModelsUnsupported,
            "selection_models_unsupported",
        ),
        (SelectionReason::ModelsUnknown, "selection_models_unknown"),
        (SelectionReason::UnknownModel, "selection_unknown_model"),
        (
            SelectionReason::EffortRequiresModel,
            "selection_effort_requires_model",
        ),
        (
            SelectionReason::EffortUnsupported,
            "selection_effort_unsupported",
        ),
        (SelectionReason::EffortUnknown, "selection_effort_unknown"),
    ] {
        assert_eq!(RuntimeFailure::Refused(reason).code(), code);
        assert!(code.len() <= 64, "{code} is longer than the store allows");
    }
}

#[test]
fn the_daemon_advertises_the_selection_capability_the_desktop_waits_for() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    assert!(api
        .capabilities()
        .contains(&orchestrator_ipc::Capability::ConversationSelection));
}

#[test]
fn the_daemon_advertises_the_permission_levels_capability() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    assert!(api
        .capabilities()
        .contains(&orchestrator_ipc::Capability::PermissionLevels));
}

#[test]
fn an_unknown_permission_is_a_bad_request_and_records_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    for permission in ["sandboxed", "Auto", "", "read_only"] {
        let result = api.dispatch(
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "permission": permission}),
        );
        assert!(matches!(result, Err(RpcError::InvalidParams)), "{result:?}");
    }
    assert_eq!(kinds(&get(&api, global(), 0)), Vec::<String>::new());
}

#[test]
fn a_send_without_a_choice_serializes_as_it_did_before_selection_existed() {
    let request = ConversationSendParams {
        target: TargetParam::Global {},
        text: "hi".to_owned(),
        idempotency_key: "k".to_owned(),
        selection: None,
        permission: None,
    };
    // The idempotency fingerprint hashes this encoding, so an absent choice or level must add no key.
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({"target": {"kind": "global"}, "text": "hi", "idempotency_key": "k"})
    );
}

#[test]
fn a_chosen_permission_is_part_of_the_encoding_the_idempotency_fingerprint_hashes() {
    let request = ConversationSendParams {
        target: TargetParam::Global {},
        text: "hi".to_owned(),
        idempotency_key: "k".to_owned(),
        selection: None,
        permission: Some(PermissionLevel::Auto),
    };
    // The same key with another level is a different request, so it conflicts.
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({"target": {"kind": "global"}, "text": "hi", "idempotency_key": "k", "permission": "auto"})
    );
}

#[test]
fn a_malformed_choice_is_a_bad_request_and_records_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    for selection in [
        json!({"agent": ""}),
        json!({"agent": "codex", "model": ""}),
        json!({"agent": "codex", "effort": "High"}),
        json!({"agent": "codex", "unexpected": true}),
    ] {
        let result = api.dispatch(
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "selection": selection}),
        );
        assert!(matches!(result, Err(RpcError::InvalidParams)), "{result:?}");
    }
    assert_eq!(kinds(&get(&api, global(), 0)), Vec::<String>::new());
}

#[test]
fn a_chosen_pi_is_refused_as_a_failed_turn_without_launching_anything() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    let sent = api
        .dispatch(
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "selection": {"agent": "pi"}}),
        )
        .unwrap();
    assert_eq!(sent["turn"]["state"], "failed");
    assert_eq!(sent["turn"]["error_code"], "selection_agent_unsupported");
    // The message stays in the transcript, as for every failed turn.
    assert_eq!(
        kinds(&get(&api, global(), 0)),
        ["user_message", "turn_failed"]
    );
}

#[test]
fn a_chosen_copilot_without_its_cli_fails_the_turn_as_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    let sent = api
        .dispatch(
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "selection": {"agent": "github-copilot"}}),
        )
        .unwrap();
    assert_eq!(sent["turn"]["state"], "failed");
    assert_eq!(sent["turn"]["error_code"], "runtime_unavailable");
    assert_eq!(
        kinds(&get(&api, global(), 0)),
        ["user_message", "turn_failed"]
    );
}

#[test]
fn a_chosen_agent_that_cannot_launch_fails_the_turn_as_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    let sent = api
        .dispatch(
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "selection": {"agent": "codex", "model": "gpt-x"}}),
        )
        .unwrap();
    assert_eq!(sent["turn"]["error_code"], "runtime_unavailable");
}

/// The shipped runtime on a machine with no agent installed, wherever the
/// test runs.
fn without_agents(directory: &std::path::Path) -> DaemonApi {
    let missing = directory.join("no-such-agent");
    let mut config = crate::executor::ExecutorConfig::for_data_dir(directory);
    config.codex_program = Some(missing.clone());
    // Never resolve `copilot` on PATH: a launchable Copilot would start a paid run.
    config.copilot_program = Some(missing.clone());
    config.claude_program = Some(missing);
    DaemonApi::with_executor(directory, config).unwrap()
}

#[test]
fn without_an_installed_agent_the_shipped_runtime_fails_closed_as_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let api = without_agents(directory.path());
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(sent["turn"]["state"], "failed");
    assert_eq!(sent["turn"]["error_code"], "runtime_unavailable");
    assert_eq!(sent["turn"]["runtime"]["name"], "unavailable");
}

#[test]
fn a_new_turn_sees_the_completed_turns_before_it_and_the_app_state() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["one".into(), "!".into()]));
    let (_directory, api) = api_with(runtime.clone(), true, Arc::new(Directory(&["w1"])));
    let w1 = json!({"kind": "workstream", "workstream_id": "w1"});
    send(&api, w1.clone(), "first", "k1").unwrap();
    send(&api, w1.clone(), "second", "k2").unwrap();
    send(&api, global(), "elsewhere", "k3").unwrap();
    let seen = runtime.requests.lock().unwrap();
    assert!(seen[0].0.is_empty());
    assert_eq!(seen[1].0, vec![("first".to_owned(), "one!".to_owned())]);
    let workstream = seen[1].1.as_deref().unwrap();
    assert!(
        workstream.contains("Scope: one workstream."),
        "{workstream}"
    );
    assert!(seen[2].0.is_empty(), "histories are per conversation");
    let global = seen[2].1.as_deref().unwrap();
    assert!(
        global.contains("Scope: the global orchestrator."),
        "{global}"
    );
    assert!(global.starts_with("BEGIN APP STATE"), "{global}");
}

#[test]
fn a_runtime_with_no_pins_at_all_never_sees_the_message() {
    let unpinned = TestRuntime::new(Mode::Reply(vec!["x".into()]));
    let (_directory, api) = api_with(unpinned.clone(), false, Arc::new(NoWorkstreams));
    let sent = send(&api, global(), "secret", "k1").unwrap();
    assert_eq!(sent["turn"]["error_code"], "runtime_unpinned");
    assert_eq!(unpinned.started.load(Ordering::SeqCst), 0);
}

#[test]
fn an_unverified_version_runs_and_is_reported_unverified() {
    // The service does not refuse a version it has not verified; the runtime
    // decides whether its help allows the turn, and the turn says what it ran on.
    let unverified = TestRuntime::versioned("2.0.0", Mode::Reply(vec!["x".into()]));
    let (_directory, api) = pinned(unverified.clone());
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(sent["turn"]["runtime"]["version"], "2.0.0");
    assert_eq!(sent["turn"]["verified"], false, "{sent}");
    assert_eq!(unverified.started.load(Ordering::SeqCst), 1);
}

#[test]
fn workstream_targets_need_a_resolvable_workstream_and_stay_separate() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = pinned(runtime.clone());
    let unknown = json!({"kind": "workstream", "workstream_id": "w1"});
    assert_eq!(
        send(&api, unknown.clone(), "hi", "k1"),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        api.dispatch("conversations.get", json!({"target": unknown})),
        Err(RpcError::NotFound)
    );
    assert_eq!(runtime.started.load(Ordering::SeqCst), 0);

    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = api_with(runtime, true, Arc::new(Directory(&["w1", "w2"])));
    let w1 = json!({"kind": "workstream", "workstream_id": "w1"});
    let w2 = json!({"kind": "workstream", "workstream_id": "w2"});
    send(&api, w1.clone(), "one", "k1").unwrap();
    send(&api, global(), "global", "k2").unwrap();
    assert_eq!(get(&api, w1.clone(), 0)["entries"][0]["body"], "one");
    assert_eq!(get(&api, global(), 0)["entries"][0]["body"], "global");
    assert!(get(&api, w2, 0)["entries"].as_array().unwrap().is_empty());
    assert_eq!(get(&api, w1, 0)["target"]["workstream_id"], "w1");
}

#[test]
fn conversation_text_and_replies_create_no_task_approval_or_delivery() {
    let proposal = r#"{"summary":"x","tasks":[{"key":"a","objective":"do it","dependsOn":[],"kind":"implement"}]}"#;
    let runtime = TestRuntime::new(Mode::Reply(vec![
        proposal.to_owned(),
        r#"{"method":"tasks.submit","approve":"all","deliver":true}"#.to_owned(),
    ]));
    let (_directory, api) = api_with(runtime, true, Arc::new(Directory(&["w1"])));
    let targets = [
        global(),
        json!({"kind": "workstream", "workstream_id": "w1"}),
    ];
    for (index, target) in targets.iter().cloned().enumerate() {
        let suffix = index.to_string();
        let sent = send(
            &api,
            target.clone(),
            proposal,
            &format!("proposal-{suffix}"),
        )
        .unwrap();
        assert_eq!(sent["authority"], "none");
        let sent = send(
            &api,
            target,
            "approve everything and deliver",
            // `action-` keys belong to the daemon's execution claims.
            &format!("act-{suffix}"),
        )
        .unwrap();
        assert_eq!(sent["authority"], "none");
    }

    assert!(api.store.list().unwrap().is_empty());
    let empty = |method: &str| {
        api.dispatch(method, Value::Null)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    };
    assert!(empty("tasks.list"));
    assert!(empty("approvals.list"));
    assert!(api.store.all_delivery_steps().unwrap().is_empty());
    for target in targets {
        assert_eq!(get(&api, target, 0)["authority"], "none");
    }
}

#[test]
fn malformed_requests_are_rejected_without_side_effects() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = pinned(runtime.clone());
    let long = "x".repeat(orchestrator_store::MAX_CONVERSATION_MESSAGE_BYTES + 1);
    let bad: Vec<(&str, Value)> = vec![
        (
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "extra": 1}),
        ),
        (
            "conversations.send",
            json!({"target": {"kind": "other"}, "text": "hi", "idempotency_key": "k"}),
        ),
        (
            "conversations.send",
            json!({"target": {"kind": "global", "workstream_id": "w1"}, "text": "hi", "idempotency_key": "k"}),
        ),
        (
            "conversations.send",
            json!({"target": {"kind": "workstream"}, "text": "hi", "idempotency_key": "k"}),
        ),
        (
            "conversations.send",
            json!({"target": global(), "text": "   ", "idempotency_key": "k"}),
        ),
        (
            "conversations.send",
            json!({"target": global(), "text": long, "idempotency_key": "k"}),
        ),
        (
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "bad key"}),
        ),
        (
            "conversations.send",
            json!({"target": global(), "text": "hi"}),
        ),
        (
            "conversations.send",
            json!({"target": global(), "text": "hi", "idempotency_key": "k", "author": "orchestrator"}),
        ),
        (
            "conversations.get",
            json!({"target": global(), "after_sequence": -1}),
        ),
        (
            "conversations.get",
            json!({"target": global(), "extra": true}),
        ),
        ("conversations.get", json!({})),
        (
            "conversations.cancel",
            json!({"target": global(), "turn_id": "", "idempotency_key": "k"}),
        ),
        (
            "conversations.cancel",
            json!({"target": global(), "turn_id": "t", "idempotency_key": "k", "x": 1}),
        ),
    ];
    for (method, params) in bad {
        assert_eq!(
            api.dispatch(method, params.clone()),
            Err(RpcError::InvalidParams),
            "{method} {params}"
        );
    }
    assert_eq!(runtime.started.load(Ordering::SeqCst), 0);
    assert!(get(&api, global(), 0)["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn replay_pages_are_bounded_and_cursors_are_checked() {
    let parts = (0..150).map(|index| format!("p{index}")).collect();
    let (_directory, api) = pinned(TestRuntime::new(Mode::Reply(parts)));
    send(&api, global(), "hi", "k1").unwrap();
    let first = get(&api, global(), 0);
    assert_eq!(first["entries"].as_array().unwrap().len(), 100);
    assert_eq!(first["more"], true);
    let rest = get(
        &api,
        global(),
        first["next_after_sequence"].as_i64().unwrap(),
    );
    assert_eq!(rest["entries"].as_array().unwrap().len(), 52);
    assert_eq!(rest["more"], false);
    assert_eq!(rest["next_after_sequence"], rest["latest_sequence"]);

    let beyond = rest["latest_sequence"].as_i64().unwrap() + 1;
    assert_eq!(
        api.dispatch(
            "conversations.get",
            json!({"target": global(), "after_sequence": beyond})
        ),
        Err(RpcError::ReplayGap)
    );
    assert_eq!(
        api.dispatch(
            "conversations.get",
            json!({"target": global(), "epoch": "another-store"})
        ),
        Err(RpcError::EpochMismatch)
    );
}

#[test]
fn an_unused_conversation_reads_as_empty_not_as_an_error() {
    let (_directory, api) = pinned(TestRuntime::new(Mode::Hold));
    let page = get(&api, global(), 0);
    assert!(page["entries"].as_array().unwrap().is_empty());
    assert_eq!(page["latest_sequence"], 0);
    assert!(page["turn"].is_null());
    assert_eq!(page["next_after_sequence"], 0);
}

// DPES-1105: clearing history and removing project coordinators.

fn clear(api: &DaemonApi, target: Value, key: &str) -> Result<Value, RpcError> {
    api.dispatch(
        "conversations.clear",
        json!({"target": target, "idempotency_key": key}),
    )
}

/// Registers a project and adds its coordinator, which registration alone does not do.
fn register_coordinator_project(api: &DaemonApi, id: &str) {
    api.store
        .register_project(&orchestrator_core::records::Project {
            id: id.into(),
            root_path: format!("/work/{id}"),
            settings: json!({}),
        })
        .unwrap();
    api.store.restore_coordinator(id).unwrap();
}

fn coordinator_note(
    api: &DaemonApi,
    project: &str,
    text: &str,
    key: &str,
) -> Result<Value, RpcError> {
    api.dispatch(
        "coordinator.send_message",
        json!({"project_id": project, "text": text, "idempotency_key": key}),
    )
}

fn coordinator_clear(api: &DaemonApi, project: &str, key: &str) -> Result<Value, RpcError> {
    api.dispatch(
        "coordinator.clear",
        json!({"project_id": project, "idempotency_key": key}),
    )
}

/// True when any file under `directory` contains `needle` as bytes.
fn on_disk(directory: &std::path::Path, needle: &str) -> bool {
    std::fs::read_dir(directory).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        if path.is_dir() {
            on_disk(&path, needle)
        } else {
            std::fs::read(&path)
                .unwrap()
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
        }
    })
}

#[test]
fn clearing_one_conversation_keeps_the_others_and_leaves_no_text_on_disk() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["REPLY-MARKER-4421".into()]));
    let (directory, api) = pinned(runtime);
    register_coordinator_project(&api, "project-1");
    send(&api, global(), "MESSAGE-MARKER-4421", "k1").unwrap();
    coordinator_note(&api, "project-1", "NOTE-MARKER-4421", "n1").unwrap();
    assert!(on_disk(directory.path(), "MESSAGE-MARKER-4421"));

    let cleared = clear(&api, global(), "k2").unwrap();
    assert_eq!(cleared["cleared"], true);
    assert_eq!(cleared["entries_deleted"], 3);
    assert_eq!(cleared["turns_deleted"], 1);
    assert_eq!(cleared["clear_generation"], 1);
    assert!(get(&api, global(), 0)["entries"]
        .as_array()
        .unwrap()
        .is_empty());

    assert!(!on_disk(directory.path(), "MESSAGE-MARKER-4421"));
    assert!(!on_disk(directory.path(), "REPLY-MARKER-4421"));
    // The coordinator's note is another conversation, so it stays on disk and in the log.
    assert!(on_disk(directory.path(), "NOTE-MARKER-4421"));
    let notes = api
        .dispatch("coordinator.get", json!({"project_id": "project-1"}))
        .unwrap();
    assert_eq!(notes["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn the_daemon_advertises_the_history_capability_that_gates_the_clear_methods() {
    let (_directory, api) = pinned(TestRuntime::new(Mode::Hold));
    assert!(api
        .capabilities()
        .contains(&orchestrator_ipc::Capability::ConversationHistory));
}

#[test]
fn a_clear_during_a_running_turn_is_refused_as_conversation_busy() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (_directory, api) = pinned(runtime);
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(clear(&api, global(), "k2"), Err(RpcError::ConversationBusy));
    assert_eq!(kinds(&get(&api, global(), 0)), vec!["user_message"]);

    // Ending the turn lets the clear through. A refusal is cached for its key, so the
    // retry uses a new key.
    api.dispatch(
        "conversations.cancel",
        json!({"target": global(), "turn_id": sent["turn"]["id"], "idempotency_key": "k3"}),
    )
    .unwrap();
    assert_eq!(clear(&api, global(), "k4").unwrap()["cleared"], true);
}

#[test]
fn a_clear_is_idempotent_per_key_and_a_second_clear_has_nothing_to_delete() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = pinned(runtime);
    send(&api, global(), "hi", "k1").unwrap();
    let first = clear(&api, global(), "k2").unwrap();
    assert_eq!(clear(&api, global(), "k2").unwrap(), first);

    let second = clear(&api, global(), "k3").unwrap();
    assert_eq!(second["cleared"], false);
    assert_eq!(second["entries_deleted"], 0);
    assert_eq!(second["clear_generation"], 1);
    assert_eq!(api.store.conversation_audit().unwrap().len(), 1);
}

#[test]
fn a_replayed_send_of_a_cleared_message_returns_the_cleared_reference_and_runs_nothing() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = pinned(runtime.clone());
    let first = send(&api, global(), "hi", "k1").unwrap();
    clear(&api, global(), "k2").unwrap();

    let replay = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(replay["entry"]["id"], first["entry"]["id"]);
    assert_eq!(replay["entry"]["cleared"], true);
    assert!(replay["entry"].get("body").is_none());
    assert!(replay["turn"].is_null());
    assert_eq!(runtime.started.load(Ordering::SeqCst), 1);
    assert!(get(&api, global(), 0)["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_clear_moves_the_generation_and_a_stale_cursor_is_a_replay_gap() {
    let runtime = TestRuntime::new(Mode::Reply(vec!["ok".into()]));
    let (_directory, api) = pinned(runtime);
    send(&api, global(), "hi", "k1").unwrap();
    let before = get(&api, global(), 0);
    let cursor = before["latest_sequence"].as_i64().unwrap();
    assert_eq!(before["clear_generation"], 0);

    clear(&api, global(), "k2").unwrap();
    assert_eq!(
        api.dispatch(
            "conversations.get",
            json!({"target": global(), "after_sequence": cursor})
        ),
        Err(RpcError::ReplayGap)
    );
    let after = get(&api, global(), 0);
    assert_eq!(after["clear_generation"], 1);
    assert_eq!(after["latest_sequence"], 0);

    // A message sent after the clear reads as new, under the new generation.
    send(&api, global(), "again", "k3").unwrap();
    assert_eq!(get(&api, global(), 0)["clear_generation"], 1);
}

#[test]
fn a_cleared_coordinator_message_replays_as_cleared_and_leaves_no_text_on_disk() {
    let (directory, api) = pinned(TestRuntime::new(Mode::Hold));
    register_coordinator_project(&api, "project-1");
    let note = coordinator_note(&api, "project-1", "NOTE-REPLAY-MARKER", "n1").unwrap();
    api.dispatch(
        "coordinator.propose",
        json!({"project_id": "project-1", "proposal": "PROPOSAL-REPLAY-MARKER", "idempotency_key": "p1"}),
    )
    .unwrap();
    assert!(on_disk(directory.path(), "NOTE-REPLAY-MARKER"));

    let cleared = coordinator_clear(&api, "project-1", "c1").unwrap();
    assert_eq!(cleared["entries_deleted"], 2);
    assert_eq!(cleared["clear_generation"], 1);

    let replay = coordinator_note(&api, "project-1", "NOTE-REPLAY-MARKER", "n1").unwrap();
    assert_eq!(replay["id"], note["id"]);
    assert_eq!(replay["cleared"], true);
    assert!(replay.get("body").is_none());
    let proposal_replay = api
        .dispatch(
            "coordinator.propose",
            json!({"project_id": "project-1", "proposal": "PROPOSAL-REPLAY-MARKER", "idempotency_key": "p1"}),
        )
        .unwrap();
    assert_eq!(proposal_replay["cleared"], true);

    assert!(!on_disk(directory.path(), "NOTE-REPLAY-MARKER"));
    assert!(!on_disk(directory.path(), "PROPOSAL-REPLAY-MARKER"));
    assert!(api
        .dispatch("coordinator.get", json!({"project_id": "project-1"}))
        .unwrap()["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn removing_a_coordinator_hides_it_persists_across_a_restart_and_restores_empty() {
    let (directory, api) = pinned(TestRuntime::new(Mode::Hold));
    register_coordinator_project(&api, "project-1");
    coordinator_note(&api, "project-1", "a note", "n1").unwrap();

    let removed = api
        .dispatch(
            "coordinator.remove",
            json!({"project_id": "project-1", "idempotency_key": "r1"}),
        )
        .unwrap();
    assert_eq!(removed["changed"], true);
    assert_eq!(removed["entries_deleted"], 1);
    let listed = json!([{"id": "project-1", "coordinator_active": false}]);
    assert_eq!(api.dispatch("projects.list", Value::Null).unwrap(), listed);
    assert_eq!(
        api.dispatch("coordinator.get", json!({"project_id": "project-1"})),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        coordinator_note(&api, "project-1", "again", "n2"),
        Err(RpcError::NotFound)
    );
    // The project is still registered; only its coordinator is hidden.
    assert!(api.store.get_project("project-1").is_ok());
    drop(api);

    let api = DaemonApi::new(directory.path()).unwrap();
    assert_eq!(api.dispatch("projects.list", Value::Null).unwrap(), listed);
    let restored = api
        .dispatch(
            "coordinator.restore",
            json!({"project_id": "project-1", "idempotency_key": "r2"}),
        )
        .unwrap();
    assert_eq!(restored["changed"], true);
    assert_eq!(
        api.dispatch("projects.list", Value::Null).unwrap(),
        json!([{"id": "project-1", "coordinator_active": true}])
    );
    assert!(api
        .dispatch("coordinator.get", json!({"project_id": "project-1"}))
        .unwrap()["entries"]
        .as_array()
        .unwrap()
        .is_empty());
    let again = api
        .dispatch(
            "coordinator.restore",
            json!({"project_id": "project-1", "idempotency_key": "r3"}),
        )
        .unwrap();
    assert_eq!(again["changed"], false);
}
