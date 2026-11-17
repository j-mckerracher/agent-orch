//! The home directory (the Settings page's "Home directory"): what `settings.set` accepts,
//! the in-place project it registers, and what that project grants a task. Every test drives
//! an in-process `DaemonApi` over a real store. No test admits a task into the home project,
//! so no agent CLI starts (the executor run is in `executor_tests.rs`).

use orchestrator_core::orchestrator_action::{parse_block, BlockScope};

use super::actions::project_code;
use super::actions_task_tests::{chat_submit, register_project, submit_queued, task_count, ALPHA};
use super::actions_tests::new_api;
use super::projects_defaults::envelope_for;
use super::*;

/// A real directory and its canonical form as the daemon saves it.
fn directory() -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let canonical = home_directory::resolve(directory.path().to_str().unwrap()).unwrap();
    (directory, canonical.to_str().unwrap().to_owned())
}

fn set_home(api: &DaemonApi, key: &str, home: Value) -> Result<Value, RpcError> {
    api.dispatch(
        "settings.set",
        json!({"idempotency_key": key, "home_directory": home}),
    )
}

fn get_home(api: &DaemonApi) -> Value {
    api.dispatch("settings.get", Value::Null).unwrap()["home_directory"].clone()
}

fn saved(api: &DaemonApi) -> Option<String> {
    api.store.orchestrator_settings().unwrap().home_directory
}

#[test]
fn resolve_refuses_what_is_not_an_existing_absolute_directory() {
    let (directory, _) = directory();
    let file = directory.path().join("file.txt");
    std::fs::write(&file, "x").unwrap();
    let missing = directory.path().join("missing");
    for typed in [
        "",
        "   ",
        "relative/dir",
        "~someone",
        "~someone/Code",
        "bad\0path",
        missing.to_str().unwrap(),
        file.to_str().unwrap(),
    ] {
        assert_eq!(
            home_directory::resolve(typed),
            Err(RpcError::InvalidParams),
            "{typed:?}"
        );
    }
}

#[test]
fn a_leading_tilde_is_the_users_home() {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty());
    let Some(Ok(expected)) = home.map(|home| home_directory::resolve(home.to_str().unwrap()))
    else {
        return;
    };
    assert_eq!(home_directory::resolve("~").unwrap(), expected);
    assert_eq!(home_directory::resolve(" ~/ ").unwrap(), expected);
}

#[test]
fn setting_the_home_directory_registers_it_as_an_in_place_project_and_get_returns_the_path() {
    let (_data, api) = new_api();
    let (_home, path) = directory();
    assert_eq!(get_home(&api), Value::Null);

    let set = set_home(&api, "home-1", json!(path)).unwrap();

    assert_eq!(set["home_directory"], json!(path));
    assert_eq!(get_home(&api), json!(path));
    assert_eq!(saved(&api), Some(path.clone()));
    let project = api.store.get_project(&path).unwrap();
    assert_eq!(project.root_path, path);
    assert!(project.is_in_place());
    // The list says so, and no other project is marked: the app limits a task's delivery from it.
    register_project(&api, ALPHA);
    let listed = api.dispatch("projects.list", Value::Null).unwrap();
    let rows = listed.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(
            row.get("in_place") == Some(&json!(true)),
            row["id"] == json!(path)
        );
    }
}

#[test]
fn clearing_the_home_directory_keeps_the_project_and_a_retry_with_the_same_key_replays() {
    let (_data, api) = new_api();
    let (_home, path) = directory();
    let first = set_home(&api, "home-2", json!(path)).unwrap();

    let cleared = set_home(&api, "home-3", Value::Null).unwrap();
    assert_eq!(cleared["home_directory"], Value::Null);
    assert_eq!(saved(&api), None);
    // Tasks already in the project keep their project.
    assert!(api.store.get_project(&path).unwrap().is_in_place());

    // The first request's key answers as it did, and writes nothing.
    assert_eq!(set_home(&api, "home-2", json!(path)).unwrap(), first);
    assert_eq!(saved(&api), None);
}

