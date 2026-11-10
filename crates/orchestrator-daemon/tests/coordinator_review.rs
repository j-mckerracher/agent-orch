//! Process-level coordinator and review proofs against the real
//! `orchestrator-daemon` binary, over the platform's authenticated local
//! transport (Unix socket, or Windows named pipe). Runs on Unix and Windows.
//!
//! The daemon is a separate process; every request below crosses the real
//! transport, handshake and capability gate. Tasks are seeded into the store
//! of a daemon that is not running (public RPC cannot create a reviewable task
//! with a workspace and a passed verification run), and the review flow itself
//! runs only through `review.*`, `approvals.*` and `delivery.*` methods.
//! Daemon output goes to `daemon.log`, which is printed when a test fails.

mod support;

use orchestrator_core::records::Project;
use orchestrator_ipc::{Capability, Client, IpcError, Relay, RelayError, RelayEvent};
use orchestrator_store::SqliteStore;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use support::process::{connect, Daemon, Layout};
use support::seed;

const WAIT: Duration = Duration::from_secs(10);

fn target_ref(task_id: &str) -> String {
    format!("refs/heads/orchestrator/review/{task_id}")
}

fn seeded_layout(tasks: &[&str]) -> (Layout, Vec<Value>) {
    let layout = Layout::new();
    seed::init_repo(&layout.repo);
    let seeded = tasks
        .iter()
        .map(|task| seed::seed_review(&layout.data, &layout.repo, &layout.worktrees, task))
        .collect();
    (layout, seeded)
}

fn request_params(seeded: &Value) -> Value {
    json!({
        "task_id": seeded["task_id"],
        "workspace_id": seeded["workspace_id"],
        "verification_run_id": seeded["verification_run_id"],
        "action": "local_review",
        "target_ref": target_ref(seeded["task_id"].as_str().unwrap()),
    })
}

fn with_key(mut params: Value, key: &str) -> Value {
    params["idempotency_key"] = json!(key);
    params
}

fn delivery_params(seeded: &Value, approval_id: &str, key: &str) -> Value {
    json!({
        "task_id": seeded["task_id"],
        "workspace_id": seeded["workspace_id"],
        "approval_id": approval_id,
        "verification_run_id": seeded["verification_run_id"],
        "action": "local_review",
        "target_ref": target_ref(seeded["task_id"].as_str().unwrap()),
        "idempotency_key": key,
    })
}

fn ref_commit(repo: &std::path::Path, task_id: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--verify", "--quiet", &target_ref(task_id)])
        .output()
        .unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn raw(
    relay: &Relay,
    frames: &Receiver<RelayEvent>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    relay
        .send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
        .unwrap();
    loop {
        match frames.recv_timeout(WAIT).expect("response within 10 s") {
            RelayEvent::Message(message) if message["id"] == id => return message,
            RelayEvent::Message(_) => {}
            RelayEvent::Closed(reason) => panic!("relay closed: {reason:?}"),
        }
    }
}

#[test]
fn coordinator_conversation_survives_daemon_restart_and_grants_no_authority() {
    let layout = Layout::new();
    let store = SqliteStore::new(&layout.data.join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/private/project-root".into(),
            settings: json!({}),
        })
        .unwrap();
    drop(store);

    let mut first = Daemon::start(&layout);
    let mut client = connect(&layout.data);
    let message = client
        .coordinator_send_message("project-1", "Plan it", "msg-1")
        .unwrap();
    assert_eq!(message["author"], "user");
    let proposal = client
        .coordinator_propose("project-1", "not a plan", "prop-1")
        .unwrap();
    assert_eq!(proposal["validation"]["status"], "invalid");
    assert_eq!(proposal["validation"]["authority"], "none");
    assert_eq!(client.tasks_list().unwrap(), json!([]));
    assert!(matches!(
        client.coordinator_get("missing", None),
        Err(IpcError::NotFound)
    ));
    assert!(matches!(
        client.review_evidence("missing", None, None),
        Err(IpcError::NotFound)
    ));
    client.shutdown().unwrap();
    first.wait_for_exit();

    let _second = Daemon::start(&layout);
    let mut client = connect(&layout.data);
    let page = client.coordinator_get("project-1", None).unwrap();
    assert_eq!(page["entries"].as_array().unwrap().len(), 2);
    assert_eq!(page["entries"][0]["body"], "Plan it");
    assert_eq!(
        client
            .coordinator_send_message("project-1", "Plan it", "msg-1")
            .unwrap(),
        message,
        "idempotent replay survives restart"
    );
    assert!(!page.to_string().contains("/private/project-root"));
    client.shutdown().unwrap();
}

