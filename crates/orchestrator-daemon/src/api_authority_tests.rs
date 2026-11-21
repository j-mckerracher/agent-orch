//! The authority model (DPES-1108 phase 5): which cards of a completed turn the orchestrator runs
//! on its own, and the bounds that keep the rest cards for the user. Every test drives an
//! in-process `DaemonApi` over a real store and calls the pass directly, as the matcher thread
//! does after a turn completes. No agent CLI is launched: the project roots do not exist.

use orchestrator_core::orchestrator_action::{parse_block, BlockScope, MAX_AUTONOMOUS_PER_TURN};
use orchestrator_core::question::MAX_REPLY_BYTES;
use orchestrator_core::task::{Session, SessionState, WaitReason};
use orchestrator_store::{
    ConversationTarget, ProposalState, QuestionRaiser, ReplyKind, TurnBlock, TurnPermission,
    TurnTrigger,
};

use super::actions::project_code;
use super::actions_task_tests::{
    proposed_task_id, register_project, running_task, submit_queued, ALPHA,
};
use super::actions_tests::{call, new_api, proposal};
use super::authority::AUTONOMOUS_ACTOR;
use super::*;

const WORDS: &str = "Please write the docs task and just send me a report.";

/// How a test turn was opened.
#[derive(Clone, Copy)]
enum Opened {
    User,
    Relay,
    Wake,
}

/// Stores one accepted block as the proposals of a completed global turn, as a finished reply
/// does, and returns the effective proposal ids in block order.
fn turn(api: &DaemonApi, id: &str, opened: Opened, actions: &str) -> Vec<String> {
    let trigger = match opened {
        Opened::Wake => TurnTrigger::Wake,
        Opened::User | Opened::Relay => TurnTrigger::User,
    };
    api.store
        .begin_conversation_turn_as(
            &ConversationTarget::Global,
            id,
            WORDS,
            "test-runtime",
            "1.0.0",
            trigger,
        )
        .unwrap();
    if matches!(opened, Opened::Relay) {
        api.store.mark_turn_read_report(id).unwrap();
    }
    api.store
        .record_conversation_turn_permission(
            id,
            TurnPermission {
                level: PermissionLevel::Restricted,
                profile: "test-profile".into(),
            },
        )
        .unwrap();
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    let block = parse_block(&held, BlockScope::Global).unwrap();
    api.store
        .complete_conversation_turn(id, TurnBlock::Accepted(block))
        .unwrap()
        .unwrap()
}

fn run(api: &DaemonApi, turn_id: &str, ids: &[String]) -> usize {
    api.run_autonomous(turn_id, ids, crate::executor::unix_now_ms())
}

fn state(api: &DaemonApi, id: &str) -> ProposalState {
    proposal(api, id).state
}

fn propose_task(code: &str, objective: &str, delivery: &str) -> String {
    format!(
        r#"{{"kind": "task.propose", "project": "{code}", "objective": "{objective}", "agent": "codex", "delivery": "{delivery}"}}"#
    )
}

fn workstream(title: &str) -> String {
    format!(r#"{{"kind": "workstream.create", "title": "{title}"}}"#)
}

fn autonomy_off(api: &DaemonApi) {
    api.store
        .update_orchestrator_settings(&orchestrator_store::SettingsPatch {
            autonomous_actions: Some(false),
            ..orchestrator_store::SettingsPatch::default()
        })
        .unwrap();
}

/// A user turn's autonomous cards run at once, in block order, approved by the orchestrator,
/// and a task it starts cites the turn as its origin.
#[test]
fn a_user_turn_runs_its_autonomous_cards_at_once_and_the_task_cites_the_turn() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let ids = turn(
        &api,
        "user-1",
        Opened::User,
        &format!(
            r#"{{"kind": "workstream.create", "ref": "w", "title": "Docs"}}, {{"kind": "objective.add", "workstream_ref": "w", "title": "Plan"}}, {}"#,
            propose_task(&code, "Write the docs.", "LocalReview")
        ),
    );
    assert_eq!(run(&api, "user-1", &ids), 3);
    for id in &ids {
        assert_eq!(state(&api, id), ProposalState::Executed, "{id}");
        assert_eq!(
            api.store.proposal_approver(id).unwrap().as_deref(),
            Some(AUTONOMOUS_ACTOR)
        );
    }
    let task_id = proposed_task_id(&ids[2]);
    assert_eq!(api.store.get(&task_id).unwrap().state, TaskState::Queued);
    let origin = api.store.task_origin(&task_id).unwrap().unwrap();
    assert_eq!(origin.origin_ref, "user-1");
    assert_eq!(origin.proposal_id, ids[2]);

    // The card says it ran on its own.
    let page = call(
        &api,
        "conversations.get",
        json!({"target": {"kind": "global"}}),
    )
    .unwrap();
    let card = page["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["id"] == ids[2])
        .unwrap()
        .clone();
    assert_eq!(card["autonomous"], true, "{card}");
    assert_eq!(card["grant_id"], Value::Null, "{card}");
}

/// Cancel, revise, merge, remove and hold need the user's word, whatever the turn.
#[test]
fn user_word_cards_stay_pending() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task = submit_queued(&api, "queued-1", "Queued work.");
    let code = project_code(ALPHA);
    let ids = turn(
        &api,
        "user-1",
        Opened::User,
        &format!(
            r#"{{"kind": "task.cancel", "task_id": "{task}"}}, {{"kind": "task.revise", "task_id": "{task}", "added_intent": "More."}}, {{"kind": "decision.hold", "task_id": "{task}", "until": "2099-01-01"}}, {{"kind": "project.remove", "project": "{code}"}}"#
        ),
    );
    assert_eq!(run(&api, "user-1", &ids), 0);
    for id in &ids {
        assert_eq!(state(&api, id), ProposalState::Pending, "{id}");
    }
    assert_eq!(api.store.get(&task).unwrap().state, TaskState::Queued);
}

