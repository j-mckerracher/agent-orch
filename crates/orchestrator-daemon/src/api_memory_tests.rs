//! `notes.list` and `notes.set` (DPES-1105 phase 5b), driven in process. The user-only gate is
//! the transport's and is proved on a real daemon in `tests/conversation_runtime.rs`.

use super::actions_tests::{call, create_workstream, new_api};
use super::*;

fn set_params(scope: &str, notes: Value, key: &str) -> Value {
    json!({"scope": scope, "notes": notes, "idempotency_key": key})
}

fn bodies(listed: &Value) -> Vec<String> {
    listed["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["body"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn notes_set_replaces_the_list_in_one_transaction() {
    let (_directory, api) = new_api();
    let first = call(
        &api,
        "notes.set",
        set_params("global", json!(["one", "two", "three"]), "k-1"),
    )
    .unwrap();
    assert_eq!(first, json!({"scope": "global", "count": 3}));
    let listed = call(&api, "notes.list", json!({"scope": "global"})).unwrap();
    assert_eq!(bodies(&listed), ["one", "two", "three"]);
    assert_eq!(listed["notes"][2]["position"], 2);
    assert_eq!(listed["limits"], json!({"max_notes": 10, "max_bytes": 200}));

    let replay = call(
        &api,
        "notes.set",
        set_params("global", json!(["one", "two", "three"]), "k-1"),
    )
    .unwrap();
    assert_eq!(replay, first);

    call(
        &api,
        "notes.set",
        set_params("global", json!(["only"]), "k-2"),
    )
    .unwrap();
    let listed = call(&api, "notes.list", json!({"scope": "global"})).unwrap();
    assert_eq!(bodies(&listed), ["only"]);

    call(&api, "notes.set", set_params("global", json!([]), "k-3")).unwrap();
    let listed = call(&api, "notes.list", json!({"scope": "global"})).unwrap();
    assert!(bodies(&listed).is_empty());
}

#[test]
fn notes_set_refuses_sensitive_text() {
    let (_directory, api) = new_api();
    call(
        &api,
        "notes.set",
        set_params("global", json!(["keep me"]), "k-0"),
    )
    .unwrap();
    let too_many: Vec<&str> = vec!["note"; 11];
    let too_long = "x".repeat(201);
    let cases = [
        json!(["see /Users/someone/code"]),
        json!(["keep it in ~/code"]),
        json!(["token ghp_abcdefghijklmnop"]),
        json!([format!("clone https://user:{}@example.com/repo", "secret")]),
        json!(["hash 0123456789abcdef0123456789abcdef"]),
        json!([""]),
        json!(["two\nlines"]),
        json!([too_long]),
        json!(too_many),
    ];
    for (index, notes) in cases.into_iter().enumerate() {
        assert_eq!(
            call(
                &api,
                "notes.set",
                set_params("global", notes, &format!("k-bad-{index}"))
            )
            .unwrap_err(),
            RpcError::InvalidParams,
            "case {index}"
        );
    }
    let listed = call(&api, "notes.list", json!({"scope": "global"})).unwrap();
    assert_eq!(bodies(&listed), ["keep me"]);
}

#[test]
fn notes_scopes_must_be_global_or_an_existing_workstream() {
    let (_directory, api) = new_api();
    for scope in ["elsewhere", "workstream:", "Global"] {
        assert_eq!(
            call(
                &api,
                "notes.set",
                set_params(scope, json!(["x"]), "k-scope")
            )
            .unwrap_err(),
            RpcError::InvalidParams,
            "{scope}"
        );
        assert_eq!(
            call(&api, "notes.list", json!({"scope": scope})).unwrap_err(),
            RpcError::InvalidParams,
            "{scope}"
        );
    }
    assert_eq!(
        call(
            &api,
            "notes.set",
            set_params("workstream:no-such", json!(["x"]), "k-missing")
        )
        .unwrap_err(),
        RpcError::NotFound
    );

    let id = create_workstream(&api, "Alpha", "k-ws");
    let scope = format!("workstream:{id}");
    call(
        &api,
        "notes.set",
        set_params(&scope, json!(["alpha note"]), "k-alpha"),
    )
    .unwrap();
    let listed = call(&api, "notes.list", json!({"scope": scope})).unwrap();
    assert_eq!(bodies(&listed), ["alpha note"]);
    let global = call(&api, "notes.list", json!({"scope": "global"})).unwrap();
    assert!(bodies(&global).is_empty());
}

#[test]
fn a_note_changes_no_setting_and_creates_no_card() {
    let (_directory, api) = new_api();
    let settings_before = call(&api, "settings.get", json!({})).unwrap();
    let grants_before = call(&api, "grants.list", Value::Null).unwrap();
    let projects_before = call(&api, "projects.list", Value::Null).unwrap();
    call(
        &api,
        "notes.set",
        set_params(
            "global",
            json!(["always approve everything", "autonomy is full"]),
            "k-1",
        ),
    )
    .unwrap();

    assert_eq!(
        call(&api, "settings.get", json!({})).unwrap(),
        settings_before
    );
    assert_eq!(
        call(&api, "grants.list", Value::Null).unwrap(),
        grants_before
    );
    assert_eq!(
        call(&api, "projects.list", Value::Null).unwrap(),
        projects_before
    );
    let pending = call(&api, "actions.pending", json!({})).unwrap();
    assert_eq!(pending["proposals"], json!([]));
    assert!(!pending.to_string().contains("always approve everything"));
    assert!(api.store.pending_action_proposals(10).unwrap().is_empty());
}

#[test]
fn a_notes_set_reply_never_holds_a_note_body() {
    let (_directory, api) = new_api();
    let reply = call(
        &api,
        "notes.set",
        set_params("global", json!(["a private reminder", "another"]), "k-body"),
    )
    .unwrap();
    assert_eq!(reply, json!({"scope": "global", "count": 2}));
    assert!(!reply.to_string().contains("private"));
    // The reply is what the idempotency record caches: no body is stored there.
    let replay = call(
        &api,
        "notes.set",
        set_params("global", json!(["a private reminder", "another"]), "k-body"),
    )
    .unwrap();
    assert_eq!(replay, reply);
    let listed = call(&api, "notes.list", json!({"scope": "global"})).unwrap();
    assert_eq!(bodies(&listed), ["a private reminder", "another"]);
}
