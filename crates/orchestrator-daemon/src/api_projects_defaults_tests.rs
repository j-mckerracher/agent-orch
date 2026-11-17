//! Per-project defaults served and enforced (DPES-1105 phase 4b). Every test drives an
//! in-process `DaemonApi` over a real store. The project root the proposal tests use is a
//! path that does not exist (`ALPHA`), so no agent CLI can start: a task admitted here fails
//! while it prepares the worktree. The two tests that register a real repository never submit
//! a task.

use orchestrator_core::task_envelope::chat_task_envelope;
use orchestrator_store::{ProjectDefaultsRow, SettingsPatch};

use super::actions::project_code;
use super::actions_task_tests::{
    assert_executed, assert_failed, proposed_task_id, register_project, task_count, ALPHA,
};
use super::actions_tests::{call, new_api, propose, resolve};
use super::projects_defaults::envelope_for;
use super::*;

const OBJECTIVE: &str = "Add unit tests for the retry module.";

fn task_action(code: &str, delivery: &str) -> String {
    task_action_for(code, delivery, OBJECTIVE)
}

/// Two actions with one project, objective and agent share a supersede subject and the newer
/// replaces the older, so a block that holds both asks for different objectives.
fn task_action_for(code: &str, delivery: &str, objective: &str) -> String {
    json!({
        "kind": "task.propose",
        "project": code,
        "objective": objective,
        "agent": "codex",
        "delivery": delivery,
    })
    .to_string()
}

fn checks() -> Vec<Vec<String>> {
    vec![vec!["cargo".to_owned(), "test".to_owned()]]
}

/// Writes the defaults row of `ALPHA`, as the user's `projects.set_defaults` would.
fn set_row(api: &DaemonApi, mode: &str) {
    api.store
        .set_project_defaults(&ProjectDefaultsRow {
            project_id: ALPHA.to_owned(),
            mode: mode.to_owned(),
            base_branch: None,
            branch_prefix: None,
            checks: checks(),
            merge_method: "squash".to_owned(),
            updated_at: String::new(),
        })
        .unwrap();
}

fn chat_pr_delivery(api: &DaemonApi, on: bool) {
    api.store
        .update_orchestrator_settings(&SettingsPatch {
            chat_pr_delivery: Some(on),
            ..SettingsPatch::default()
        })
        .unwrap();
}

/// Params of `projects.set_defaults` for `project_id`, with `overrides` laid over a valid
/// `direct-pr` row.
fn set_params(project_id: &str, key: &str, overrides: Value) -> Value {
    let mut params = json!({
        "project_id": project_id,
        "mode": "direct-pr",
        "base_branch": null,
        "branch_prefix": null,
        "checks": [["cargo", "test"]],
        "merge_method": "squash",
        "idempotency_key": key,
    });
    for (name, value) in overrides.as_object().unwrap() {
        params[name] = value.clone();
    }
    params
}

/// A git repository with one commit on `main`, registered with the daemon.
fn register_real_project(api: &DaemonApi) -> (tempfile::TempDir, String) {
    let repository = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(args)
            .current_dir(repository.path())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q", "--initial-branch=main"]);
    git(&["commit", "-q", "--allow-empty", "-m", "initial"]);
    let registered = call(
        api,
        "projects.register",
        json!({"path": repository.path().to_str().unwrap()}),
    )
    .unwrap();
    let id = registered["id"].as_str().unwrap().to_owned();
    (repository, id)
}

#[test]
fn a_project_with_no_defaults_behaves_exactly_as_before() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);

    for kind in [TaskKind::Implement, TaskKind::Investigate] {
        assert_eq!(
            envelope_for(&api.store, Some(ALPHA), kind).unwrap(),
            chat_task_envelope()
        );
    }
    // The listing and the settings answer carry nothing new for this project.
    assert_eq!(
        call(&api, "projects.list", Value::Null).unwrap(),
        json!([{"id": ALPHA, "coordinator_active": true}])
    );
    let defaults = call(&api, "projects.get_defaults", json!({"project_id": ALPHA})).unwrap();
    assert_eq!(
        defaults,
        json!({"defaults": {
            "mode": "local-only",
            "base_branch": null,
            "branch_prefix": null,
            "checks": [],
            "merge_method": "squash",
        }})
    );
    assert!(
        api.store.project_defaults(ALPHA).unwrap().is_none(),
        "a read writes nothing"
    );

    let ids = propose(
        &api,
        "t1",
        &format!(
            "{}, {}",
            task_action(&code, "LocalReview"),
            task_action_for(
                &code,
                "PullRequest",
                "Open a pull request for the retry module."
            )
        ),
    );
    let approved = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&approved, &proposed_task_id(&ids[0]));
    assert_eq!(
        approved["proposal"]["target"],
        json!({"project_name": "alpha", "machine_busy": false})
    );
    let task = api.store.get(&proposed_task_id(&ids[0])).unwrap();
    assert_eq!(task.contract.verification, VerificationPolicy::None);
    // Pull request delivery is still refused, and the setting alone does not change that.
    chat_pr_delivery(&api, true);
    let denied = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_failed(&denied, "action_denied");
    assert_eq!(denied["proposal"]["denial"], "delivery_not_authorized");
    assert_eq!(task_count(&api), 1);
}

