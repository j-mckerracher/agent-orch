use orchestrator_core::decomposition::{validate_proposal, DecompositionError};
use serde_json::{json, Value};

fn task(title: &str, key: &str, objective: &str, dependencies: &[&str]) -> Value {
    json!({
        "title": title,
        "contract": {
            "id": key,
            "project_id": "project",
            "parent_id": null,
            "objective": objective,
            "kind": "Implement",
            "agent": "test-agent",
            "dependencies": dependencies,
            "verification": "None",
            "delivery": "LocalReview",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0
        }
    })
}

fn proposal(tasks: Vec<Value>) -> String {
    json!({ "objective": "Build widget", "tasks": tasks }).to_string()
}

#[test]
fn assigns_canonical_ids_and_rewrites_dependencies() {
    let plan = validate_proposal(&proposal(vec![
        task("Foundation", "local-a", "Build foundation", &[]),
        task("Widget", "local-b", "Build widget", &["local-a"]),
    ]))
    .unwrap();

    assert_eq!(plan.objective(), "Build widget");
    assert_eq!(plan.tasks()[0].title(), "Foundation");
    assert_eq!(plan.tasks()[0].contract().id, "decomp-bfd4004a8291a691-1");
    assert_eq!(plan.tasks()[1].contract().id, "decomp-bfd4004a8291a691-2");
    assert_eq!(
        plan.tasks()[1].contract().dependencies,
        vec!["decomp-bfd4004a8291a691-1"]
    );
}

#[test]
fn reordered_json_keys_produce_same_plan() {
    let contract_a = r#"{"id":"local","project_id":"project","parent_id":null,"objective":"Build widget","kind":"Implement","agent":"test-agent","dependencies":[],"verification":"None","delivery":"LocalReview","autonomy":"WorkspaceWrites","max_retries":0,"max_repairs":0}"#;
    let contract_b = r#"{"max_repairs":0,"autonomy":"WorkspaceWrites","delivery":"LocalReview","verification":"None","dependencies":[],"agent":"test-agent","kind":"Implement","objective":"Build widget","parent_id":null,"project_id":"project","max_retries":0,"id":"local"}"#;
    let first = format!(
        r#"{{"objective":"Build widget","tasks":[{{"title":"Widget","contract":{contract_a}}}]}}"#
    );
    let second = format!(
        r#"{{"tasks":[{{"contract":{contract_b},"title":"Widget"}}],"objective":"Build widget"}}"#
    );

    assert_eq!(
        validate_proposal(&first).unwrap(),
        validate_proposal(&second).unwrap()
    );
}

#[test]
fn rejects_unknown_fields_at_proposal_and_contract_levels() {
    let mut value: Value = serde_json::from_str(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &[],
    )]))
    .unwrap();
    value["extra"] = json!(true);
    assert!(matches!(
        validate_proposal(&value.to_string()),
        Err(DecompositionError::Json(_))
    ));

    let mut value: Value = serde_json::from_str(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &[],
    )]))
    .unwrap();
    value["tasks"][0]["extra"] = json!(true);
    assert!(matches!(
        validate_proposal(&value.to_string()),
        Err(DecompositionError::Json(_))
    ));
    let mut value: Value = serde_json::from_str(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &[],
    )]))
    .unwrap();
    value["tasks"][0]["contract"]["extra"] = json!(true);
    assert!(matches!(
        validate_proposal(&value.to_string()),
        Err(DecompositionError::Json(_))
    ));
    let mut value: Value = serde_json::from_str(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &[],
    )]))
    .unwrap();
    value["tasks"][0]["contract"]["verification"] =
        json!({"Commands": {"commands": [["cargo", "test"]], "extra": true}});
    assert!(matches!(
        validate_proposal(&value.to_string()),
        Err(DecompositionError::Json(_))
    ));
}

#[test]
fn rejects_missing_required_task_title() {
    let mut value: Value = serde_json::from_str(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &[],
    )]))
    .unwrap();
    value["tasks"][0].as_object_mut().unwrap().remove("title");

    assert!(matches!(
        validate_proposal(&value.to_string()),
        Err(DecompositionError::Json(_))
    ));
}

#[test]
fn rejects_wrong_contract_field_type() {
    let mut value: Value = serde_json::from_str(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &[],
    )]))
    .unwrap();
    value["tasks"][0]["contract"]["max_retries"] = json!("never");

    assert!(matches!(
        validate_proposal(&value.to_string()),
        Err(DecompositionError::Json(_))
    ));
}

#[test]
fn rejects_empty_task_list() {
    assert!(matches!(
        validate_proposal(&proposal(vec![])),
        Err(DecompositionError::EmptyTasks)
    ));
}

#[test]
fn rejects_unknown_dependency_key() {
    let error = validate_proposal(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &["missing"],
    )]))
    .unwrap_err();

    assert!(error.to_string().contains("local"));
    assert!(error.to_string().contains("missing"));
}

#[test]
fn rejects_self_dependency_with_named_path() {
    let error = validate_proposal(&proposal(vec![task(
        "Widget",
        "local",
        "Build widget",
        &["local"],
    )]))
    .unwrap_err();

    assert!(error.to_string().contains("local -> local"));
}

#[test]
fn rejects_multi_task_cycle_with_named_path() {
    let error = validate_proposal(&proposal(vec![
        task("First", "first", "First task", &["second"]),
        task("Second", "second", "Second task", &["first"]),
    ]))
    .unwrap_err();

    assert!(error.to_string().contains("first -> second -> first"));
}

#[test]
fn reports_every_task_with_an_invalid_contract() {
    let mut second = task("Second", "second", "Second task", &[]);
    second["contract"]["agent"] = json!("");
    let error =
        validate_proposal(&proposal(vec![task("First", "first", "", &[]), second])).unwrap_err();

    let DecompositionError::InvalidContracts { failures } = error else {
        panic!("expected invalid contract failures");
    };
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.key.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
}

#[test]
fn proposal_id_is_only_a_local_key() {
    let plan = validate_proposal(&proposal(vec![task(
        "Widget",
        "proposal-owned-id",
        "Build widget",
        &[],
    )]))
    .unwrap();

    assert_eq!(plan.tasks()[0].contract().id, "decomp-bfd4004a8291a691-1");
    assert_ne!(plan.tasks()[0].contract().id, "proposal-owned-id");
}

#[test]
fn rejects_duplicate_local_keys() {
    let error = validate_proposal(&proposal(vec![
        task("First", "same", "First task", &[]),
        task("Second", "same", "Second task", &[]),
    ]))
    .unwrap_err();

    assert!(matches!(error, DecompositionError::DuplicateKey(key) if key == "same"));
}
