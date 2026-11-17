//! Routing by scope (DPES-1105 phase 6a): what a workstream conversation sees of its own
//! tasks, what the global orchestrator routed to it, and the flat-depth rule. Every test
//! drives an in-process `DaemonApi` over a real store and never launches an agent CLI:
//! the project root is a path that does not exist, so a launch fails before any process
//! starts.

use orchestrator_core::orchestrator_action::{parse_block, BlockError, BlockScope};
use orchestrator_store::ConversationTarget;

use super::actions::project_code;
use super::actions_task_tests::{
    assert_executed, assert_failed, proposed_task_id, register_project, submit_queued, ALPHA,
};
use super::actions_tests::{create_workstream, new_api, proposal, propose, propose_in, resolve};
use super::conversation_agent::{INSTRUCTIONS, INSTRUCTIONS_WITH_ACTIONS};
use super::conversation_snapshot::build_snapshot;
use super::*;

/// The `task.propose` action that routes `objective` to `workstream`.
fn route_block(workstream: &str, objective: &str) -> String {
    format!(
        r#"{{"kind": "task.propose", "project": "{}", "objective": "{objective}", "agent": "codex", "workstream_id": "{workstream}"}}"#,
        project_code(ALPHA)
    )
}

/// Proposes in the global conversation, approves as the user and returns the task id.
fn route(api: &DaemonApi, turn: &str, workstream: &str, objective: &str) -> String {
    let ids = propose(api, turn, &route_block(workstream, objective));
    let approved = resolve(api, &ids[0], "approve", &format!("k-{turn}")).unwrap();
    let task_id = proposed_task_id(&ids[0]);
    assert_executed(&approved, &task_id);
    task_id
}

fn snapshot(api: &DaemonApi, target: &ConversationTarget, actions: bool) -> String {
    build_snapshot(
        &api.store,
        target,
        actions,
        &api.config.load.read(std::time::Instant::now()),
    )
}

fn workstream_target(id: &str) -> ConversationTarget {
    ConversationTarget::Workstream(id.to_owned())
}

/// The text of the snapshot between two headings.
fn between<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let start = text
        .find(from)
        .unwrap_or_else(|| panic!("no {from} in {text}"));
    let rest = &text[start..];
    &rest[..rest.find(to).unwrap_or_else(|| panic!("no {to} in {text}"))]
}

fn get_with_actions(api: &DaemonApi, workstream: Option<&str>, actions: bool) -> Value {
    let target = match workstream {
        Some(id) => json!({"kind": "workstream", "workstream_id": id}),
        None => json!({"kind": "global"}),
    };
    let capabilities: &[Capability] = if actions {
        &[Capability::OrchestratorActions]
    } else {
        &[]
    };
    api.dispatch_in(
        "conversations.get",
        json!({"target": target}),
        &CallContext::new(capabilities),
    )
    .unwrap()
}

#[test]
fn a_workstream_snapshot_lists_its_own_tasks_before_the_global_open_tasks() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let other = create_workstream(&api, "Other", "w-2");
    let mine = route(&api, "t1", &own, "Own piece of work.");
    let foreign = route(&api, "t2", &other, "Somebody else's piece of work.");

    let text = snapshot(&api, &workstream_target(&own), true);

    let own_block = between(&text, "Tasks of this workstream", "Projects (");
    assert!(own_block.contains(&mine), "{text}");
    assert!(!own_block.contains(&foreign), "{text}");
    let own_at = text.find("Tasks of this workstream").unwrap();
    assert!(
        own_at < text.find("Open tasks (up to 20").unwrap(),
        "{text}"
    );
    // The global list still names both, as it did before this phase.
    let open = &text[text.find("Open tasks (up to 20").unwrap()..];
    assert!(open.contains(&mine) && open.contains(&foreign), "{text}");
    // A global conversation has no block of its own tasks.
    let global = snapshot(&api, &ConversationTarget::Global, true);
    assert!(!global.contains("Tasks of this workstream"), "{global}");
}