#[test]
fn daemon_gates_the_coordinator_and_review_methods_on_token_and_capability() {
    let layout = Layout::new();
    // No daemon yet: there is no token to present and nothing to relay to.
    assert!(matches!(
        Client::connect(&layout.data),
        Err(IpcError::Authentication)
    ));
    assert_eq!(
        Relay::open(&layout.data, &[Capability::Health], |_| {}).err(),
        Some(RelayError::Unavailable)
    );

    let mut daemon = Daemon::start(&layout);
    let mut client = connect(&layout.data);
    let negotiated = client.capabilities().unwrap();
    for capability in [
        Capability::Coordinator,
        Capability::Review,
        Capability::Approvals,
        Capability::Workspaces,
        Capability::Delivery,
        Capability::Events,
    ] {
        assert!(negotiated.contains(&capability), "{capability:?} missing");
    }

    // A client that did not ask for a capability cannot use it, locally...
    let mut health_only =
        Client::connect_with_capabilities(&layout.data, &[Capability::Health]).unwrap();
    assert!(matches!(
        health_only.review_evidence("task", None, None),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(matches!(
        health_only.coordinator_get("project", None),
        Err(IpcError::UnsupportedCapability)
    ));
    // ...and the daemon refuses the same raw requests on its side.
    let (events, frames) = mpsc::channel();
    let (relay, welcome) = Relay::open(&layout.data, &[Capability::Health], move |event| {
        let _ = events.send(event);
    })
    .unwrap();
    assert_eq!(welcome["capabilities"], json!(["health"]));
    for (id, method, params) in [
        (1, "review.evidence", json!({"task_id": "task"})),
        (2, "review.request_approval", json!({})),
        (3, "review.request_changes", json!({})),
        (4, "coordinator.get", json!({"project_id": "project"})),
        (5, "coordinator.send_message", json!({})),
        (6, "coordinator.propose", json!({})),
        (7, "approvals.list", json!(null)),
        (8, "approvals.resolve", json!({})),
        (9, "delivery.request", json!({})),
    ] {
        let response = raw(&relay, &frames, id, method, params);
        assert_eq!(response["error"]["code"], -32004, "{method}: {response}");
        assert!(response.get("result").is_none(), "{method}: {response}");
    }
    relay.close();

    // A wrong token is refused. The file is rewritten in place (same file
    // identity, same length) so the client still accepts it as the token file.
    let token_path = layout.data.join("daemon.token");
    let token = fs::read_to_string(&token_path).unwrap();
    assert_eq!(token.len(), 64);
    let wrong = format!(
        "{}{}",
        if token.starts_with('f') { "0" } else { "f" },
        &token[1..]
    );
    let write_token = |contents: &str| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&token_path)
            .unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.sync_all().unwrap();
    };
    write_token(&wrong);
    assert!(matches!(
        Client::connect(&layout.data),
        Err(IpcError::Authentication)
    ));
    assert_eq!(
        Relay::open(&layout.data, &[Capability::Review], |_| {}).err(),
        Some(RelayError::Authentication)
    );
    write_token(&token);
    let mut client = connect(&layout.data);
    assert!(matches!(
        client.review_evidence("missing", None, None),
        Err(IpcError::NotFound)
    ));
    client.shutdown().unwrap();
    daemon.wait_for_exit();
}

#[test]
fn approval_flow_approves_and_delivers_through_the_real_daemon() {
    let (layout, seeded) = seeded_layout(&["task-a"]);
    let seeded = &seeded[0];
    let task_id = seeded["task_id"].as_str().unwrap();
    let candidate = seeded["candidate_commit"].as_str().unwrap();
    let _daemon = Daemon::start(&layout);
    let mut client = connect(&layout.data);

    let before = client.review_evidence(task_id, None, None).unwrap();
    assert_eq!(before["state"], "missing");
    assert_eq!(before["review_state"], "not_requested");
    assert_eq!(before["decision_ready"], false);
    assert_eq!(before["candidate"]["commit"], candidate);
    let diff = client
        .workspaces_diff(task_id, seeded["workspace_id"].as_str(), None)
        .unwrap();
    assert!(diff["diff"].as_str().unwrap().contains("candidate.txt"));
    let early =
        client.delivery_request(delivery_params(seeded, "no-such-approval", "deliver-early"));
    assert!(
        matches!(early, Err(IpcError::Forbidden)),
        "delivery needs an approval: {early:?}"
    );
    assert_eq!(ref_commit(&layout.repo, task_id), None);

    let request = with_key(request_params(seeded), "request-1");
    let requested = client.review_request_approval(request.clone()).unwrap();
    assert_eq!(requested["approval"]["state"], "requested");
    assert_eq!(requested["approval"]["candidate_commit"], candidate);
    assert_eq!(requested["existing"], false);
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    assert!(!approval_id.is_empty());
    assert_eq!(client.review_request_approval(request).unwrap(), requested);

    let awaiting = client
        .review_evidence(task_id, Some(&approval_id), None)
        .unwrap();
    assert_eq!(awaiting["state"], "current");
    assert_eq!(awaiting["review_state"], "awaiting_decision");
    assert_eq!(awaiting["decision_ready"], true);
    assert_eq!(awaiting["digest_relationship"], "matches");
    assert_eq!(awaiting["workspace"]["id"], seeded["workspace_id"]);
    assert_eq!(awaiting["verification"][0]["status"], "passed");
    let pending = client.approvals_list().unwrap();
    assert!(
        pending
            .as_array()
            .unwrap()
            .iter()
            .any(|approval| approval["id"] == approval_id.as_str()),
        "{pending}"
    );

    let approved = client
        .approvals_resolve(&approval_id, "approve", "resolve-1")
        .unwrap();
    assert_eq!(approved["id"], approval_id.as_str());
    assert_eq!(approved["state"]["state"], "approved");
    assert_eq!(
        client
            .approvals_resolve(&approval_id, "approve", "resolve-1")
            .unwrap(),
        approved,
        "idempotent replay"
    );
    let after = client
        .review_evidence(task_id, Some(&approval_id), None)
        .unwrap();
    assert_eq!(after["review_state"], "approved");
    assert_eq!(after["approval"]["state"], "approved");

    let delivery = delivery_params(seeded, &approval_id, "deliver-1");
    let delivered = client.delivery_request(delivery.clone()).unwrap();
    assert_eq!(delivered["task_id"], task_id);
    assert_eq!(delivered["outcome"], "review_ref_created");
    assert_eq!(
        ref_commit(&layout.repo, task_id).as_deref(),
        Some(candidate),
        "the review ref points at the approved candidate"
    );
    assert_eq!(client.delivery_request(delivery).unwrap(), delivered);

    // The durable journal, read from a second connection, records the delivery.
    let mut observer = connect(&layout.data);
    observer.events_subscribe(Some(0)).unwrap();
    let (found, seen) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(event) = observer.next_event() {
            if found.send(event).is_err() {
                return;
            }
        }
    });
    loop {
        let event = seen.recv_timeout(WAIT).expect("delivery event within 10 s");
        if event.task_id == task_id && event.kind == "delivery.review_ref_created" {
            break;
        }
    }
    client.shutdown().unwrap();
}