/// A wake turn may raise a decision on existing work, but may not create a task or a
/// workstream: those stay cards. A relay of an agent's report is held to the same rule.
#[test]
fn a_wake_or_relay_turn_acts_on_existing_work_and_never_creates_a_task() {
    for (opened, turn_id) in [(Opened::Wake, "wake-1"), (Opened::Relay, "relay-1")] {
        let (_directory, api) = new_api();
        register_project(&api, ALPHA);
        let task = submit_queued(&api, "queued-1", "Queued work.");
        let code = project_code(ALPHA);
        let ids = turn(
            &api,
            turn_id,
            opened,
            &format!(
                r#"{}, {}, {{"kind": "decision.ask", "task_id": "{task}", "question": "Which branch?"}}"#,
                propose_task(&code, "New work.", "LocalReview"),
                workstream("New initiative")
            ),
        );
        assert_eq!(run(&api, turn_id, &ids), 1, "{turn_id}");
        assert_eq!(state(&api, &ids[0]), ProposalState::Pending, "{turn_id}");
        assert_eq!(state(&api, &ids[1]), ProposalState::Pending, "{turn_id}");
        assert_eq!(state(&api, &ids[2]), ProposalState::Executed, "{turn_id}");
        assert!(api.store.get(&proposed_task_id(&ids[0])).is_err());
    }
}

/// The restart path holds the same line: a task card from a wake turn that somehow ended up
/// approved by the orchestrator fails with `no_user_origin` and makes no task.
#[test]
fn a_restart_never_runs_a_wake_task_the_orchestrator_approved() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let ids = turn(
        &api,
        "wake-1",
        Opened::Wake,
        &propose_task(&code, "New work.", "LocalReview"),
    );
    api.store
        .approve_action_proposal(&ids[0], AUTONOMOUS_ACTOR)
        .unwrap();
    api.reconcile_actions();
    let stored = proposal(&api, &ids[0]);
    assert_eq!(stored.state, ProposalState::Failed);
    assert_eq!(stored.error_code.as_deref(), Some("no_user_origin"));
    assert!(api.store.get(&proposed_task_id(&ids[0])).is_err());
}

/// A task for a project that is not registered, or that the user removed, stays a card.
#[test]
fn a_task_for_an_unregistered_or_removed_project_stays_a_card() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    api.store.archive_project(ALPHA).unwrap();
    let ids = turn(
        &api,
        "user-1",
        Opened::User,
        &format!(
            "{}, {}",
            propose_task(&project_code(ALPHA), "Removed project.", "LocalReview"),
            propose_task("p-00000000", "Unknown project.", "LocalReview")
        ),
    );
    assert_eq!(run(&api, "user-1", &ids), 0);
    for id in &ids {
        assert_eq!(state(&api, id), ProposalState::Pending, "{id}");
    }
}