#[test]
fn a_routed_task_carries_the_routed_marker_in_the_workstream_snapshot() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let routed = route(&api, "t1", &own, "Routed piece of work.");
    let linked = submit_queued(&api, "s-1", "Linked by hand.");
    api.store.link_task(&own, &linked, None).unwrap();

    let text = snapshot(&api, &workstream_target(&own), true);

    let block = between(&text, "Tasks of this workstream", "Projects (");
    let line_of = |id: &str| block.lines().find(|line| line.contains(id)).unwrap();
    assert!(
        line_of(&routed).ends_with("(routed by the global orchestrator)"),
        "{block}"
    );
    assert!(!line_of(&linked).contains("routed"), "{block}");
    // Without the actions capability the marker is not read at all.
    let plain = snapshot(&api, &workstream_target(&own), false);
    assert!(!plain.contains("routed by"), "{plain}");
}

#[test]
fn a_workstream_snapshot_hides_the_workstream_create_kind_and_global_keeps_it() {
    let (_directory, api) = new_api();
    let own = create_workstream(&api, "Own", "w-1");
    let kinds = |text: &str| -> String {
        text.lines()
            .find(|line| line.starts_with("Action kinds:"))
            .unwrap()
            .to_owned()
    };

    let in_workstream = kinds(&snapshot(&api, &workstream_target(&own), true));
    let global = kinds(&snapshot(&api, &ConversationTarget::Global, true));

    assert!(
        !in_workstream.contains("workstream.create"),
        "{in_workstream}"
    );
    assert!(
        in_workstream.contains("workstream.update"),
        "{in_workstream}"
    );
    assert!(in_workstream.contains("task.propose"), "{in_workstream}");
    assert!(
        global.starts_with("Action kinds: workstream.create, "),
        "{global}"
    );
}

