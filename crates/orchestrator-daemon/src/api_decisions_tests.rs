//! User decisions (DPES-1108 phase 3): `decision.ask` and the `decisions.*` methods, driven
//! through an in-process `DaemonApi` over a real store. The project root does not exist
//! (`ALPHA`), so no agent can start. The user-only gate lives in the ipc crate and is pinned
//! there and in `tests/conversation_runtime.rs`.

use super::actions_task_tests::{register_project, submit_queued, ALPHA};
use super::actions_tests::{call, new_api, propose, resolve};
use super::holds::civil_from_days;
use super::*;

const MS_PER_DAY: i64 = 86_400_000;

fn ask_action(task_id: &str) -> String {
    json!({"kind": "decision.ask", "task_id": task_id, "question": "SQLite or Postgres?",
        "options": ["SQLite", "Postgres"], "key": "db"})
    .to_string()
}

fn date_in(days: i64) -> String {
    let today = i64::try_from(crate::executor::unix_now_ms()).unwrap() / MS_PER_DAY;
    let (year, month, day) = civil_from_days(today + days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A queued task with one question raised through an approved `decision.ask`.
fn asked(api: &DaemonApi) -> (String, String) {
    register_project(api, ALPHA);
    let task = submit_queued(api, "s-1", "Pick the database");
    let ids = propose(api, "t1", &ask_action(&task));
    let result = resolve(api, &ids[0], "approve", "k-1").unwrap();
    assert_eq!(result["proposal"]["state"], "executed", "{result}");
    let reference = result["proposal"]["result_ref"].as_str().unwrap();
    let question = reference.strip_prefix("question:").unwrap().to_owned();
    (task, question)
}

fn listed(api: &DaemonApi) -> Vec<Value> {
    call(api, "decisions.list", json!({})).unwrap()["questions"]
        .as_array()
        .unwrap()
        .clone()
}

/// The queue entry of `task_id`.
fn queued(api: &DaemonApi, task_id: &str) -> Value {
    let snapshot = call(api, "queue.snapshot", Value::Null).unwrap();
    snapshot["queued"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["task_id"] == task_id)
        .cloned()
        .unwrap_or_else(|| panic!("{task_id} is not queued: {snapshot}"))
}

#[test]
fn an_approved_ask_raises_one_question_that_holds_its_task() {
    let (_directory, api) = new_api();
    let (task, question) = asked(&api);

    let questions = listed(&api);
    assert_eq!(questions.len(), 1);
    let listed = &questions[0];
    assert_eq!(listed["id"], question.as_str());
    assert_eq!(listed["task_id"], task.as_str());
    assert_eq!(listed["task_title"], "Pick the database");
    assert_eq!(listed["question"], "SQLite or Postgres?");
    assert_eq!(listed["options"], json!(["SQLite", "Postgres"]));
    assert_eq!(listed["key"], "db");
    assert_eq!(listed["raised_by"], "orchestrator");
    assert_eq!(listed["state"], "open");
    assert_eq!(listed["deferred_until"], Value::Null);
    assert_eq!(listed["closing"], Value::Null);

    let entry = queued(&api, &task);
    assert_eq!(entry["waiting_reason"], "decision", "{entry}");
    assert_eq!(entry["waiting_detail"], "open_question", "{entry}");
    assert_eq!(entry["position"], Value::Null, "{entry}");

    // Asking again under the same key, from another turn, finds the open question.
    let ids = propose(&api, "t2", &ask_action(&task));
    let again = resolve(&api, &ids[0], "approve", "k-2").unwrap();
    assert_eq!(
        again["proposal"]["result_ref"],
        format!("question:{question}")
    );
    assert_eq!(listed_len(&api), 1);
}

fn listed_len(api: &DaemonApi) -> usize {
    listed(api).len()
}

#[test]
fn an_answer_is_stored_verbatim_closes_once_and_frees_the_task() {
    let (_directory, api) = new_api();
    let (task, question) = asked(&api);
    let words = "  Postgres in prod.\nSQLite in tests.  ";
    let request = json!({"question_id": question, "answer": words, "idempotency_key": "a-1"});

    let answered = call(&api, "decisions.answer", request.clone()).unwrap();
    assert_eq!(answered["question"]["state"], "answered", "{answered}");
    assert_eq!(answered["question"]["closing"]["kind"], "answer");
    assert_eq!(answered["question"]["closing"]["text"], words);
    assert!(listed(&api).is_empty());
    assert_ne!(queued(&api, &task)["waiting_reason"], "decision");

    // A replay returns the stored outcome; a new answer to a closed question is a conflict.
    assert_eq!(call(&api, "decisions.answer", request).unwrap(), answered);
    let late = json!({"question_id": question, "answer": "SQLite", "idempotency_key": "a-2"});
    assert_eq!(
        call(&api, "decisions.answer", late),
        Err(RpcError::Conflict)
    );
    let moot = json!({"question_id": question, "evidence": "x", "idempotency_key": "r-1"});
    assert_eq!(
        call(&api, "decisions.reconcile", moot),
        Err(RpcError::Conflict)
    );
}

#[test]
fn a_deferral_keeps_the_question_open_and_the_task_held() {
    let (_directory, api) = new_api();
    let (task, question) = asked(&api);
    let until = date_in(3);

    let deferred = call(
        &api,
        "decisions.defer",
        json!({"question_id": question, "until": until, "idempotency_key": "d-1"}),
    )
    .unwrap();
    assert_eq!(deferred["question"]["state"], "open", "{deferred}");
    assert_eq!(deferred["question"]["deferred_until"], until.as_str());
    assert_eq!(listed(&api)[0]["deferred_until"], until.as_str());
    assert_eq!(queued(&api, &task)["waiting_reason"], "decision");

    // The hold range applies: past days and days beyond 60 are refused.
    for (index, bad) in [date_in(-1), date_in(61), "soon".to_owned()]
        .iter()
        .enumerate()
    {
        let request = json!({"question_id": question, "until": bad, "idempotency_key": format!("d-x{index}")});
        assert_eq!(
            call(&api, "decisions.defer", request),
            Err(RpcError::InvalidParams),
            "{bad}"
        );
    }

    // A reconcile closes it with evidence, and the task is free.
    let evidence = "The user chose Postgres in the design review.";
    let closed = call(
        &api,
        "decisions.reconcile",
        json!({"question_id": question, "evidence": evidence, "idempotency_key": "r-1"}),
    )
    .unwrap();
    assert_eq!(closed["question"]["state"], "reconciled", "{closed}");
    assert_eq!(closed["question"]["closing"]["kind"], "reconcile");
    assert_eq!(closed["question"]["closing"]["text"], evidence);
    assert_eq!(closed["question"]["deferred_until"], Value::Null);
    assert_ne!(queued(&api, &task)["waiting_reason"], "decision");
}

#[test]
fn the_methods_refuse_unknown_questions_and_bad_text() {
    let (_directory, api) = new_api();
    let (_task, question) = asked(&api);

    let unknown = json!({"question_id": "nope", "answer": "x", "idempotency_key": "u-1"});
    assert_eq!(
        call(&api, "decisions.answer", unknown),
        Err(RpcError::NotFound)
    );
    let blank = json!({"question_id": question, "answer": " \n", "idempotency_key": "u-2"});
    assert_eq!(
        call(&api, "decisions.answer", blank),
        Err(RpcError::InvalidParams)
    );
    let extra = json!({"question_id": question, "answer": "x", "by": "agent",
        "idempotency_key": "u-3"});
    assert_eq!(
        call(&api, "decisions.answer", extra),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        call(&api, "decisions.list", json!({"all": true})),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(listed_len(&api), 1);
}

#[test]
fn an_ask_on_a_missing_task_fails_and_raises_nothing() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let ids = propose(&api, "t1", &ask_action("task-missing"));
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_eq!(result["proposal"]["state"], "failed", "{result}");
    assert!(listed(&api).is_empty());
}