/// With the switch off every card waits for the user; a turn the pass already looked at is not
/// picked up again when the switch comes back on.
#[test]
fn the_switch_off_leaves_every_card_pending_and_turning_it_on_runs_no_old_card() {
    let (_directory, api) = new_api();
    autonomy_off(&api);
    let ids = turn(&api, "user-1", Opened::User, &workstream("Docs"));
    assert_eq!(run(&api, "user-1", &ids), 0);
    assert_eq!(state(&api, &ids[0]), ProposalState::Pending);

    api.store
        .update_orchestrator_settings(&orchestrator_store::SettingsPatch {
            autonomous_actions: Some(true),
            ..orchestrator_store::SettingsPatch::default()
        })
        .unwrap();
    // The thread only runs turns queued by a completion; nothing re-queues this one.
    api.autonomy_tick(crate::executor::unix_now_ms());
    assert_eq!(state(&api, &ids[0]), ProposalState::Pending);
}

/// At most `MAX_AUTONOMOUS_PER_TURN` cards of one turn run on their own.
#[test]
fn a_turn_runs_at_most_the_cap_and_the_rest_stay_cards() {
    let (_directory, api) = new_api();
    let actions: Vec<String> = (0..=MAX_AUTONOMOUS_PER_TURN)
        .map(|index| workstream(&format!("Initiative {index}")))
        .collect();
    let ids = turn(&api, "user-1", Opened::User, &actions.join(", "));
    assert_eq!(run(&api, "user-1", &ids), MAX_AUTONOMOUS_PER_TURN);
    let executed = ids
        .iter()
        .filter(|id| state(&api, id) == ProposalState::Executed)
        .count();
    assert_eq!(executed, MAX_AUTONOMOUS_PER_TURN);
    assert_eq!(state(&api, ids.last().unwrap()), ProposalState::Pending);
}

/// A card an earlier turn made, that a later block only matched, is not the later turn's to run.
#[test]
fn a_matched_card_of_an_earlier_turn_is_not_run_by_a_later_turn() {
    let (_directory, api) = new_api();
    let earlier = turn(&api, "wake-1", Opened::Wake, &workstream("Docs"));
    assert_eq!(run(&api, "wake-1", &earlier), 0);
    let later = turn(&api, "user-1", Opened::User, &workstream("Docs"));
    assert_eq!(later, earlier);
    assert_eq!(run(&api, "user-1", &later), 0);
    assert_eq!(state(&api, &earlier[0]), ProposalState::Pending);
}

/// A user's departure from the project's default delivery is stored on that task with the
/// user's words, and a task that takes the default records nothing.
#[test]
fn a_departure_from_the_default_delivery_is_kept_on_that_task_only() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let ids = turn(
        &api,
        "user-1",
        Opened::User,
        &format!(
            "{}, {}",
            propose_task(&code, "Report only.", "ReportOnly"),
            propose_task(&code, "Default delivery.", "LocalReview")
        ),
    );
    assert_eq!(run(&api, "user-1", &ids), 2);
    let departed = api
        .store
        .task_overrides(&proposed_task_id(&ids[0]))
        .unwrap();
    assert_eq!(departed.len(), 1);
    assert_eq!(departed[0].field, "delivery");
    assert_eq!(departed[0].project_default, "LocalReview");
    assert_eq!(departed[0].chosen, "ReportOnly");
    assert_eq!(departed[0].user_words, WORDS);
    assert!(api
        .store
        .task_overrides(&proposed_task_id(&ids[1]))
        .unwrap()
        .is_empty());
}

/// `tasks.discard` records the user's word for a finished task only.
#[test]
fn tasks_discard_takes_a_finished_task_only() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task = submit_queued(&api, "queued-1", "Queued work.");
    assert_eq!(
        call(
            &api,
            "tasks.discard",
            json!({"task_id": task, "idempotency_key": "d-1"})
        ),
        Err(RpcError::Conflict)
    );
    call(
        &api,
        "tasks.cancel",
        json!({"task_id": task, "idempotency_key": "c-1"}),
    )
    .unwrap();
    let discarded = call(
        &api,
        "tasks.discard",
        json!({"task_id": task, "idempotency_key": "d-2"}),
    )
    .unwrap();
    assert_eq!(discarded["discarded"], true, "{discarded}");
    assert!(api.store.task_discarded(&task).unwrap());
    assert_eq!(
        call(
            &api,
            "tasks.discard",
            json!({"task_id": "no-such-task", "idempotency_key": "d-3"})
        ),
        Err(RpcError::NotFound)
    );
}