#[test]
fn a_workstream_conversation_cannot_create_a_workstream() {
    let create = r#"{"kind": "workstream.create", "title": "Child"}"#;
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{create}]}}\n```");
    assert!(matches!(
        parse_block(&held, BlockScope::Workstream("w-own")),
        Err(BlockError::OutOfScope)
    ));

    // A card stored all the same fails at execution, and nothing is created.
    let (_directory, api) = new_api();
    let own = create_workstream(&api, "Own", "w-1");
    let ids = propose_in(
        &api,
        &workstream_target(&own),
        "t1",
        PermissionLevel::Restricted,
        create,
    );
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "invalid");
    assert_eq!(api.store.list_workstreams(None).unwrap().len(), 1);
}

#[test]
fn a_workstream_conversation_cannot_route_a_task_to_another_workstream() {
    let held = format!(
        "```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{}]}}\n```",
        route_block("w-other", "Into the other one.")
    );
    assert!(matches!(
        parse_block(&held, BlockScope::Workstream("w-own")),
        Err(BlockError::OutOfScope)
    ));

    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let other = create_workstream(&api, "Other", "w-2");
    let ids = propose_in(
        &api,
        &workstream_target(&own),
        "t1",
        PermissionLevel::Restricted,
        &route_block(&other, "Into the other one."),
    );
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "invalid");
    assert!(api.store.list().unwrap().is_empty());
    assert!(api.store.routed_in(&other, 10).unwrap().is_empty());
}

#[test]
fn global_routes_a_task_to_a_workstream_and_the_user_approval_is_still_required() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let ids = propose(&api, "t1", &route_block(&own, "Routed piece of work."));

    // Proposed, not approved: nothing exists yet and nothing is routed.
    assert_eq!(
        proposal(&api, &ids[0]).state,
        orchestrator_store::ProposalState::Pending
    );
    assert!(api.store.list().unwrap().is_empty());
    assert!(api.store.routed_in(&own, 10).unwrap().is_empty());

    // A caller that is not the user cannot approve it.
    let digest = proposal(&api, &ids[0]).digest;
    let refused = api
        .resolve_as(
            &Actor::Agent("codex".to_owned()),
            json!({"proposal_id": ids[0], "decision": "approve", "digest": digest, "idempotency_key": "k-agent"}),
        )
        .unwrap_err();
    assert_eq!(refused, RpcError::Forbidden);
    assert_eq!(
        proposal(&api, &ids[0]).state,
        orchestrator_store::ProposalState::Pending
    );
    assert!(api.store.list().unwrap().is_empty());

    // The user's approval makes the task, links it and routes it.
    let approved = resolve(&api, &ids[0], "approve", "k-user").unwrap();
    let task_id = proposed_task_id(&ids[0]);
    assert_executed(&approved, &task_id);
    let routed = api.store.routed_in(&own, 10).unwrap();
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].task_id, task_id);
    assert_eq!(
        api.store
            .workstream_task_link(&task_id)
            .unwrap()
            .unwrap()
            .workstream_id,
        own
    );
}

#[test]
fn conversations_get_adds_routed_in_for_a_workstream_only_with_the_actions_capability() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let other = create_workstream(&api, "Other", "w-2");

    // Nothing routed yet: the field is absent, not empty.
    assert!(get_with_actions(&api, Some(&own), true)
        .get("routed_in")
        .is_none());

    let task_id = route(&api, "t1", &own, "Fix the login bug");
    let page = get_with_actions(&api, Some(&own), true);
    let rows = page["routed_in"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{page}");
    assert_eq!(rows[0]["task_id"], task_id);
    assert_eq!(rows[0]["title"], "Fix the login bug");
    assert!(rows[0]["routed_at"]
        .as_str()
        .is_some_and(|at| !at.is_empty()));
    assert!(rows[0]["state"]
        .as_str()
        .is_some_and(|state| !state.is_empty()));

    assert!(get_with_actions(&api, Some(&own), false)
        .get("routed_in")
        .is_none());
    assert!(get_with_actions(&api, Some(&other), true)
        .get("routed_in")
        .is_none());
    assert!(get_with_actions(&api, None, true)
        .get("routed_in")
        .is_none());
}

#[test]
fn routed_in_row_and_snapshot_line_hold_no_path_from_the_objective() {
    const TAIL: &str = "TAIL-SECRET-4471";
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    // The path sits well inside the first 80 bytes; the tail is cut off.
    let objective = format!(
        "Fix /Users/canary-9931/deep/file.rs now {} {TAIL}",
        "and so on ".repeat(10)
    );
    let task_id = route(&api, "t1", &own, &objective);

    let page = get_with_actions(&api, Some(&own), true);
    let row = &page["routed_in"][0];
    let title = row["title"].as_str().unwrap();
    assert!(title.starts_with("Fix [path] now"), "{title}");
    assert!(title.len() <= 80 + '…'.len_utf8(), "{title}");
    let mut keys: Vec<&str> = row
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["routed_at", "state", "task_id", "title"]);

    let text = snapshot(&api, &workstream_target(&own), true);
    for seen in [page.to_string(), text.clone()] {
        assert!(!seen.contains(TAIL), "{seen}");
        assert!(!seen.contains(ALPHA), "{seen}");
        assert!(!seen.contains("canary-9931"), "{seen}");
        assert!(!seen.contains("/Users/"), "{seen}");
    }
    let block = between(&text, "Tasks of this workstream", "Projects (");
    let line = block.lines().find(|line| line.contains(&task_id)).unwrap();
    assert!(line.contains("project alpha ("), "{line}");
    assert!(line.contains("Fix [path] now"), "{line}");
}

#[test]
fn the_preamble_names_the_scope_rule_and_stays_under_the_cap() {
    for sentence in [
        "A workstream conversation acts only inside its own workstream and cannot create workstreams.",
        "In the global conversation, route a task by naming the owning workstream_id in task.propose.",
    ] {
        assert!(INSTRUCTIONS_WITH_ACTIONS.contains(sentence), "{sentence}");
        assert!(!INSTRUCTIONS.contains(sentence), "{sentence}");
    }
    assert!(INSTRUCTIONS_WITH_ACTIONS.len() <= 3 * 1024);
}
