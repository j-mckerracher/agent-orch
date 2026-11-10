//! The desktop host bridge against the real `orchestrator-daemon` process.
//!
//! No in-process fake and no fixture: the daemon binary is built from this
//! repository, a reviewable task is seeded into its store, and the host
//! `Bridge` starts the daemon itself through `launch::spawn` (the detached
//! launch path of the installed app) and talks to it over the platform's
//! authenticated local transport, a Unix socket or a Windows named pipe. Every
//! method below is the daemon's own `coordinator.*`, `review.*`, `approvals.*`,
//! `workspaces.*` or `delivery.*` method; an unknown id is refused by the
//! daemon, never answered from canned data. Runs in `cargo test` with no GUI.
//!
//! The daemon and the `seed_review` example are built once per test process
//! (nested `cargo build --locked` into a separate target directory) unless
//! `AGENT_ORCH_DAEMON_BIN` and `AGENT_ORCH_SEED_BIN` name prebuilt binaries. A
//! failed build fails the test with the build output; nothing is skipped.
//! The daemon's `daemon.log` is printed when a test fails.

use super::*;
use orchestrator_ipc::Client;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::OnceLock;
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(10);
const STOP: Duration = Duration::from_secs(15);
const BUILD_FAILED: &str = "building the real daemon failed";

struct Binaries {
    daemon: PathBuf,
    seed: PathBuf,
}