#[test]
fn a_pr_mode_without_the_chat_setting_asks_for_local_review_and_says_so() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    set_row(&api, "direct-pr");
    let code = project_code(ALPHA);

    let envelope = envelope_for(&api.store, Some(ALPHA), TaskKind::Implement).unwrap();
    assert!(envelope.delivery_grants(DeliveryPolicy::LocalReview));
    assert!(!envelope.delivery_grants(DeliveryPolicy::PullRequest));

    let ids = propose(
        &api,
        "t1",
        &format!(
            "{}, {}",
            task_action(&code, "LocalReview"),
            task_action_for(
                &code,
                "PullRequest",
                "Open a pull request for the retry module."
            )
        ),
    );
    let approved = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&approved, &proposed_task_id(&ids[0]));
    let target = &approved["proposal"]["target"];
    assert_eq!(target["below_project_default"], true, "{approved}");
    assert_eq!(target["project_mode"], "direct-pr", "{approved}");
    // The project's checks ride on the task, whatever the setting.
    let task = api.store.get(&proposed_task_id(&ids[0])).unwrap();
    assert_eq!(task.contract.delivery, DeliveryPolicy::LocalReview);
    assert_eq!(
        task.contract.verification,
        VerificationPolicy::Commands { commands: checks() }
    );
    let denied = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_failed(&denied, "action_denied");
    assert_eq!(task_count(&api), 1);
}

#[test]
fn a_pr_mode_with_the_chat_setting_allows_pull_request_delivery() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    set_row(&api, "direct-pr");
    chat_pr_delivery(&api, true);
    let code = project_code(ALPHA);

    let ids = propose(&api, "t1", &task_action(&code, "PullRequest"));
    let approved = resolve(&api, &ids[0], "approve", "k-1").unwrap();

    assert_executed(&approved, &proposed_task_id(&ids[0]));
    assert!(approved["proposal"]["target"]
        .get("below_project_default")
        .is_none());
    let task = api.store.get(&proposed_task_id(&ids[0])).unwrap();
    assert_eq!(task.contract.delivery, DeliveryPolicy::PullRequest);
    assert_eq!(
        task.contract.verification,
        VerificationPolicy::Commands { commands: checks() }
    );
    // Approval is still required: the autonomy never rises.
    assert_eq!(task.contract.autonomy, AutonomyPolicy::ApprovalRequired);
}

#[test]
fn lowering_the_setting_between_propose_and_approve_denies_the_card() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    set_row(&api, "no-mistakes");
    chat_pr_delivery(&api, true);
    let code = project_code(ALPHA);
    let ids = propose(&api, "t1", &task_action(&code, "PullRequest"));

    chat_pr_delivery(&api, false);
    let denied = resolve(&api, &ids[0], "approve", "k-1").unwrap();

    assert_failed(&denied, "action_denied");
    assert_eq!(denied["proposal"]["denial"], "delivery_not_authorized");
    assert_eq!(task_count(&api), 0, "nothing was created");
    // A denied card is never executable, even after the setting is raised again.
    chat_pr_delivery(&api, true);
    assert_eq!(
        resolve(&api, &ids[0], "approve", "k-2"),
        Err(RpcError::ProposalNotPending)
    );
    assert_eq!(task_count(&api), 0);
}

#[test]
fn lowering_the_mode_between_propose_and_approve_denies_the_card() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    set_row(&api, "direct-pr");
    chat_pr_delivery(&api, true);
    let code = project_code(ALPHA);
    let ids = propose(&api, "t1", &task_action(&code, "PullRequest"));

    api.store.reset_project_defaults(ALPHA).unwrap();
    let denied = resolve(&api, &ids[0], "approve", "k-1").unwrap();

    assert_failed(&denied, "action_denied");
    assert_eq!(task_count(&api), 0);
}

