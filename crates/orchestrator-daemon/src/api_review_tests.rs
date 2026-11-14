use super::*;
use orchestrator_core::contract::{AutonomyPolicy, DeliveryPolicy, TaskKind, VerificationPolicy};
use orchestrator_core::records::{Project, VerificationRun, Workspace};
use orchestrator_core::task::TaskEvent;
use orchestrator_git::{create_task_worktree, Repository};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const TARGET_REF: &str = "refs/heads/orchestrator/review/task-1";

struct Fixture {
    _directory: TempDir,
    api: DaemonApi,
    repository_path: PathBuf,
    worktree_path: PathBuf,
    base_sha: String,
    candidate_sha: String,
}

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit_file(worktree: &Path, name: &str, contents: &str) -> String {
    fs::write(worktree.join(name), contents).unwrap();
    git(worktree, &["add", name]);
    git(worktree, &["commit", "-m", name]);
    git(worktree, &["rev-parse", "HEAD"])
}

fn contract(id: &str, project_id: &str, delivery: DeliveryPolicy) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: project_id.into(),
        parent_id: None,
        objective: "Review one candidate".into(),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 0,
        max_repairs: 0,
        model_selection: None,
    }
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let repository_path = directory.path().join("repo");
    fs::create_dir_all(&repository_path).unwrap();
    git(&repository_path, &["init", "-b", "main"]);
    git(
        &repository_path,
        &["config", "user.email", "test@example.invalid"],
    );
    git(&repository_path, &["config", "user.name", "Review Test"]);
    // The `git()` helper ignores global and system config, but the code under
    // test does not. Git for Windows ships `core.autocrlf=true` in its system
    // config, so without this the task worktree would check `base.txt` out
    // with CRLF endings and a later `git add -A` here would commit it as a
    // modification.
    git(&repository_path, &["config", "core.autocrlf", "false"]);
    fs::write(repository_path.join("base.txt"), "base\n").unwrap();
    git(&repository_path, &["add", "base.txt"]);
    git(&repository_path, &["commit", "-m", "base"]);
    let repository = Repository::new(&repository_path).unwrap();
    let base_sha = repository.resolve_base_ref("HEAD").unwrap();
    let worktree = create_task_worktree(
        &repository,
        "task-1",
        "main",
        &directory.path().join("worktrees"),
    )
    .unwrap();
    let worktree_path = worktree.path().to_path_buf();
    let candidate_sha = commit_file(&worktree_path, "candidate.txt", "candidate\n");

    let api = DaemonApi::new(&directory.path().join("data")).unwrap();
    api.store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: repository_path.to_string_lossy().into_owned(),
            settings: json!({}),
        })
        .unwrap();
    api.store
        .create(&contract(
            "task-1",
            "project-1",
            DeliveryPolicy::LocalReview,
        ))
        .unwrap();
    api.store.transition("task-1", TaskEvent::Admit).unwrap();
    api.store
        .add_workspace(&Workspace {
            id: "workspace-1".into(),
            task_id: "task-1".into(),
            path: worktree_path.to_string_lossy().into_owned(),
            branch: worktree.branch().into(),
            base_commit: base_sha.clone(),
        })
        .unwrap();
    api.store
        .add_verification_run(&VerificationRun {
            id: "verification-1".into(),
            task_id: "task-1".into(),
            commit: candidate_sha.clone(),
            status: "passed".into(),
            log_path: None,
        })
        .unwrap();
    Fixture {
        _directory: directory,
        api,
        repository_path,
        worktree_path,
        base_sha,
        candidate_sha,
    }
}

fn request_params(key: &str) -> Value {
    json!({
        "task_id": "task-1",
        "workspace_id": "workspace-1",
        "verification_run_id": "verification-1",
        "action": "local_review",
        "target_ref": TARGET_REF,
        "idempotency_key": key,
    })
}

fn request_approval(fixture: &Fixture, key: &str) -> Value {
    fixture
        .api
        .dispatch("review.request_approval", request_params(key))
        .unwrap()
}

fn evidence(fixture: &Fixture) -> Value {
    fixture
        .api
        .dispatch("review.evidence", json!({"task_id": "task-1"}))
        .unwrap()
}

