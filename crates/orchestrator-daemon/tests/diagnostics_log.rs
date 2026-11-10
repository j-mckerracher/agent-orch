//! Process-level proof that the real daemon explains its failures in
//! `daemon.log` and never writes a credential there. Runs on Unix and Windows
//! over the platform's authenticated local transport; every wait is seconds
//! scale and the daemon log is printed (redacted) when a test fails.

mod support;

use orchestrator_ipc::Client;
use serde_json::{json, Value};
use std::fs;
use std::thread;
use std::time::{Duration, Instant};
use support::process::{connect, Daemon, Layout};
use support::seed;

const WAIT: Duration = Duration::from_secs(20);
const SECRET: &str = "SECRETVALUE0123456789";

fn log_events(layout: &Layout) -> Vec<Value> {
    fs::read_to_string(&layout.log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn wait_for_event(layout: &Layout, event: &str) -> Value {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = log_events(layout)
            .into_iter()
            .find(|candidate| candidate["event"] == event)
        {
            return found;
        }
        assert!(Instant::now() < deadline, "no `{event}` event in daemon.log");
        thread::sleep(Duration::from_millis(50));
    }
}

fn submit_codex(client: &mut Client, project_id: &str) -> String {
    let task = client
        .tasks_submit(json!({
            "idempotency_key": "diag-1",
            "contract": {
                "project_id": project_id,
                "parent_id": null,
                "objective": "never launches",
                "kind": "Implement",
                "agent": "codex",
                "dependencies": [],
                "verification": "None",
                "delivery": "ReportOnly",
                "autonomy": "WorkspaceWrites",
                "max_retries": 0,
                "max_repairs": 0
            }
        }))
        .unwrap();
    task["contract"]["id"].as_str().unwrap().to_owned()
}

#[test]
fn a_failed_launch_is_explained_in_the_log_without_the_credential() {
    let layout = Layout::new();
    seed::init_repo(&layout.repo);
    let missing = layout.data.parent().unwrap().join(format!("token={SECRET}"));
    let token_file = layout.data.join("daemon.token");

    let daemon = Daemon::start_with_env(&layout, &[("AGENT_ORCH_CODEX_PROGRAM", &missing)]);
    let token = fs::read_to_string(&token_file).unwrap().trim().to_owned();
    assert_eq!(token.len(), 64, "the token under test is the real one");

    let mut client = connect(&layout.data);
    let project = client
        .projects_register(&layout.repo.display().to_string())
        .unwrap();
    let project_id = project["id"].as_str().unwrap().to_owned();
    let task_id = submit_codex(&mut client, &project_id);

    let launch = wait_for_event(&layout, "agent_launch_failed");
    assert_eq!(launch["level"], "error");
    assert_eq!(launch["component"], "executor");
    assert_eq!(launch["task_id"], task_id.as_str());
    assert_eq!(launch["agent"], "codex");
    wait_for_event(&layout, "daemon_start");

    let task = client.tasks_get(&task_id).unwrap();
    assert_eq!(task["state"], "Failed", "the failure is also in the task state");

    let log = fs::read_to_string(&layout.log).unwrap();
    assert!(!log.contains(SECRET), "credential in daemon.log:\n{log}");
    assert!(!log.contains(&token), "ipc token in daemon.log:\n{log}");
    drop(client);
    drop(daemon);
}
