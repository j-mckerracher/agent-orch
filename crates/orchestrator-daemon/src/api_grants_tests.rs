//! `grants.create`, `grants.revoke` and `grants.list` (DPES-1105 phase 5a), driven in process.
//! The user-only gate is the transport's and is proved on a real daemon in
//! `tests/conversation_runtime.rs`.

use super::actions::project_code;
use super::actions_task_tests::{register_project, ALPHA};
use super::actions_tests::{call, create_workstream, new_api};
use super::*;

fn create_params(scope: Value, kinds: Value, key: &str) -> Value {
    json!({
        "scope": scope,
        "kinds": kinds,
        "token_budget": 5000,
        "idempotency_key": key,
    })
}

fn alpha_scope() -> Value {
    json!({"kind": "project", "id": project_code(ALPHA)})
}

#[test]
fn grants_create_refuses_bad_terms_with_a_stable_error() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let refused = |params: Value| call(&api, "grants.create", params).unwrap_err();

    for kind in [
        "task.cancel",
        "task.revise",
        "project.remove",
        "delivery.merge",
        "no.such",
        "workstream.create",
    ] {
        assert_eq!(
            refused(create_params(
                alpha_scope(),
                json!([kind]),
                &format!("k-{kind}")
            )),
            RpcError::GrantInvalid,
            "{kind}"
        );
    }
    assert_eq!(
        refused(create_params(alpha_scope(), json!([]), "k-empty")),
        RpcError::GrantInvalid
    );
    assert_eq!(
        refused(create_params(
            alpha_scope(),
            json!(["task.propose", "task.propose"]),
            "k-dup"
        )),
        RpcError::GrantInvalid
    );
    for (name, value) in [
        ("expires_in_secs", json!(59)),
        ("expires_in_secs", json!(7 * 24 * 3600 + 1)),
        ("max_uses", json!(0)),
        ("max_uses", json!(201)),
        ("token_budget", json!(0)),
        ("token_budget", json!(1_000_000_001_u64)),
    ] {
        let mut params = create_params(
            alpha_scope(),
            json!(["task.propose"]),
            &format!("k-{name}-{value}"),
        );
        params[name] = value;
        assert_eq!(refused(params), RpcError::GrantInvalid, "{name}");
    }
    // A scope kind that is neither a project nor a workstream.
    assert_eq!(
        refused(create_params(
            json!({"kind": "all", "id": "x"}),
            json!(["task.propose"]),
            "k-all"
        )),
        RpcError::GrantInvalid
    );
    // A scope that names nothing the daemon knows.
    assert_eq!(
        refused(create_params(
            json!({"kind": "project", "id": "p-00000000"}),
            json!(["task.propose"]),
            "k-project"
        )),
        RpcError::GrantNotFound
    );
    assert_eq!(
        refused(create_params(
            json!({"kind": "workstream", "id": "no-such"}),
            json!(["task.propose"]),
            "k-workstream"
        )),
        RpcError::GrantNotFound
    );
    // Unknown fields and a missing budget are malformed requests.
    let mut extra = create_params(alpha_scope(), json!(["task.propose"]), "k-extra");
    extra["all_projects"] = json!(true);
    assert_eq!(refused(extra), RpcError::InvalidParams);
    let missing =
        json!({"scope": alpha_scope(), "kinds": ["task.propose"], "idempotency_key": "k-none"});
    assert_eq!(refused(missing), RpcError::InvalidParams);
    // Nothing was written.
    assert!(api
        .store
        .standing_grants(true, crate::executor::unix_now_ms(), 10)
        .unwrap()
        .is_empty());
}

