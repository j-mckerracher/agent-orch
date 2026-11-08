use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, VerificationRun};
use orchestrator_store::{SqliteStore, Store};
use serde_json::{json, Value};
use tempfile::TempDir;

fn create_task(store: &SqliteStore) {
    store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/repo".into(),
            settings: json!({}),
        })
        .unwrap();
    store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "verify candidate".into(),
            kind: TaskKind::Implement,
            agent: "test-agent".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::Commands {
                commands: vec![vec!["cargo".into(), "test".into()]],
            },
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
        })
        .unwrap();
}

fn run(id: &str, commit: &str, status: &str) -> VerificationRun {
    VerificationRun {
        id: id.into(),
        task_id: "task-1".into(),
        commit: commit.into(),
        status: status.into(),
        log_path: None,
    }
}

fn details(check: &str, trust: &str) -> Value {
    json!({
        "checks": [{"name": "candidate", "outcome": check}],
        "commands": [{
            "trust": trust,
            "status": check,
            "exit_code": if check == "passed" { Some(0) } else { None },
            "stdout": "captured output",
        }],
    })
}

#[test]
fn appends_and_reopens_results_without_overwriting_failed_runs() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("store.sqlite");
    let failed_run = run("verification-1", "candidate-a", "failed");
    let failed_details = details("rejected", "denied");
    let passed_run = run("verification-2", "candidate-b", "passed");
    let passed_details = details("passed", "trusted");

    {
        let store = SqliteStore::new(&database).unwrap();
        create_task(&store);
        store
            .record_verification_result(&failed_run, 100, 200, &failed_details)
            .unwrap();
        let attempted_overwrite = run("verification-1", "candidate-changed", "passed");
        assert!(store
            .record_verification_result(
                &attempted_overwrite,
                300,
                400,
                &details("passed", "trusted"),
            )
            .is_err());
        store
            .record_verification_result(&passed_run, 300, 400, &passed_details)
            .unwrap();
    }

    let reopened = SqliteStore::new(&database).unwrap();
    let results = reopened.verification_results_for_task("task-1").unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, failed_run);
    assert_eq!(results[0].1["details"], failed_details);
    assert_eq!(results[0].1["started_at_ms"], json!(100));
    assert_eq!(results[0].1["finished_at_ms"], json!(200));
    assert_eq!(results[1].0, passed_run);
    assert_eq!(results[1].1["details"], passed_details);
    assert_eq!(results[1].1["started_at_ms"], json!(300));
    assert_eq!(results[1].1["finished_at_ms"], json!(400));
}