fn assert_path_free(fixture: &Fixture, value: &Value) {
    let text = value.to_string();
    for path in [&fixture.repository_path, &fixture.worktree_path] {
        let raw = path.to_string_lossy().into_owned();
        assert!(!text.contains(&raw), "leaked {raw}");
        assert!(!text.contains(&raw.replace('\\', "\\\\")), "leaked {raw}");
    }
}

fn valid_proposal(project_id: &str) -> String {
    json!({
        "objective": "Ship the feature",
        "tasks": [
            {"title": "First", "contract": contract("a", project_id, DeliveryPolicy::LocalReview)},
            {"title": "Second", "contract": {
                "id": "b", "project_id": project_id, "parent_id": null,
                "objective": "Second step", "kind": "Implement", "agent": "codex",
                "dependencies": ["a"], "verification": "None", "delivery": "LocalReview",
                "autonomy": "WorkspaceWrites", "max_retries": 0, "max_repairs": 0
            }}
        ]
    })
    .to_string()
}

fn propose(api: &DaemonApi, key: &str, proposal: &str) -> Value {
    api.dispatch(
        "coordinator.propose",
        json!({"project_id": "project-1", "proposal": proposal, "idempotency_key": key}),
    )
    .unwrap()
}

#[test]
fn new_methods_reject_malformed_parameters() {
    let fixture = fixture();
    let invalid = [
        (
            "coordinator.send_message",
            json!({"project_id": "project-1"}),
        ),
        (
            "coordinator.send_message",
            json!({"project_id": "project-1", "text": "hi", "idempotency_key": "k", "author": "coordinator"}),
        ),
        (
            "coordinator.send_message",
            json!({"project_id": "project-1", "text": "  ", "idempotency_key": "k"}),
        ),
        (
            "coordinator.propose",
            json!({"project_id": "project-1", "proposal": 4}),
        ),
        (
            "coordinator.propose",
            json!({"project_id": "project-1", "proposal": "{}", "idempotency_key": "k", "approve": true}),
        ),
        (
            "coordinator.get",
            json!({"project_id": "project-1", "after_sequence": -1}),
        ),
        ("coordinator.get", json!({"extra": true})),
        ("review.request_approval", json!({"task_id": "task-1"})),
        (
            "review.request_approval",
            json!({"task_id": "task-1", "verification_run_id": "verification-1", "action": "local_review", "idempotency_key": "k"}),
        ),
        ("review.evidence", json!({"task_id": 4})),
        (
            "review.evidence",
            json!({"task_id": "task-1", "path": "../x"}),
        ),
        (
            "review.evidence",
            json!({"task_id": "task-1", "path": "/abs"}),
        ),
        (
            "review.evidence",
            json!({"task_id": "task-1", "path": "-x"}),
        ),
        (
            "review.evidence",
            json!({"task_id": "task-1", "actor": "agent"}),
        ),
        (
            "review.request_changes",
            json!({"approval_id": "approval-1"}),
        ),
        (
            "review.request_changes",
            json!({"approval_id": "a", "candidate_commit": "short", "feedback": "x", "idempotency_key": "k"}),
        ),
        (
            "review.request_changes",
            json!({"approval_id": "a", "candidate_commit": "a".repeat(40), "feedback": "x", "idempotency_key": "k", "actor": "agent"}),
        ),
    ];
    for (method, params) in invalid {
        assert_eq!(
            fixture.api.dispatch(method, params.clone()),
            Err(RpcError::InvalidParams),
            "{method} {params}"
        );
    }
}

#[test]
fn capabilities_include_coordinator_and_review() {
    let fixture = fixture();
    let capabilities = fixture.api.capabilities();
    assert!(capabilities.contains(&orchestrator_ipc::Capability::Coordinator));
    assert!(capabilities.contains(&orchestrator_ipc::Capability::Review));
}