#[test]
fn grants_create_replays_by_key_and_list_shows_the_policy() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let params = create_params(alpha_scope(), json!(["task.propose", "task.retry"]), "g-1");

    let created = call(&api, "grants.create", params.clone()).unwrap();
    let replay = call(&api, "grants.create", params).unwrap();

    assert_eq!(created, replay);
    let grant = &created["grant"];
    assert_eq!(
        grant["scope"],
        json!({"kind": "project", "id": project_code(ALPHA), "label": "alpha"})
    );
    assert_eq!(grant["kinds"], json!(["task.propose", "task.retry"]));
    assert_eq!(grant["max_uses"], 20);
    assert_eq!(grant["uses_used"], 0);
    assert_eq!(grant["token_budget"], 5000);
    assert_eq!(grant["tokens_spent"], 0);
    assert_eq!(grant["status"], "active");
    assert_eq!(grant["revoked_at_ms"], Value::Null);
    assert_eq!(
        grant["expires_at_ms"].as_u64().unwrap() - grant["created_at_ms"].as_u64().unwrap(),
        8 * 3600 * 1000
    );
    let on_workstream = call(
        &api,
        "grants.create",
        create_params(
            json!({"kind": "workstream", "id": workstream}),
            json!(["objective.add"]),
            "g-2",
        ),
    )
    .unwrap();
    assert_eq!(on_workstream["grant"]["scope"]["label"], "Ship M7");

    let listed = call(&api, "grants.list", json!({})).unwrap();
    assert_eq!(listed["grants"].as_array().unwrap().len(), 2, "{listed}");
    assert_eq!(
        listed["grants"][0]["id"], on_workstream["grant"]["id"],
        "newest first"
    );
    assert_eq!(listed["more"], false);
    let policy = &listed["policy"];
    assert_eq!(
        policy["grantable_kinds"],
        json!([
            "workstream.update",
            "objective.add",
            "objective.revise",
            "task.propose",
            "task.retry",
            "workstream.link_task"
        ])
    );
    assert_eq!(policy["default_expires_in_secs"], 28800);
    assert_eq!(policy["max_expires_in_secs"], 604800);
    assert_eq!(policy["default_max_uses"], 20);
    assert_eq!(policy["max_max_uses"], 200);
    assert_eq!(policy["merge_kind_available"], true);
    assert_eq!(policy["merge_default_max_uses"], 5);
    assert_eq!(policy["merge_max_max_uses"], 20);
    assert_eq!(policy["merge_max_expires_in_secs"], 86400);
    assert_eq!(
        policy["never_grantable_kinds"],
        json!(["project.remove", "task.cancel", "task.revise"])
    );
    let one = call(&api, "grants.list", json!({"limit": 1})).unwrap();
    assert_eq!(one["grants"].as_array().unwrap().len(), 1);
    assert_eq!(one["more"], true);
    assert!(!listed.to_string().contains("canary-root"), "{listed}");
}

fn merge_params(key: &str) -> Value {
    let mut params = create_params(alpha_scope(), json!(["delivery.merge"]), key);
    params["confirm_unattended_merge"] = json!(true);
    params["branch_prefix"] = json!("moa/");
    params
}

#[test]
fn merge_grant_without_a_branch_prefix_is_refused() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let mut absent = merge_params("m-no-prefix");
    absent.as_object_mut().unwrap().remove("branch_prefix");
    assert_eq!(
        call(&api, "grants.create", absent).unwrap_err(),
        RpcError::GrantInvalid
    );
    let mut empty = merge_params("m-empty-prefix");
    empty["branch_prefix"] = json!("");
    assert_eq!(
        call(&api, "grants.create", empty).unwrap_err(),
        RpcError::GrantInvalid
    );
    // Nothing was written.
    let listed = call(&api, "grants.list", json!({})).unwrap();
    assert_eq!(listed["grants"], json!([]), "{listed}");
}

