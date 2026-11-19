//! Authority records (DPES-1108 phase 5, migrations 0066 and 0067): the autonomy switch, task
//! origins, one-task overrides, the user's discard and the unlanded-work archive guard.

use std::path::{Path, PathBuf};

use orchestrator_core::Project;
use orchestrator_store::{
    ArchiveError, AuthorityError, ConversationTarget, OriginKind, SettingsPatch, SqliteStore,
    TaskOverride, TurnBlock, TurnTrigger,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

/// Runs one statement on a second connection with `binds` bound in order.
fn sql(path: &Path, statement: &str, binds: &[&str]) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let options = SqliteConnectOptions::new()
                .filename(path)
                .foreign_keys(true);
            let mut connection = sqlx::SqliteConnection::connect_with(&options)
                .await
                .unwrap();
            // Test-only helper: every statement passed in is a literal written in this file.
            let mut query = sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()));
            for value in binds {
                query = query.bind(*value);
            }
            query.execute(&mut connection).await.unwrap();
        });
}

fn store_with_project(directory: &Path) -> (SqliteStore, PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    store
        .register_project(&Project {
            id: "p1".to_owned(),
            root_path: "/work/p1".to_owned(),
            settings: serde_json::json!({}),
        })
        .unwrap();
    (store, path)
}

fn add_task(path: &Path, id: &str, state: &str) {
    sql(
        path,
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES (?, 'p1', NULL, ?, '{}', 1)",
        &[id, &format!("\"{state}\"")],
    );
}

/// A finished attempt of `task` that committed `changed` files.
fn add_attempt(path: &Path, task: &str, changed: &str) {
    sql(
        path,
        "INSERT INTO attempts (id, task_id, attempt_number, state, changed_files) \
         VALUES ('a1', ?, 1, '\"Succeeded\"', CAST(? AS INTEGER))",
        &[task, changed],
    );
}

fn turn(store: &SqliteStore, id: &str, trigger: TurnTrigger) {
    store
        .begin_conversation_turn_as(
            &ConversationTarget::Global,
            id,
            "Start the docs task, and deliver it as a report.",
            "runtime",
            "1.0.0",
            trigger,
        )
        .unwrap();
    store
        .complete_conversation_turn(id, TurnBlock::None)
        .unwrap();
}

#[test]
fn autonomous_actions_are_on_by_default_and_the_user_can_turn_them_off() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _) = store_with_project(directory.path());
    assert!(store.orchestrator_settings().unwrap().autonomous_actions);
    let saved = store
        .update_orchestrator_settings(&SettingsPatch {
            autonomous_actions: Some(false),
            ..SettingsPatch::default()
        })
        .unwrap();
    assert!(!saved.autonomous_actions);
    // A patch that does not name it leaves it off.
    let saved = store
        .update_orchestrator_settings(&SettingsPatch {
            chat_pr_delivery: Some(true),
            ..SettingsPatch::default()
        })
        .unwrap();
    assert!(!saved.autonomous_actions);
}

#[test]
fn only_a_user_turn_that_read_no_report_can_be_a_task_origin() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t1", "Queued");
    turn(&store, "user-turn", TurnTrigger::User);
    turn(&store, "relay-turn", TurnTrigger::User);
    store.mark_turn_read_report("relay-turn").unwrap();
    turn(&store, "wake-turn", TurnTrigger::Wake);

    for refused in ["wake-turn", "relay-turn", "no-such-turn"] {
        let error = store.record_task_origin("t1", refused, "p-1").unwrap_err();
        assert_eq!(
            AuthorityError::of(&error),
            Some(AuthorityError::NotUserTurn),
            "{refused}"
        );
    }
    assert_eq!(store.task_origin("t1").unwrap(), None);

    let origin = store.record_task_origin("t1", "user-turn", "p-1").unwrap();
    assert_eq!(origin.kind, OriginKind::UserTurn);
    assert_eq!(origin.origin_ref, "user-turn");
    // The same origin again is a no-op; a different one is refused.
    assert_eq!(
        store.record_task_origin("t1", "user-turn", "p-1").unwrap(),
        origin
    );
    let error = store
        .record_task_origin("t1", "user-turn", "p-2")
        .unwrap_err();
    assert_eq!(
        AuthorityError::of(&error),
        Some(AuthorityError::AlreadyRecorded)
    );
    assert_eq!(store.task_origin("t1").unwrap(), Some(origin));
}