#[test]
fn approval_flow_rejects_and_requests_changes_through_the_real_daemon() {
    let (layout, seeded) = seeded_layout(&["task-b", "task-c"]);
    let _daemon = Daemon::start(&layout);
    let mut client = connect(&layout.data);

    // Reject: the decision sticks, evidence calls the approval inactive, and
    // nothing is delivered or can be approved afterwards.
    let rejected_task = &seeded[0];
    let task_id = rejected_task["task_id"].as_str().unwrap();
    let requested = client
        .review_request_approval(with_key(request_params(rejected_task), "request-b"))
        .unwrap();
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    let rejected = client
        .approvals_resolve(&approval_id, "reject", "resolve-b")
        .unwrap();
    assert_eq!(rejected["state"]["state"], "rejected");
    assert_eq!(
        client
            .approvals_resolve(&approval_id, "reject", "resolve-b")
            .unwrap(),
        rejected,
        "idempotent replay"
    );
    let evidence = client
        .review_evidence(task_id, Some(&approval_id), None)
        .unwrap();
    assert_eq!(evidence["review_state"], "rejected");
    assert_eq!(evidence["state"], "stale");
    assert!(evidence["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("approval_inactive")));
    assert_eq!(evidence["decision_ready"], false);
    assert!(
        !client
            .approvals_list()
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|approval| approval["id"] == approval_id.as_str()),
        "a rejected approval is no longer pending"
    );
    let approve_late = client.approvals_resolve(&approval_id, "approve", "resolve-b-late");
    assert!(
        matches!(approve_late, Err(IpcError::Conflict)),
        "approve after reject: {approve_late:?}"
    );
    let late = client.delivery_request(delivery_params(rejected_task, &approval_id, "deliver-b"));
    assert!(
        matches!(late, Err(IpcError::Forbidden)),
        "delivery after reject: {late:?}"
    );
    assert_eq!(ref_commit(&layout.repo, task_id), None);

    // Request changes: the approval stays pending but cannot be approved.
    let changes_task = &seeded[1];
    let task_id = changes_task["task_id"].as_str().unwrap();
    let candidate = changes_task["candidate_commit"].as_str().unwrap();
    let requested = client
        .review_request_approval(with_key(request_params(changes_task), "request-c"))
        .unwrap();
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    let changes = client
        .review_request_changes(&approval_id, candidate, "Rename the file", "changes-c")
        .unwrap();
    assert_eq!(changes["review_state"], "changes_requested");
    assert_eq!(changes["approval_state"], "requested");
    assert_eq!(
        client
            .review_request_changes(&approval_id, candidate, "Rename the file", "changes-c")
            .unwrap(),
        changes,
        "idempotent replay"
    );
    let evidence = client
        .review_evidence(task_id, Some(&approval_id), None)
        .unwrap();
    assert_eq!(evidence["review_state"], "changes_requested");
    assert_eq!(evidence["decision_ready"], false);
    assert!(matches!(
        client.approvals_resolve(&approval_id, "approve", "resolve-c"),
        Err(IpcError::Conflict)
    ));
    assert_eq!(ref_commit(&layout.repo, task_id), None);
    assert!(matches!(
        client.review_request_changes("no-such-approval", candidate, "x", "changes-missing"),
        Err(IpcError::NotFound)
    ));
    client.shutdown().unwrap();
}
