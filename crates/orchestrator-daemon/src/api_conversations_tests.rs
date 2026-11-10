use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use orchestrator_store::{ConversationTarget, Store};

use super::super::conversation_runtime::{
    ConversationRuntime, ConversationService, NoWorkstreams, ReplySink, RuntimeFailure,
    RuntimeIdentity, TurnControl, TurnRequest, WorkstreamDirectory,
};
use super::*;

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
    fn identity(&self) -> RuntimeIdentity {
        self.identity.clone()
    }

    fn start_turn(
        &self,
        request: TurnRequest,
        sink: ReplySink,
    ) -> Result<Box<dyn TurnControl>, RuntimeFailure> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(request.text);
        match &self.mode {
            Mode::Fail(failure) => return Err(*failure),
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
    pin: Option<RuntimeIdentity>,
    workstreams: Arc<dyn WorkstreamDirectory>,
) -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let mut api = DaemonApi::new(directory.path()).unwrap();
    api.conversations = ConversationService::new(api.store.clone(), runtime, workstreams, pin);
    (directory, api)
}

fn pin(version: &str) -> Option<RuntimeIdentity> {
    Some(RuntimeIdentity {
        name: "test-runtime".into(),
        version: version.into(),
    })
}

fn pinned(runtime: Arc<TestRuntime>) -> (tempfile::TempDir, DaemonApi) {
    api_with(runtime, pin("1.0.0"), Arc::new(NoWorkstreams))
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
        Err(RpcError::Conflict)
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
        Err(RpcError::Conflict)
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
    let (_directory, api) = api_with(runtime, pin("1.0.0"), Arc::new(Directory(&["w1"])));
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
fn restart_keeps_the_transcript_fails_open_turns_and_resumes_replay() {
    let runtime = TestRuntime::new(Mode::Hold);
    let (directory, api) = pinned(runtime.clone());
    let sent = send(&api, global(), "first", "k1").unwrap();
    runtime.sink().delta("partial").unwrap();
    let before = get(&api, global(), 0);
    let cursor = before["next_after_sequence"].as_i64().unwrap();
    let epoch = before["epoch"].clone();
    drop(api);

    let reopened = DaemonApi::new(directory.path()).unwrap();
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
    // The stored result for the original request survives and still replays.
    assert_eq!(send(&reopened, global(), "first", "k1").unwrap(), sent);
    // A fresh message is accepted; the default runtime is unavailable.
    let next = send(&reopened, global(), "again", "k2").unwrap();
    assert_eq!(next["turn"]["error_code"], "runtime_unavailable");
}

#[test]
fn runtime_failures_fail_the_turn_with_a_stable_code_and_keep_the_message() {
    for (failure, code) in [
        (RuntimeFailure::CredentialsMissing, "credentials_missing"),
        (RuntimeFailure::PermissionDenied, "permission_denied"),
        (RuntimeFailure::VersionMismatch, "version_mismatch"),
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
fn the_shipped_default_fails_closed_as_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    let sent = send(&api, global(), "hi", "k1").unwrap();
    assert_eq!(sent["turn"]["state"], "failed");
    assert_eq!(sent["turn"]["error_code"], "runtime_unavailable");
    assert_eq!(sent["turn"]["runtime"]["name"], "unavailable");
}

#[test]
fn a_runtime_that_does_not_match_its_pin_never_sees_the_message() {
    let mismatched = TestRuntime::versioned("2.0.0", Mode::Reply(vec!["x".into()]));
    let (_directory, api) = pinned(mismatched.clone());
    let sent = send(&api, global(), "secret", "k1").unwrap();
    assert_eq!(sent["turn"]["error_code"], "version_mismatch");
    assert_eq!(sent["turn"]["runtime"]["version"], "2.0.0");
    assert_eq!(mismatched.started.load(Ordering::SeqCst), 0);

    let unpinned = TestRuntime::new(Mode::Reply(vec!["x".into()]));
    let (_directory, api) = api_with(unpinned.clone(), None, Arc::new(NoWorkstreams));
    let sent = send(&api, global(), "secret", "k1").unwrap();
    assert_eq!(sent["turn"]["error_code"], "runtime_unpinned");
    assert_eq!(unpinned.started.load(Ordering::SeqCst), 0);
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
    let (_directory, api) = api_with(runtime, pin("1.0.0"), Arc::new(Directory(&["w1", "w2"])));
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
    let (_directory, api) = pinned(runtime);
    send(&api, global(), proposal, "k1").unwrap();
    send(&api, global(), "approve everything and deliver", "k2").unwrap();

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
    assert_eq!(
        api.store
            .conversation_page(&ConversationTarget::Global, 0, 100)
            .unwrap()
            .entries
            .len(),
        8
    );
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