#[test]
fn grants_create_for_a_merge_needs_the_confirmation_one_project_and_no_other_kind() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let refused = |params: Value| call(&api, "grants.create", params).unwrap_err();
    let changed = |name: &str, value: Value, key: &str| {
        let mut params = merge_params(key);
        params[name] = value;
        params
    };

    // Without the confirmation, or with it false: refused, and nothing is written.
    let mut missing = merge_params("m-missing");
    missing
        .as_object_mut()
        .unwrap()
        .remove("confirm_unattended_merge");
    assert_eq!(refused(missing), RpcError::GrantInvalid);
    assert_eq!(
        refused(changed("confirm_unattended_merge", json!(false), "m-false")),
        RpcError::GrantInvalid
    );
    // Not alone, in either order.
    for (index, kinds) in [
        json!(["delivery.merge", "task.propose"]),
        json!(["task.propose", "delivery.merge"]),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            refused(changed("kinds", kinds, &format!("m-kinds-{index}"))),
            RpcError::GrantInvalid
        );
    }
    // A workstream scope, too many uses, too long a life, a malformed prefix.
    assert_eq!(
        refused(changed(
            "scope",
            json!({"kind": "workstream", "id": workstream}),
            "m-workstream"
        )),
        RpcError::GrantInvalid
    );
    assert_eq!(
        refused(changed("max_uses", json!(21), "m-uses")),
        RpcError::GrantInvalid
    );
    assert_eq!(
        refused(changed("expires_in_secs", json!(24 * 3600 + 1), "m-life")),
        RpcError::GrantInvalid
    );
    assert_eq!(
        refused(changed("branch_prefix", json!("a b"), "m-prefix")),
        RpcError::GrantInvalid
    );
    // A prefix belongs to a merge grant alone.
    let mut ordinary = create_params(alpha_scope(), json!(["task.propose"]), "m-ordinary");
    ordinary["branch_prefix"] = json!("moa/");
    assert_eq!(refused(ordinary), RpcError::GrantInvalid);
    assert!(api
        .store
        .standing_grants(true, crate::executor::unix_now_ms(), 10)
        .unwrap()
        .is_empty());

    let mut params = merge_params("m-ok");
    params["branch_prefix"] = json!("moa/");
    let created = call(&api, "grants.create", params.clone()).unwrap();
    assert_eq!(call(&api, "grants.create", params).unwrap(), created);
    let grant = &created["grant"];
    assert_eq!(grant["kinds"], json!(["delivery.merge"]));
    assert_eq!(grant["confirm_unattended_merge"], true);
    assert_eq!(grant["branch_prefix"], "moa/");
    assert_eq!(grant["max_uses"], 5, "the default of a merge grant");
    assert_eq!(grant["status"], "active");
    // The limits are inclusive.
    let mut limits = merge_params("m-limits");
    limits["max_uses"] = json!(20);
    limits["expires_in_secs"] = json!(24 * 3600);
    assert!(call(&api, "grants.create", limits).is_ok());
    // An ordinary grant shows neither.
    let plain = call(
        &api,
        "grants.create",
        create_params(alpha_scope(), json!(["task.propose"]), "m-plain"),
    )
    .unwrap();
    assert_eq!(plain["grant"]["confirm_unattended_merge"], false);
    assert_eq!(plain["grant"]["branch_prefix"], Value::Null);
    assert_eq!(plain["grant"]["max_uses"], 20);
}

#[test]
fn grants_revoke_is_immediate_repeatable_and_names_unknown_grants() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let created = call(
        &api,
        "grants.create",
        create_params(alpha_scope(), json!(["task.propose"]), "g-1"),
    )
    .unwrap();
    let id = created["grant"]["id"].as_str().unwrap().to_owned();

    let revoked = call(
        &api,
        "grants.revoke",
        json!({"grant_id": id, "idempotency_key": "r-1"}),
    )
    .unwrap();
    assert_eq!(revoked["grant"]["status"], "revoked");
    assert_eq!(revoked["stopped"], 0);
    assert!(revoked["grant"]["revoked_at_ms"].as_u64().is_some());
    // Another key revokes it again, with no effect.
    let again = call(
        &api,
        "grants.revoke",
        json!({"grant_id": id, "idempotency_key": "r-2"}),
    )
    .unwrap();
    assert_eq!(
        again["grant"]["revoked_at_ms"],
        revoked["grant"]["revoked_at_ms"]
    );
    assert_eq!(again["stopped"], 0);
    assert_eq!(
        call(
            &api,
            "grants.revoke",
            json!({"grant_id": "grant-missing", "idempotency_key": "r-3"})
        )
        .unwrap_err(),
        RpcError::GrantNotFound
    );

    // A revoked grant leaves the default list and stays in the full one.
    let open = call(&api, "grants.list", json!({})).unwrap();
    assert_eq!(open["grants"], json!([]));
    let all = call(&api, "grants.list", json!({"include_ended": true})).unwrap();
    assert_eq!(all["grants"][0]["status"], "revoked");
}