/// A task whose worker reported a wait, as the executor leaves it.
fn waiting_task(api: &DaemonApi, key: &str) -> String {
    let id = running_task(api, key);
    api.store
        .add_session(
            &id,
            &Session {
                id: "session-1".into(),
                attempt_id: "attempt-1".into(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    api.store
        .update_session_state(&id, "session-1", SessionState::Running)
        .unwrap();
    api.store
        .enter_waiting(&id, "attempt-1", "session-1", WaitReason::Blocker, &[])
        .unwrap();
    id
}

/// `tasks.message` queues the user's words for any task that is not busy, and the same key
/// queues once. A running task is refused.
#[test]
fn tasks_message_takes_a_waiting_task_and_refuses_the_rest() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let queued = submit_queued(&api, "queued-1", "Queued work.");
    assert_eq!(
        call(
            &api,
            "tasks.message",
            json!({"task_id": queued, "text": "Hello.", "idempotency_key": "m-1"})
        )
        .unwrap(),
        json!({"task_id": queued, "queued": true}),
        "a task that has not started keeps the message for its launch"
    );
    let running = running_task(&api, "running-1");
    assert_eq!(
        call(
            &api,
            "tasks.message",
            json!({"task_id": running, "text": "Hello.", "idempotency_key": "m-0"})
        ),
        Err(RpcError::Conflict)
    );
    assert_eq!(
        call(
            &api,
            "tasks.message",
            json!({"task_id": "no-such-task", "text": "Hello.", "idempotency_key": "m-2"})
        ),
        Err(RpcError::NotFound)
    );

    let task = waiting_task(&api, "waiting-1");
    let long = "x".repeat(MAX_REPLY_BYTES + 1);
    for (key, text) in [("m-3", " \n"), ("m-4", "ok\u{7}"), ("m-5", long.as_str())] {
        assert_eq!(
            call(
                &api,
                "tasks.message",
                json!({"task_id": task, "text": text, "idempotency_key": key})
            ),
            Err(RpcError::InvalidParams),
            "{key}"
        );
    }

    // A worker that waits on an open question takes no user message until it closes.
    let asked = waiting_task(&api, "waiting-2");
    api.store
        .raise_question(
            "q-asked",
            &asked,
            "Which?",
            &[],
            None,
            QuestionRaiser::Worker,
        )
        .unwrap();
    assert_eq!(
        call(
            &api,
            "tasks.message",
            json!({"task_id": asked, "text": "Hello.", "idempotency_key": "m-7"})
        ),
        Err(RpcError::Conflict)
    );

    let sent = json!({"task_id": task, "text": "Use the other branch.", "idempotency_key": "m-6"});
    let reply = call(&api, "tasks.message", sent.clone()).unwrap();
    assert_eq!(reply, json!({"task_id": task, "queued": true}));
    assert_eq!(call(&api, "tasks.message", sent).unwrap(), reply);
    let replies = api.store.pending_replies(&task).unwrap();
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0].kind, ReplyKind::UserMessage);
    assert_eq!(replies[0].text, "Use the other branch.");
}

/// A finished task takes the user's message: it goes back to waiting under a new attempt. The
/// agent's own refusal (Codex at Auto) is checked first, so the task stays as it was.
#[test]
fn tasks_message_reopens_a_finished_task_unless_its_agent_cannot_resume() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task = waiting_task(&api, "finished-1");
    api.store
        .record_agent_session_id(&task, "session-1", "abc-123")
        .unwrap();
    api.store.fail_waiting_task(&task, "gone").unwrap();
    let sent = |key: &str| json!({"task_id": task, "text": "Add a test.", "idempotency_key": key});
    // The fixture task is Codex at the default level, which cannot resume.
    let refused = call(&api, "tasks.message", sent("f-1"));
    assert_eq!(refused, Err(RpcError::Conflict), "{refused:?}");
    assert_eq!(api.store.get(&task).unwrap().state, TaskState::Failed);
}

/// The switch is on by default, read by `settings.get`, and turned off by `settings.set`.
#[test]
fn settings_carry_the_autonomy_switch_on_by_default() {
    let (_directory, api) = new_api();
    assert_eq!(
        call(&api, "settings.get", Value::Null).unwrap()["autonomous_actions"],
        true
    );
    let set = call(
        &api,
        "settings.set",
        json!({"idempotency_key": "s-1", "autonomous_actions": false}),
    )
    .unwrap();
    assert_eq!(set["autonomous_actions"], false, "{set}");
    assert!(
        !api.store
            .orchestrator_settings()
            .unwrap()
            .autonomous_actions
    );
}