#[test]
fn a_directory_that_is_not_there_is_refused_registers_nothing_and_leaves_the_key_free() {
    let (_data, api) = new_api();
    let (home, path) = directory();
    let missing = home.path().join("missing");

    assert_eq!(
        set_home(&api, "home-4", json!(missing.to_str().unwrap())),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        set_home(&api, "home-4", json!("relative")),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(saved(&api), None);
    assert_eq!(api.store.list_projects().unwrap(), Vec::new());

    // The refused request did not claim the key.
    set_home(&api, "home-4", json!(path)).unwrap();
    assert_eq!(saved(&api), Some(path));
}

#[test]
fn a_directory_that_is_already_a_project_stays_the_project_it_is() {
    let (_data, api) = new_api();
    let (_repository, path) = directory();
    register_project(&api, &path);

    set_home(&api, "home-5", json!(path)).unwrap();

    assert_eq!(saved(&api), Some(path.clone()));
    assert!(
        !api.store.get_project(&path).unwrap().is_in_place(),
        "a Git project stays a worktree project"
    );
}

#[test]
fn a_removed_project_at_the_directory_comes_back() {
    let (_data, api) = new_api();
    let (_home, path) = directory();
    register_project(&api, &path);
    api.store.archive_project(&path).unwrap();
    assert!(api.store.project_is_archived(&path).unwrap());

    set_home(&api, "home-6", json!(path)).unwrap();

    assert!(!api.store.project_is_archived(&path).unwrap());
}

#[test]
fn an_in_place_project_takes_only_report_only_delivery() {
    let (_data, api) = new_api();
    let (_home, path) = directory();
    set_home(&api, "home-7", json!(path)).unwrap();

    // What the user's own submit asks for is refused, and nothing is queued.
    let mut request = chat_submit("in-place-submit", "Tidy the notes.");
    request.contract.project_id = path.clone();
    assert_eq!(
        api.dispatch("tasks.submit", serde_json::to_value(&request).unwrap()),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(task_count(&api), 0);

    // The envelope the reply sink checks a model's proposal against grants nothing else.
    let envelope = envelope_for(&api.store, Some(&path), TaskKind::Implement).unwrap();
    assert!(envelope.delivery_grants(DeliveryPolicy::ReportOnly));
    assert!(!envelope.delivery_grants(DeliveryPolicy::LocalReview));
    assert!(!envelope.delivery_grants(DeliveryPolicy::PullRequest));
}

#[test]
fn a_proposal_for_the_home_project_is_stored_with_report_only_delivery() {
    let (_data, api) = new_api();
    let (_home, path) = directory();
    let (_other, other) = directory();
    set_home(&api, "home-8", json!(path)).unwrap();
    register_project(&api, &other);
    let block = |project: &str| {
        let held = format!(
            "```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{{\"kind\": \"task.propose\", \"project\": \"{}\", \"objective\": \"Tidy the notes.\", \"agent\": \"codex\"}}]}}\n```",
            project_code(project)
        );
        parse_block(&held, BlockScope::Global).unwrap()
    };
    let digest = |block: orchestrator_core::orchestrator_action::ActionBlock| {
        block.actions()[0].canonical(None).json
    };

    let home = digest(home_directory::with_in_place_delivery(
        &api.store,
        block(&path),
    ));
    let other = digest(home_directory::with_in_place_delivery(
        &api.store,
        block(&other),
    ));

    assert!(home.contains(r#""delivery":"ReportOnly""#), "{home}");
    assert!(other.contains("LocalReview"), "{other}");
}

#[test]
fn a_task_that_ran_in_place_has_no_workspace_to_review_or_land() {
    let (_data, api) = new_api();
    let (_home, path) = directory();
    // The task is in a project whose root does not exist, so it fails before an agent starts.
    register_project(&api, ALPHA);
    let task_id = submit_queued(&api, "in-place-task", "Tidy the notes.");
    api.store
        .add_workspace(&Workspace::in_place(
            "in-place-workspace".to_owned(),
            task_id.clone(),
            path,
        ))
        .unwrap();

    assert_eq!(
        api.selected_workspace(&task_id, None),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        api.selected_workspace(&task_id, Some("in-place-workspace")),
        Err(RpcError::NotFound)
    );
}