fn binaries() -> &'static Binaries {
    static BINARIES: OnceLock<Binaries> = OnceLock::new();
    BINARIES.get_or_init(|| {
        let prebuilt = (
            std::env::var_os("AGENT_ORCH_DAEMON_BIN").filter(|v| !v.is_empty()),
            std::env::var_os("AGENT_ORCH_SEED_BIN").filter(|v| !v.is_empty()),
        );
        if let (Some(daemon), Some(seed)) = prebuilt {
            return Binaries {
                daemon: daemon.into(),
                seed: seed.into(),
            };
        }
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.ancestors().nth(3).expect("repository root");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| manifest.join("target"))
            .join("real-daemon");
        let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "--locked", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .args(["-p", "orchestrator-daemon", "--bin", "orchestrator-daemon"])
            .args(["--example", "seed_review", "--target-dir"])
            .arg(&target)
            .output()
            .expect("cargo runs");
        assert!(
            output.status.success(),
            "{BUILD_FAILED}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let suffix = std::env::consts::EXE_SUFFIX;
        Binaries {
            daemon: target
                .join("debug")
                .join(format!("orchestrator-daemon{suffix}")),
            seed: target
                .join("debug")
                .join("examples")
                .join(format!("seed_review{suffix}")),
        }
    })
}

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(path)
        .args([
            "-c",
            "user.name=e2e",
            "-c",
            "user.email=e2e@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A seeded data directory plus the host bridge that will launch the daemon
/// over it. Short paths (`ao*` root, one-letter children) keep Windows under
/// MAX_PATH.
struct Fixture {
    _root: TempDir,
    data: PathBuf,
    repo: PathBuf,
    bridge: Bridge,
    seeded: Vec<Value>,
}

impl Fixture {
    fn new(tasks: &[&str]) -> Self {
        let binaries = binaries();
        let root = tempfile::Builder::new().prefix("ao").tempdir().unwrap();
        let data = root.path().join("d");
        let repo = root.path().join("r");
        let worktrees = root.path().join("w");
        fs::create_dir(&data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        fs::write(repo.join("README.md"), "fixture repository\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        let seeded = tasks
            .iter()
            .map(|task| {
                let output = Command::new(&binaries.seed)
                    .arg(&data)
                    .arg(&repo)
                    .arg(&worktrees)
                    .arg(task)
                    .output()
                    .expect("seed_review runs");
                assert!(
                    output.status.success(),
                    "seed_review failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                serde_json::from_slice(&output.stdout).expect("seed_review prints JSON")
            })
            .collect();
        let bridge = Bridge::new(Some(data.clone()), Some(binaries.daemon.clone()));
        Self {
            _root: root,
            data,
            repo,
            bridge,
            seeded,
        }
    }

    fn open(&self, window: &str, capabilities: &[Capability]) -> (Opened, Frames) {
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let emit = move |payload| sender.lock().unwrap().send(payload).is_ok();
        let opened = self
            .bridge
            .open(window, capabilities, emit)
            .unwrap_or_else(|error| panic!("the host could not open the daemon: {error:?}"));
        (
            opened,
            Frames {
                receiver,
                window: window.to_owned(),
                next_id: 1,
                seen: Vec::new(),
            },
        )
    }

    fn token(&self) -> String {
        fs::read_to_string(self.data.join("daemon.token")).unwrap()
    }

    fn ref_commit(&self, task_id: &str) -> Option<String> {
        let output = Command::new("git")
            .current_dir(&self.repo)
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/orchestrator/review/{task_id}"),
            ])
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The host never stops the daemon, so the test does, over a plain
        // client, and waits for the token to go away so the directory can be
        // removed.
        if let Ok(mut client) = Client::connect_with_capabilities(&self.data, &[Capability::Health])
        {
            let _ = client.shutdown();
        }
        let deadline = Instant::now() + STOP;
        while self.data.join("daemon.token").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        if thread::panicking() {
            eprintln!(
                "--- daemon log ({}) ---\n{}\n--- end daemon log ---",
                self.data.join(launch::DAEMON_LOG).display(),
                fs::read_to_string(self.data.join(launch::DAEMON_LOG)).unwrap_or_default()
            );
        }
    }
}

/// The frames the host emitted to one window.
struct Frames {
    receiver: Receiver<Value>,
    window: String,
    next_id: u64,
    seen: Vec<String>,
}

impl Frames {
    fn next(&mut self) -> Value {
        let payload = self.receiver.recv_timeout(WAIT).expect("frame within 10 s");
        self.seen.push(payload.to_string());
        payload
    }

    /// Sends one request through the bridge and returns the daemon's response
    /// object (`result` or `error`), skipping interleaved notifications.
    fn call(&mut self, bridge: &Bridge, connection: u64, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        bridge
            .send(
                &self.window,
                connection,
                json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            )
            .unwrap();
        loop {
            let payload = self.next();
            assert_eq!(payload["kind"], "message", "{payload}");
            if payload["message"]["id"] == id {
                return payload["message"].clone();
            }
        }
    }

    fn result(&mut self, bridge: &Bridge, connection: u64, method: &str, params: Value) -> Value {
        let response = self.call(bridge, connection, method, params);
        assert!(response.get("error").is_none(), "{method}: {response}");
        response["result"].clone()
    }

    fn error_code(&mut self, bridge: &Bridge, connection: u64, method: &str, params: Value) -> i64 {
        let response = self.call(bridge, connection, method, params);
        assert!(response.get("result").is_none(), "{method}: {response}");
        response["error"]["code"].as_i64().expect("error code")
    }
}

fn target_ref(task_id: &str) -> String {
    format!("refs/heads/orchestrator/review/{task_id}")
}

fn request_params(seeded: &Value, key: &str) -> Value {
    json!({
        "task_id": seeded["task_id"],
        "workspace_id": seeded["workspace_id"],
        "verification_run_id": seeded["verification_run_id"],
        "action": "local_review",
        "target_ref": target_ref(seeded["task_id"].as_str().unwrap()),
        "idempotency_key": key,
    })
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

const FORBIDDEN: i64 = -32012;
const CONFLICT: i64 = -32011;
const NOT_FOUND: i64 = -32010;
const NOT_NEGOTIATED: i64 = -32004;

#[test]
fn the_host_launches_the_real_daemon_and_drives_an_approval_to_delivery() {
    let fixture = Fixture::new(&["task-a"]);
    let seeded = &fixture.seeded[0];
    let task_id = seeded["task_id"].as_str().unwrap();
    let candidate = seeded["candidate_commit"].as_str().unwrap();
    let project_id = seeded["project_id"].as_str().unwrap();

    let (opened, mut frames) = fixture.open("main", &Capability::ALL);
    assert!(opened.launched_by_host, "the host starts the daemon itself");
    assert_eq!(opened.welcome["version"], 1);
    let negotiated = opened.welcome["capabilities"].as_array().unwrap();
    for capability in [
        "coordinator",
        "review",
        "approvals",
        "workspaces",
        "delivery",
    ] {
        assert!(negotiated.contains(&json!(capability)), "{capability}");
    }
    let bridge = &fixture.bridge;
    let connection = opened.connection_id;

    // Coordinator: the real conversation store, idempotent, no authority.
    let message = frames.result(
        bridge,
        connection,
        "coordinator.send_message",
        json!({"project_id": project_id, "text": "Review it", "idempotency_key": "msg-1"}),
    );
    assert_eq!(message["author"], "user");
    let page = frames.result(
        bridge,
        connection,
        "coordinator.get",
        json!({"project_id": project_id, "after_sequence": null}),
    );
    assert_eq!(page["entries"].as_array().unwrap().len(), 1);
    assert_eq!(page["entries"][0]["body"], "Review it");
    assert_eq!(
        frames.error_code(
            bridge,
            connection,
            "coordinator.get",
            json!({"project_id": "no-such-project", "after_sequence": null}),
        ),
        NOT_FOUND,
        "an unknown project is refused, not invented"
    );

    // Review: nothing requested yet, and delivery is refused without approval.
    let evidence = frames.result(
        bridge,
        connection,
        "review.evidence",
        json!({"task_id": task_id, "approval_id": null, "path": null}),
    );
    assert_eq!(evidence["state"], "missing");
    assert_eq!(evidence["review_state"], "not_requested");
    assert_eq!(evidence["candidate"]["commit"], candidate);
    assert_eq!(
        frames.error_code(
            bridge,
            connection,
            "delivery.request",
            delivery_params(seeded, "no-such-approval", "deliver-early"),
        ),
        FORBIDDEN
    );
    assert_eq!(fixture.ref_commit(task_id), None);

    // Pending approval, then approve.
    let requested = frames.result(
        bridge,
        connection,
        "review.request_approval",
        request_params(seeded, "request-1"),
    );
    assert_eq!(requested["approval"]["state"], "requested");
    assert_eq!(requested["approval"]["candidate_commit"], candidate);
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    let awaiting = frames.result(
        bridge,
        connection,
        "review.evidence",
        json!({"task_id": task_id, "approval_id": approval_id, "path": null}),
    );
    assert_eq!(awaiting["review_state"], "awaiting_decision");
    assert_eq!(awaiting["decision_ready"], true);
    let pending = frames.result(bridge, connection, "approvals.list", Value::Null);
    assert!(
        pending
            .as_array()
            .unwrap()
            .iter()
            .any(|approval| approval["id"] == approval_id.as_str()),
        "{pending}"
    );
    let approved = frames.result(
        bridge,
        connection,
        "approvals.resolve",
        json!({"approval_id": approval_id, "decision": "approve", "idempotency_key": "resolve-1"}),
    );
    assert_eq!(approved["state"]["state"], "approved");
    let after = frames.result(
        bridge,
        connection,
        "review.evidence",
        json!({"task_id": task_id, "approval_id": approval_id, "path": null}),
    );
    assert_eq!(after["review_state"], "approved");

    // Delivery creates the review ref at the approved candidate, and the
    // durable journal replayed over the bridge records it.
    let delivered = frames.result(
        bridge,
        connection,
        "delivery.request",
        delivery_params(seeded, &approval_id, "deliver-1"),
    );
    assert_eq!(delivered["outcome"], "review_ref_created");
    assert_eq!(fixture.ref_commit(task_id).as_deref(), Some(candidate));
    frames.result(
        bridge,
        connection,
        "events.subscribe",
        json!({"after_sequence": 0}),
    );
    loop {
        let payload = frames.next();
        let event = &payload["message"]["params"]["event"];
        if event["task_id"] == task_id && event["kind"] == "delivery.review_ref_created" {
            break;
        }
    }

    // The webview-facing frames never carried the token.
    let token = fixture.token();
    assert_eq!(token.len(), 64);
    assert!(
        frames.seen.iter().all(|frame| !frame.contains(&token)),
        "the token reached the webview"
    );
    assert!(!opened.welcome.to_string().contains(&token));

    // A second window reaches the already-running daemon instead of
    // launching another, and observes the same decision.
    bridge.close_window("main");
    let (reopened, mut frames) = fixture.open("main", &Capability::ALL);
    assert!(!reopened.launched_by_host);
    let state = frames.result(
        bridge,
        reopened.connection_id,
        "review.evidence",
        json!({"task_id": task_id, "approval_id": approval_id, "path": null}),
    );
    assert_eq!(state["review_state"], "approved");
    bridge.close_window("main");
}

#[test]
fn the_host_drives_reject_and_request_changes_through_the_real_daemon() {
    let fixture = Fixture::new(&["task-b", "task-c"]);
    let bridge = &fixture.bridge;
    let (opened, mut frames) = fixture.open("main", &Capability::ALL);
    assert!(opened.launched_by_host);
    let connection = opened.connection_id;

    let rejected_task = &fixture.seeded[0];
    let task_id = rejected_task["task_id"].as_str().unwrap();
    let requested = frames.result(
        bridge,
        connection,
        "review.request_approval",
        request_params(rejected_task, "request-b"),
    );
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    let rejected = frames.result(
        bridge,
        connection,
        "approvals.resolve",
        json!({"approval_id": approval_id, "decision": "reject", "idempotency_key": "resolve-b"}),
    );
    assert_eq!(rejected["state"]["state"], "rejected");
    let evidence = frames.result(
        bridge,
        connection,
        "review.evidence",
        json!({"task_id": task_id, "approval_id": approval_id, "path": null}),
    );
    assert_eq!(evidence["review_state"], "rejected");
    assert_eq!(evidence["state"], "stale");
    assert!(evidence["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("approval_inactive")));
    assert_eq!(
        frames.error_code(
            bridge,
            connection,
            "approvals.resolve",
            json!({
                "approval_id": approval_id,
                "decision": "approve",
                "idempotency_key": "resolve-b-late"
            }),
        ),
        CONFLICT
    );
    assert_eq!(
        frames.error_code(
            bridge,
            connection,
            "delivery.request",
            delivery_params(rejected_task, &approval_id, "deliver-b"),
        ),
        FORBIDDEN
    );
    assert_eq!(fixture.ref_commit(task_id), None);

    let changes_task = &fixture.seeded[1];
    let task_id = changes_task["task_id"].as_str().unwrap();
    let candidate = changes_task["candidate_commit"].as_str().unwrap();
    let requested = frames.result(
        bridge,
        connection,
        "review.request_approval",
        request_params(changes_task, "request-c"),
    );
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    let changes = frames.result(
        bridge,
        connection,
        "review.request_changes",
        json!({
            "approval_id": approval_id,
            "candidate_commit": candidate,
            "feedback": "Rename the file",
            "idempotency_key": "changes-c"
        }),
    );
    assert_eq!(changes["review_state"], "changes_requested");
    assert_eq!(changes["approval_state"], "requested");
    assert_eq!(
        frames.error_code(
            bridge,
            connection,
            "approvals.resolve",
            json!({
                "approval_id": approval_id,
                "decision": "approve",
                "idempotency_key": "resolve-c"
            }),
        ),
        CONFLICT
    );
    assert_eq!(fixture.ref_commit(task_id), None);
    bridge.close_window("main");
}

#[test]
fn the_host_forwards_capability_and_token_gating_from_the_real_daemon() {
    let fixture = Fixture::new(&["task-d"]);
    let bridge = &fixture.bridge;
    let task_id = fixture.seeded[0]["task_id"].as_str().unwrap();

    // A window that negotiated only `events` is refused review, coordinator,
    // approval and delivery methods by the daemon itself.
    let (opened, mut frames) = fixture.open("main", &[Capability::Events]);
    assert!(opened.launched_by_host);
    let negotiated = opened.welcome["capabilities"].as_array().unwrap();
    assert!(negotiated.contains(&json!("events")));
    assert!(!negotiated.contains(&json!("review")));
    let connection = opened.connection_id;
    for (method, params) in [
        ("review.evidence", json!({"task_id": task_id})),
        ("review.request_approval", json!({})),
        ("review.request_changes", json!({})),
        ("coordinator.get", json!({"project_id": "e2e-project"})),
        ("coordinator.send_message", json!({})),
        ("approvals.list", Value::Null),
        ("approvals.resolve", json!({})),
        ("delivery.request", json!({})),
    ] {
        assert_eq!(
            frames.error_code(bridge, connection, method, params),
            NOT_NEGOTIATED,
            "{method}"
        );
    }

    // The host refuses to forward shutdown or a second handshake.
    for method in ["shutdown", "handshake"] {
        let error = bridge
            .send(
                "main",
                connection,
                json!({"jsonrpc": "2.0", "id": 99, "method": method}),
            )
            .unwrap_err();
        assert_eq!(error.code, "protocol");
    }
    bridge.close_window("main");

    // A wrong token is refused as an authentication failure, not retried as a
    // missing daemon and not launched over. The file is rewritten in place.
    let token = fixture.token();
    let token_path = fixture.data.join("daemon.token");
    let wrong = format!(
        "{}{}",
        if token.starts_with('f') { "0" } else { "f" },
        &token[1..]
    );
    let write_token = |contents: &str| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&token_path)
            .unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.sync_all().unwrap();
    };
    write_token(&wrong);
    let (emit, _frames) = mpsc::channel::<Value>();
    let emit = Mutex::new(emit);
    let error = bridge
        .open("main", &[Capability::Review], move |payload| {
            emit.lock().unwrap().send(payload).is_ok()
        })
        .unwrap_err();
    assert_eq!(error.code, RelayError::Authentication.code(), "{error:?}");
    write_token(&token);
    let (opened, mut frames) = fixture.open("main", &[Capability::Review]);
    assert!(!opened.launched_by_host);
    assert_eq!(
        frames.error_code(
            bridge,
            opened.connection_id,
            "review.evidence",
            json!({"task_id": "no-such-task", "approval_id": null, "path": null}),
        ),
        NOT_FOUND,
        "an unknown task is refused by the daemon, not answered from a fixture"
    );
    bridge.close_window("main");
}