#[test]
fn an_override_is_kept_with_the_users_words_and_only_for_its_task() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t1", "Queued");
    add_task(&path, "t2", "Queued");
    let record = TaskOverride {
        task_id: "t1".to_owned(),
        field: "delivery".to_owned(),
        project_default: "LocalReview".to_owned(),
        chosen: "ReportOnly".to_owned(),
        user_words: "Just send me a report for this one.".to_owned(),
        turn_id: "turn-1".to_owned(),
        created_at: String::new(),
    };
    let saved = store.record_task_override(&record).unwrap();
    assert_eq!(saved.user_words, record.user_words);
    assert_eq!(store.task_overrides("t1").unwrap(), vec![saved.clone()]);
    assert!(store.task_overrides("t2").unwrap().is_empty());
    // Writing it again returns it; a different choice for the same field is refused.
    assert_eq!(store.record_task_override(&record).unwrap(), saved);
    let other = TaskOverride {
        chosen: "PullRequest".to_owned(),
        ..record.clone()
    };
    assert_eq!(
        AuthorityError::of(&store.record_task_override(&other).unwrap_err()),
        Some(AuthorityError::AlreadyRecorded)
    );
    for invalid in [
        TaskOverride {
            field: "agent".to_owned(),
            ..record.clone()
        },
        TaskOverride {
            chosen: "LocalReview".to_owned(),
            ..record.clone()
        },
        TaskOverride {
            user_words: String::new(),
            ..record.clone()
        },
    ] {
        assert_eq!(
            AuthorityError::of(&store.record_task_override(&invalid).unwrap_err()),
            Some(AuthorityError::InvalidOverride)
        );
    }
}

#[test]
fn committed_work_blocks_the_archive_until_the_user_discards_it() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t1", "Succeeded");
    add_attempt(&path, "t1", "3");
    assert!(store.has_unlanded_candidate("t1").unwrap());
    let error = store.archive_tasks(&["t1".to_owned()]).unwrap_err();
    assert_eq!(ArchiveError::of(&error), Some(ArchiveError::UnlandedWork));
    assert!(store.archived_task_ids().unwrap().is_empty());

    assert!(!store.task_discarded("t1").unwrap());
    assert!(store.discard_task_candidate("t1").unwrap());
    assert!(!store.discard_task_candidate("t1").unwrap());
    assert!(store.task_discarded("t1").unwrap());
    assert!(!store.has_unlanded_candidate("t1").unwrap());
    assert_eq!(
        store.archive_tasks(&["t1".to_owned()]).unwrap(),
        vec!["t1".to_owned()]
    );
}

#[test]
fn a_running_task_cannot_be_discarded() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t1", "Running");
    let error = store.discard_task_candidate("t1").unwrap_err();
    assert_eq!(ArchiveError::of(&error), Some(ArchiveError::NotFinished));
    assert!(store.discard_task_candidate("missing").is_err());
}

#[test]
fn a_run_with_no_changes_or_delivered_work_does_not_block_the_archive() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    // No changed files: nothing was committed.
    add_task(&path, "empty", "Succeeded");
    add_attempt(&path, "empty", "0");
    assert!(!store.has_unlanded_candidate("empty").unwrap());

    // Delivered to a local review ref.
    add_task(&path, "review", "Succeeded");
    add_attempt(&path, "review", "2");
    sql(
        &path,
        "INSERT INTO delivery_steps (id, task_id, action_kind, action, parameter_digest, \
         candidate_sha, approval_id, verification_run_id, destination, outcome) \
         VALUES ('d1', 'review', 'local_review', '{}', ?, ?, 'ap1', 'v1', 'local', \
         'review_ref_created')",
        &[&"a".repeat(64), &"b".repeat(40)],
    );
    assert!(!store.has_unlanded_candidate("review").unwrap());

    // Bound to a pull request.
    add_task(&path, "pr", "Succeeded");
    add_attempt(&path, "pr", "2");
    sql(
        &path,
        "INSERT INTO pull_requests (task_id, project_id, number, branch, base_branch, state, \
         head_sha, mergeable, required_total, required_passed, required_failed, \
         required_unknown, checks_json, title, observed_ms, changed_ms) \
         VALUES ('pr', 'p1', 7, 'task/pr', 'main', 'open', ?, 'yes', 0, 0, 0, 0, '[]', \
         'Docs', 1, 1)",
        &[&"c".repeat(40)],
    );
    assert!(!store.has_unlanded_candidate("pr").unwrap());

    // A verified commit with no delivery blocks, even without a changed-files count.
    add_task(&path, "verified", "VerificationFailed");
    sql(
        &path,
        "INSERT INTO verification_runs (id, task_id, \"commit\", status) \
         VALUES ('v2', 'verified', ?, 'failed')",
        &[&"d".repeat(40)],
    );
    assert!(store.has_unlanded_candidate("verified").unwrap());

    assert_eq!(
        store
            .archive_tasks(&["empty".to_owned(), "review".to_owned(), "pr".to_owned()])
            .unwrap()
            .len(),
        3
    );
}