#[test]
fn coordinator_messages_are_idempotent_ordered_and_user_authored() {
    let fixture = fixture();
    let params =
        json!({"project_id": "project-1", "text": "Plan the work", "idempotency_key": "m-1"});
    let first = fixture
        .api
        .dispatch("coordinator.send_message", params.clone())
        .unwrap();
    assert_eq!(first["kind"], "message");
    assert_eq!(first["author"], "user");
    assert!(first["validation"].is_null());
    assert_eq!(
        fixture
            .api
            .dispatch("coordinator.send_message", params)
            .unwrap(),
        first
    );
    assert_eq!(
        fixture.api.dispatch(
            "coordinator.send_message",
            json!({"project_id": "project-1", "text": "different", "idempotency_key": "m-1"})
        ),
        Err(RpcError::IdempotencyConflict)
    );
    fixture
        .api
        .dispatch(
            "coordinator.send_message",
            json!({"project_id": "project-1", "text": "Second", "idempotency_key": "m-2"}),
        )
        .unwrap();

    let all = fixture
        .api
        .dispatch("coordinator.get", json!({"project_id": "project-1"}))
        .unwrap();
    let entries = all["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["body"], "Plan the work");
    assert_eq!(entries[1]["body"], "Second");
    assert!(entries[0]["sequence"].as_i64() < entries[1]["sequence"].as_i64());
    assert_eq!(all["more"], false);

    let cursor = entries[0]["sequence"].as_i64().unwrap();
    let page = fixture
        .api
        .dispatch(
            "coordinator.get",
            json!({"project_id": "project-1", "after_sequence": cursor}),
        )
        .unwrap();
    assert_eq!(page["entries"].as_array().unwrap().len(), 1);
    assert_eq!(page["entries"][0]["body"], "Second");
    assert_eq!(page["next_after_sequence"], entries[1]["sequence"]);
}

#[test]
fn coordinator_enforces_project_existence_and_size_limits() {
    let fixture = fixture();
    assert_eq!(
        fixture.api.dispatch(
            "coordinator.send_message",
            json!({"project_id": "missing", "text": "hi", "idempotency_key": "m-1"})
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        fixture
            .api
            .dispatch("coordinator.get", json!({"project_id": "missing"})),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        fixture.api.dispatch(
            "coordinator.send_message",
            json!({"project_id": "project-1", "text": "x".repeat(8 * 1024 + 1), "idempotency_key": "m-2"})
        ),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        fixture.api.dispatch(
            "coordinator.propose",
            json!({"project_id": "project-1", "proposal": "x".repeat(64 * 1024 + 1), "idempotency_key": "p-1"})
        ),
        Err(RpcError::InvalidParams)
    );
    let at_limit = fixture
        .api
        .dispatch(
            "coordinator.send_message",
            json!({"project_id": "project-1", "text": "x".repeat(8 * 1024), "idempotency_key": "m-3"}),
        )
        .unwrap();
    assert_eq!(at_limit["body"].as_str().unwrap().len(), 8 * 1024);
}

#[test]
fn coordinator_get_pages_at_one_hundred_entries() {
    let fixture = fixture();
    for index in 0..101 {
        fixture
            .api
            .dispatch(
                "coordinator.send_message",
                json!({"project_id": "project-1", "text": format!("m{index}"), "idempotency_key": format!("m-{index}")}),
            )
            .unwrap();
    }
    let first = fixture
        .api
        .dispatch("coordinator.get", json!({"project_id": "project-1"}))
        .unwrap();
    assert_eq!(first["entries"].as_array().unwrap().len(), 100);
    assert_eq!(first["more"], true);
    let second = fixture
        .api
        .dispatch(
            "coordinator.get",
            json!({"project_id": "project-1", "after_sequence": first["next_after_sequence"]}),
        )
        .unwrap();
    assert_eq!(second["entries"].as_array().unwrap().len(), 1);
    assert_eq!(second["more"], false);
    assert_eq!(second["entries"][0]["body"], "m100");
}

#[test]
fn valid_proposal_is_validated_without_authority_or_side_effects() {
    let fixture = fixture();
    let tasks_before = fixture.api.store.list().unwrap().len();
    let approvals_before = fixture
        .api
        .store
        .approval_records_for_task("task-1")
        .unwrap()
        .len();
    let entry = propose(&fixture.api, "p-1", &valid_proposal("project-1"));
    assert_eq!(entry["kind"], "proposal");
    assert_eq!(entry["author"], "coordinator");
    let validation = &entry["validation"];
    assert_eq!(validation["status"], "valid");
    assert_eq!(validation["authority"], "none");
    assert_eq!(validation["objective"], "Ship the feature");
    let tasks = validation["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["title"], "First");
    assert_eq!(
        tasks[1]["dependencies"],
        json!([tasks[0]["id"].as_str().unwrap()])
    );
    assert_ne!(tasks[0]["id"], "a");

    assert_eq!(fixture.api.store.list().unwrap().len(), tasks_before);
    assert_eq!(
        fixture
            .api
            .store
            .approval_records_for_task("task-1")
            .unwrap()
            .len(),
        approvals_before
    );
    assert_eq!(
        propose(&fixture.api, "p-1", &valid_proposal("project-1")),
        entry
    );
}

#[test]
fn invalid_proposals_report_stable_codes() {
    let fixture = fixture();
    let code = |proposal: String, key: &str| -> Vec<String> {
        let entry = propose(&fixture.api, key, &proposal);
        let validation = &entry["validation"];
        assert_eq!(validation["status"], "invalid", "{proposal}");
        assert_eq!(validation["authority"], "none");
        validation["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|error| error["code"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(code("not json".into(), "p-1"), ["invalid_json"]);
    assert_eq!(
        code(json!({"objective": "o", "tasks": []}).to_string(), "p-2"),
        ["empty_tasks"]
    );
    let task = |id: &str, dependencies: Value| {
        json!({"title": id, "contract": {
            "id": id, "project_id": "project-1", "parent_id": null, "objective": id,
            "kind": "Implement", "agent": "codex", "dependencies": dependencies,
            "verification": "None", "delivery": "LocalReview",
            "autonomy": "WorkspaceWrites", "max_retries": 0, "max_repairs": 0
        }})
    };
    assert_eq!(
        code(
            json!({"objective": "o", "tasks": [task("a", json!([])), task("a", json!([]))]})
                .to_string(),
            "p-3"
        ),
        ["duplicate_key"]
    );
    assert_eq!(
        code(
            json!({"objective": "o", "tasks": [task("a", json!(["zzz"]))]}).to_string(),
            "p-4"
        ),
        ["unknown_dependency"]
    );
    assert_eq!(
        code(
            json!({"objective": "o", "tasks": [task("a", json!(["b"])), task("b", json!(["a"]))]})
                .to_string(),
            "p-5"
        ),
        ["dependency_cycle"]
    );
    assert_eq!(
        code(valid_proposal("other-project"), "p-6"),
        ["project_mismatch", "project_mismatch"]
    );
}

#[test]
fn proposals_cannot_smuggle_authority_fields() {
    let fixture = fixture();
    for (key, mutate) in [
        ("p-1", json!({"approve": true})),
        ("p-2", json!({"authority": "full"})),
        ("p-3", json!({"policy": {"delivery": "Auto"}})),
    ] {
        let mut proposal: Value = serde_json::from_str(&valid_proposal("project-1")).unwrap();
        proposal
            .as_object_mut()
            .unwrap()
            .extend(mutate.as_object().unwrap().clone());
        let entry = propose(&fixture.api, key, &proposal.to_string());
        assert_eq!(entry["validation"]["status"], "invalid");
        assert_eq!(entry["validation"]["authority"], "none");
    }
    let mut proposal: Value = serde_json::from_str(&valid_proposal("project-1")).unwrap();
    proposal["tasks"][0]["contract"]["approval"] = json!("granted");
    let entry = propose(&fixture.api, "p-4", &proposal.to_string());
    assert_eq!(entry["validation"]["status"], "invalid");
    assert_eq!(fixture.api.store.list().unwrap().len(), 1);
}

#[test]
fn request_approval_binds_the_delivery_digest_and_is_replayable() {
    let fixture = fixture();
    let first = request_approval(&fixture, "ra-1");
    assert_eq!(first["existing"], false);
    assert_eq!(first["approval"]["state"], "requested");
    assert_eq!(first["approval"]["kind"], "local_review_merge");
    assert_eq!(
        first["approval"]["candidate_commit"],
        fixture.candidate_sha.as_str()
    );
    assert_eq!(first["action"]["base_sha"], fixture.base_sha.as_str());
    assert_eq!(first["action"]["target_ref"], TARGET_REF);

    let destination = Repository::new(&fixture.repository_path)
        .unwrap()
        .path()
        .to_string_lossy()
        .into_owned();
    let expected = delivery_digest(
        DeliveryAction::LocalReview,
        &destination,
        &fixture.base_sha,
        Some(TARGET_REF),
    )
    .unwrap();
    assert_eq!(first["digest"], review::hex(&expected));
    assert_path_free(&fixture, &first);

    assert_eq!(request_approval(&fixture, "ra-1"), first);
    let second = request_approval(&fixture, "ra-2");
    assert_eq!(second["existing"], true);
    assert_eq!(second["approval"]["id"], first["approval"]["id"]);
    assert_eq!(
        fixture
            .api
            .store
            .approval_records_for_task("task-1")
            .unwrap()
            .len(),
        1
    );
    let stored = fixture
        .api
        .store
        .approval_parameters(first["approval"]["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored["target_ref"], TARGET_REF);
    assert_eq!(stored["base_sha"], fixture.base_sha.as_str());
}

#[test]
fn request_approval_refuses_unsafe_requests() {
    let fixture = fixture();
    let mut wrong_action = request_params("ra-1");
    wrong_action["action"] = json!("report_only");
    wrong_action["target_ref"] = Value::Null;
    assert_eq!(
        fixture
            .api
            .dispatch("review.request_approval", wrong_action),
        Err(RpcError::Forbidden)
    );
    let mut no_ref = request_params("ra-2");
    no_ref.as_object_mut().unwrap().remove("target_ref");
    assert_eq!(
        fixture.api.dispatch("review.request_approval", no_ref),
        Err(RpcError::InvalidParams)
    );
    let mut missing_run = request_params("ra-3");
    missing_run["verification_run_id"] = json!("nope");
    assert_eq!(
        fixture.api.dispatch("review.request_approval", missing_run),
        Err(RpcError::NotFound)
    );
    assert!(fixture
        .api
        .store
        .approval_records_for_task("task-1")
        .unwrap()
        .is_empty());
}

#[test]
fn request_approval_requires_passed_verification_of_the_current_head() {
    let fixture = fixture();
    fixture
        .api
        .store
        .add_verification_run(&VerificationRun {
            id: "verification-failed".into(),
            task_id: "task-1".into(),
            commit: fixture.candidate_sha.clone(),
            status: "failed".into(),
            log_path: None,
        })
        .unwrap();
    let mut failed = request_params("ra-1");
    failed["verification_run_id"] = json!("verification-failed");
    assert_eq!(
        fixture.api.dispatch("review.request_approval", failed),
        Err(RpcError::Conflict)
    );

    commit_file(&fixture.worktree_path, "later.txt", "later\n");
    assert_eq!(
        fixture
            .api
            .dispatch("review.request_approval", request_params("ra-2")),
        Err(RpcError::Conflict)
    );
    assert!(fixture
        .api
        .store
        .approval_records_for_task("task-1")
        .unwrap()
        .is_empty());
}

#[test]
fn evidence_without_an_approval_is_missing_and_pinned_to_head() {
    let fixture = fixture();
    let result = evidence(&fixture);
    assert_eq!(result["state"], "missing");
    assert_eq!(result["reasons"], json!(["no_approval"]));
    assert_eq!(result["review_state"], "not_requested");
    assert_eq!(result["decision_ready"], false);
    assert_eq!(result["candidate"]["source"], "workspace_head");
    assert_eq!(
        result["candidate"]["commit"],
        fixture.candidate_sha.as_str()
    );
    assert_eq!(result["snapshot"]["files"], json!(["candidate.txt"]));
    assert_eq!(result["snapshot"]["diff_state"], "complete");
    assert!(result["snapshot"]["diff"]
        .as_str()
        .unwrap()
        .contains("+candidate"));
    assert_eq!(result["verification"][0]["matches_candidate"], true);
    assert_path_free(&fixture, &result);
}

#[test]
fn evidence_is_current_for_a_fresh_approval_and_stale_after_the_workspace_moves() {
    let fixture = fixture();
    let approval = request_approval(&fixture, "ra-1");
    let current = evidence(&fixture);
    assert_eq!(current["state"], "current");
    assert_eq!(current["reasons"], json!([]));
    assert_eq!(current["review_state"], "awaiting_decision");
    assert_eq!(current["decision_ready"], true);
    assert_eq!(current["digest_relationship"], "matches");
    assert_eq!(current["candidate"]["source"], "approval");
    assert_eq!(current["approval"]["id"], approval["approval"]["id"]);
    assert_eq!(current["action"]["target_ref"], TARGET_REF);
    assert_path_free(&fixture, &current);

    commit_file(&fixture.worktree_path, "later.txt", "later\n");
    let stale = evidence(&fixture);
    assert_eq!(stale["state"], "stale");
    assert_eq!(stale["decision_ready"], false);
    let reasons = stale["reasons"].as_array().unwrap();
    assert!(reasons.contains(&json!("candidate_not_head")));
    assert_eq!(
        stale["candidate"]["commit"],
        fixture.candidate_sha.as_str(),
        "the snapshot stays pinned to the approved candidate"
    );
    assert_ne!(
        stale["candidate"]["workspace_head"],
        stale["candidate"]["commit"]
    );
    assert_eq!(stale["snapshot"]["files"], json!(["candidate.txt"]));
    assert!(!stale["snapshot"]["diff"]
        .as_str()
        .unwrap()
        .contains("later.txt"));
}

#[test]
fn evidence_marks_an_unverified_unknown_candidate_stale() {
    use orchestrator_core::approval::{
        Actor, ApprovalAction, ApprovalActionKind, ApproverAllowlist,
    };

    let fixture = fixture();
    let unknown = "b".repeat(40);
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256([7; 32]),
        &unknown,
        "task-1",
    )
    .unwrap();
    let allowlist = ApproverAllowlist::new("local-user", Vec::<String>::new()).unwrap();
    fixture
        .api
        .store
        .request_approval(
            "approval-x",
            &action,
            &Actor::Agent("agent-1".into()),
            &allowlist,
        )
        .unwrap();
    let result = evidence(&fixture);
    assert_eq!(result["state"], "stale");
    assert_eq!(result["digest_relationship"], "unavailable");
    assert_eq!(result["decision_ready"], false);
    let reasons = result["reasons"].as_array().unwrap();
    for reason in [
        "candidate_not_head",
        "candidate_unavailable",
        "verification_commit_mismatch",
    ] {
        assert!(reasons.contains(&json!(reason)), "{reason} in {reasons:?}");
    }
    assert_eq!(result["snapshot"]["diff_state"], "unavailable");
    assert!(result["snapshot"]["diff"].is_null());
}

#[test]
fn evidence_snapshot_is_size_bounded_and_path_filterable() {
    let fixture = fixture();
    commit_file(
        &fixture.worktree_path,
        "big.txt",
        &"0123456789abcdef\n".repeat(40_000),
    );
    let big = evidence(&fixture);
    assert_eq!(big["snapshot"]["diff_state"], "too_large");
    assert!(big["snapshot"]["diff"].is_null());
    assert!(big["snapshot"]["diff_bytes"].as_u64().unwrap() > 512 * 1024);
    assert_eq!(
        big["snapshot"]["files"],
        json!(["big.txt", "candidate.txt"])
    );

    let filtered = fixture
        .api
        .dispatch(
            "review.evidence",
            json!({"task_id": "task-1", "path": "candidate.txt"}),
        )
        .unwrap();
    assert_eq!(filtered["snapshot"]["diff_state"], "complete");
    let diff = filtered["snapshot"]["diff"].as_str().unwrap();
    assert!(diff.contains("+candidate") && !diff.contains("0123456789"));
}

#[test]
fn evidence_for_unknown_task_or_approval_is_not_found() {
    let fixture = fixture();
    assert_eq!(
        fixture
            .api
            .dispatch("review.evidence", json!({"task_id": "missing"})),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        fixture.api.dispatch(
            "review.evidence",
            json!({"task_id": "task-1", "approval_id": "missing"})
        ),
        Err(RpcError::NotFound)
    );
}

fn changes_params(approval: &Value, candidate: &str, key: &str) -> Value {
    json!({
        "approval_id": approval["approval"]["id"],
        "candidate_commit": candidate,
        "feedback": "Please rename the file",
        "idempotency_key": key,
    })
}

#[test]
fn request_changes_blocks_approval_but_not_rejection() {
    let fixture = fixture();
    let approval = request_approval(&fixture, "ra-1");
    let approval_id = approval["approval"]["id"].as_str().unwrap().to_owned();
    let params = changes_params(&approval, &fixture.candidate_sha, "rc-1");
    let result = fixture
        .api
        .dispatch("review.request_changes", params.clone())
        .unwrap();
    assert_eq!(result["review_state"], "changes_requested");
    assert_eq!(result["approval_state"], "requested");
    assert_eq!(
        result["change_request"]["feedback"],
        "Please rename the file"
    );
    assert_eq!(
        result["change_request"]["candidate_commit"],
        fixture.candidate_sha.as_str()
    );
    assert_eq!(
        fixture
            .api
            .dispatch("review.request_changes", params)
            .unwrap(),
        result
    );

    let after = evidence(&fixture);
    assert_eq!(after["review_state"], "changes_requested");
    assert_eq!(after["decision_ready"], false);
    assert_eq!(
        after["change_request"]["feedback"],
        "Please rename the file"
    );

    assert_eq!(
        fixture.api.dispatch(
            "approvals.resolve",
            json!({"approval_id": approval_id, "decision": "approve", "idempotency_key": "ap-1"})
        ),
        Err(RpcError::Conflict)
    );
    assert_eq!(
        fixture
            .api
            .store
            .approval_records_for_task("task-1")
            .unwrap()[0]
            .state,
        orchestrator_core::approval::ApprovalState::Requested
    );
    let events = fixture.api.store.stream_events_after(0, 100).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "review.changes_requested"));
    assert_path_free(
        &fixture,
        &Value::String(
            events
                .iter()
                .map(|event| event.payload.to_string())
                .collect::<String>(),
        ),
    );

    let rejected = fixture
        .api
        .dispatch(
            "approvals.resolve",
            json!({"approval_id": approval_id, "decision": "reject", "idempotency_key": "rj-1"}),
        )
        .unwrap();
    assert_eq!(rejected["state"]["state"], "rejected");
}

#[test]
fn request_changes_refuses_stale_duplicate_and_unknown_requests() {
    let fixture = fixture();
    let approval = request_approval(&fixture, "ra-1");
    assert_eq!(
        fixture.api.dispatch(
            "review.request_changes",
            changes_params(&approval, &"c".repeat(40), "rc-1")
        ),
        Err(RpcError::Conflict)
    );
    assert_eq!(
        fixture.api.dispatch(
            "review.request_changes",
            json!({"approval_id": "missing", "candidate_commit": fixture.candidate_sha, "feedback": "x", "idempotency_key": "rc-2"})
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        fixture.api.dispatch(
            "review.request_changes",
            json!({"approval_id": approval["approval"]["id"], "candidate_commit": fixture.candidate_sha, "feedback": "x".repeat(8 * 1024 + 1), "idempotency_key": "rc-3"})
        ),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        fixture.api.dispatch(
            "review.request_changes",
            json!({"approval_id": approval["approval"]["id"], "candidate_commit": fixture.candidate_sha, "feedback": "  ", "idempotency_key": "rc-4"})
        ),
        Err(RpcError::InvalidParams)
    );
    fixture
        .api
        .dispatch(
            "review.request_changes",
            changes_params(&approval, &fixture.candidate_sha, "rc-5"),
        )
        .unwrap();
    assert_eq!(
        fixture.api.dispatch(
            "review.request_changes",
            changes_params(&approval, &fixture.candidate_sha, "rc-6")
        ),
        Err(RpcError::Conflict)
    );
}

#[test]
fn request_changes_refuses_a_decided_approval() {
    let fixture = fixture();
    let approval = request_approval(&fixture, "ra-1");
    fixture
        .api
        .dispatch(
            "approvals.resolve",
            json!({"approval_id": approval["approval"]["id"], "decision": "approve", "idempotency_key": "ap-1"}),
        )
        .unwrap();
    assert_eq!(
        fixture.api.dispatch(
            "review.request_changes",
            changes_params(&approval, &fixture.candidate_sha, "rc-1")
        ),
        Err(RpcError::Conflict)
    );
    let after = evidence(&fixture);
    assert_eq!(after["review_state"], "approved");
}

#[test]
fn approval_from_review_request_delivers_end_to_end() {
    let fixture = fixture();
    let approval = request_approval(&fixture, "ra-1");
    fixture
        .api
        .dispatch(
            "approvals.resolve",
            json!({"approval_id": approval["approval"]["id"], "decision": "approve", "idempotency_key": "ap-1"}),
        )
        .unwrap();
    let delivered = fixture
        .api
        .dispatch(
            "delivery.request",
            json!({
                "task_id": "task-1",
                "workspace_id": "workspace-1",
                "approval_id": approval["approval"]["id"],
                "verification_run_id": "verification-1",
                "action": "local_review",
                "target_ref": TARGET_REF,
                "idempotency_key": "d-1"
            }),
        )
        .unwrap();
    assert_eq!(delivered["outcome"], "review_ref_created");
    assert_eq!(
        git(&fixture.repository_path, &["rev-parse", TARGET_REF]),
        fixture.candidate_sha
    );
}

#[test]
fn evidence_marks_a_digest_that_no_longer_matches_its_parameters_stale() {
    use orchestrator_core::approval::{
        Actor, ApprovalAction, ApprovalActionKind, ApproverAllowlist,
    };

    let fixture = fixture();
    let honest = request_approval(&fixture, "ra-1");
    let parameters = fixture
        .api
        .store
        .approval_parameters(honest["approval"]["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let forged = ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256([9; 32]),
        &fixture.candidate_sha,
        "task-1",
    )
    .unwrap();
    let allowlist = ApproverAllowlist::new("local-user", Vec::<String>::new()).unwrap();
    fixture
        .api
        .store
        .request_approval_with_parameters(
            "approval-forged",
            &forged,
            &Actor::Agent("agent-1".into()),
            &allowlist,
            &parameters,
        )
        .unwrap();

    let result = evidence(&fixture);
    assert_eq!(result["approval"]["id"], "approval-forged");
    assert_eq!(result["digest_relationship"], "mismatch");
    assert_eq!(result["state"], "stale");
    assert_eq!(result["decision_ready"], false);
    assert_eq!(
        result["reasons"],
        json!(["digest_mismatch"]),
        "the candidate, verification and workspace are all current"
    );
}

#[test]
fn evidence_snapshot_lists_at_most_one_thousand_files() {
    let fixture = fixture();
    let write_files = |range: std::ops::Range<u32>| {
        for index in range {
            fs::write(
                fixture.worktree_path.join(format!("many-{index:04}.txt")),
                format!("{index}\n"),
            )
            .unwrap();
        }
        // Stage only the generated files so no other work-tree difference can
        // join the commit.
        git(&fixture.worktree_path, &["add", "--", "many-*.txt"]);
        git(&fixture.worktree_path, &["commit", "-m", "many files"]);
    };
    // Counts what the fixture really committed, independently of the code
    // under test, and names any path the fixture did not intend to commit.
    let assert_committed = |expected: usize| {
        let names = git(
            &fixture.worktree_path,
            &[
                "diff",
                "--name-only",
                "--no-renames",
                &fixture.base_sha,
                "HEAD",
            ],
        );
        let stray: Vec<&str> = names
            .lines()
            .filter(|name| *name != "candidate.txt" && !name.starts_with("many-"))
            .collect();
        assert!(
            stray.is_empty(),
            "fixture committed unexpected paths: {stray:?}"
        );
        assert_eq!(
            names.lines().count(),
            expected,
            "fixture committed path count"
        );
    };
    let summary = |snapshot: &Value| {
        format!(
            "files.len()={} files_truncated={}",
            snapshot["files"].as_array().map_or(0, Vec::len),
            snapshot["files_truncated"],
        )
    };

    // candidate.txt plus 999 files is exactly the cap.
    write_files(0..999);
    assert_committed(1000);
    let at_cap = evidence(&fixture);
    let snapshot = &at_cap["snapshot"];
    assert_eq!(
        snapshot["files"].as_array().unwrap().len(),
        1000,
        "{}",
        summary(snapshot)
    );
    assert_eq!(snapshot["files_truncated"], false, "{}", summary(snapshot));
    assert_eq!(snapshot["diff_state"], "complete", "{}", summary(snapshot));

    write_files(999..1001);
    assert_committed(1002);
    let over_cap = evidence(&fixture);
    let snapshot = &over_cap["snapshot"];
    let files = snapshot["files"].as_array().unwrap();
    assert_eq!(files.len(), 1000, "{}", summary(snapshot));
    assert_eq!(snapshot["files_truncated"], true, "{}", summary(snapshot));
    assert_eq!(files[0], "candidate.txt", "Git byte order is kept");
}