#[test]
fn a_reject_is_never_rechecked() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let ids = propose(&api, "t1", &task_action(&code, "PullRequest"));

    let rejected = resolve(&api, &ids[0], "reject", "k-1").unwrap();

    assert_eq!(rejected["proposal"]["state"], "rejected", "{rejected}");
}

#[test]
fn projects_set_defaults_refuses_no_mistakes_without_checks() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);

    let refused = call(
        &api,
        "projects.set_defaults",
        set_params(ALPHA, "s-1", json!({"mode": "no-mistakes", "checks": []})),
    );

    assert_eq!(refused, Err(RpcError::InvalidParams));
    assert!(api.store.project_defaults(ALPHA).unwrap().is_none());
}

#[test]
fn projects_set_defaults_refuses_the_orchestrator_namespace() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);

    for (index, prefix) in ["orchestrator/", "orchestrator/review/", "no-slash", "a//b/"]
        .into_iter()
        .enumerate()
    {
        let refused = call(
            &api,
            "projects.set_defaults",
            set_params(
                ALPHA,
                &format!("s-{index}"),
                json!({"branch_prefix": prefix}),
            ),
        );
        assert_eq!(refused, Err(RpcError::InvalidParams), "{prefix}");
    }
    assert!(api.store.project_defaults(ALPHA).unwrap().is_none());
}

#[test]
fn projects_set_defaults_refuses_a_base_branch_that_does_not_exist() {
    let (_directory, api) = new_api();
    let (_repository, id) = register_real_project(&api);

    for (index, branch) in ["no-such-branch", "../escape", "-option", "HEAD"]
        .into_iter()
        .enumerate()
    {
        let refused = call(
            &api,
            "projects.set_defaults",
            set_params(&id, &format!("s-{index}"), json!({"base_branch": branch})),
        );
        assert_eq!(refused, Err(RpcError::InvalidParams), "{branch}");
    }
    assert!(
        api.store.project_defaults(&id).unwrap().is_none(),
        "nothing was written"
    );
}

#[test]
fn projects_set_defaults_stores_a_valid_row_once_and_get_returns_it() {
    let (_directory, api) = new_api();
    let (_repository, id) = register_real_project(&api);
    let params = set_params(
        &id,
        "s-1",
        json!({"base_branch": "main", "branch_prefix": "team/"}),
    );

    let first = call(&api, "projects.set_defaults", params.clone()).unwrap();
    let again = call(&api, "projects.set_defaults", params).unwrap();

    assert_eq!(first, again, "a replayed key answers the stored result");
    assert_eq!(
        first,
        json!({"defaults": {
            "mode": "direct-pr",
            "base_branch": "main",
            "branch_prefix": "team/",
            "checks": [["cargo", "test"]],
            "merge_method": "squash",
        }})
    );
    let read = call(&api, "projects.get_defaults", json!({"project_id": id})).unwrap();
    assert_eq!(read, first);
    let listed = call(&api, "projects.list", Value::Null).unwrap();
    assert_eq!(listed[0]["mode"], "direct-pr");
    assert!(listed[0].get("archived").is_none());
    // An unknown project is not found, and a bad mode is refused.
    assert_eq!(
        call(
            &api,
            "projects.set_defaults",
            set_params("/no/such/project", "s-2", json!({}))
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        call(
            &api,
            "projects.set_defaults",
            set_params(&id, "s-3", json!({"mode": "yolo"}))
        ),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        call(
            &api,
            "projects.get_defaults",
            json!({"project_id": "/no/such/project"})
        ),
        Err(RpcError::NotFound)
    );
}

#[test]
fn settings_carry_chat_pr_delivery_and_it_is_off_by_default() {
    let (_directory, api) = new_api();

    let before = call(&api, "settings.get", Value::Null).unwrap();
    assert_eq!(before["chat_pr_delivery"], false);

    let set = call(
        &api,
        "settings.set",
        json!({"idempotency_key": "c-1", "chat_pr_delivery": true}),
    )
    .unwrap();
    assert_eq!(set["chat_pr_delivery"], true);
    assert!(api.store.orchestrator_settings().unwrap().chat_pr_delivery);
    assert_eq!(
        call(&api, "settings.set", json!({"idempotency_key": "c-2"})),
        Err(RpcError::InvalidParams)
    );
}
