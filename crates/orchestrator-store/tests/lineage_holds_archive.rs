//! Task lineage, held decisions and the project archive (DPES-1105 phase 4d, migration 0045).
//!
//! The store's public API does the writes. A second raw connection reads the tables and tries
//! the writes the database itself must refuse, as another process would. Time is injected: a
//! hold is compared with the `now_ms` the test passes, and no test sleeps.

use std::fs;
use std::path::{Path, PathBuf};

use orchestrator_core::Project;
use orchestrator_store::{
    ArchiveError, HoldError, HoldState, HoldSubject, LineageError, LineageRelation, SqliteStore,
    StoreError,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const DAY_MS: i64 = 86_400_000;
/// 2026-11-17 in the afternoon, UTC.
const NOW_NOV_17: i64 = 1_794_901_798_000;
/// The last millisecond of 2026-11-18, UTC.
const END_NOV_18: i64 = 1_795_046_399_999;
/// The last millisecond of 2026-11-21, UTC.
const END_NOV_21: i64 = END_NOV_18 + 3 * DAY_MS;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection with `binds` bound in order, and reports whether
/// the database took it.
fn try_sql(path: &Path, sql: &str, binds: &[&str]) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        // Test-only helper: every statement passed in is a literal written in this file.
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn sql(path: &Path, statement: &str, binds: &[&str]) {
    try_sql(path, statement, binds).unwrap();
}

/// The first column of every row `statement` returns, as text, read on a second connection.
fn scalars(path: &Path, statement: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(statement)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

fn count(path: &Path, table_sql: &'static str) -> String {
    scalars(path, table_sql).remove(0)
}

fn register(store: &SqliteStore, id: &str) {
    store
        .register_project(&Project {
            id: id.to_owned(),
            root_path: format!("/work/{id}"),
            settings: serde_json::json!({}),
        })
        .unwrap();
}

/// A store with the project `p1` registered. Returns the database path with it.
fn store_with_project(directory: &Path) -> (SqliteStore, PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    register(&store, "p1");
    (store, path)
}

/// A task row with `state` as the store writes it (JSON text, so `"Running"` carries quotes).
fn add_task(path: &Path, id: &str, project: &str, state: &str) {
    sql(
        path,
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES (?, ?, NULL, ?, '{}', 1)",
        &[id, project, &format!("\"{state}\"")],
    );
}

fn add_approval(path: &Path, id: &str, task_id: &str, status: &str) {
    sql(
        path,
        "INSERT INTO approvals (id, task_id, action, status, granted_by) \
         VALUES (?, ?, '{}', ?, NULL)",
        &[id, task_id, status],
    );
}

fn add_conversation(path: &Path) {
    sql(
        path,
        "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        &[],
    );
    sql(
        path,
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        &[],
    );
}

/// A pending proposal. `index` keeps `(turn_id, idx)` unique and seeds the digest.
fn add_proposal(path: &Path, id: &str, index: usize, state: &str) {
    sql(
        path,
        "INSERT INTO action_proposals \
         (id, scope, turn_id, idx, kind, params_json, digest, subject, state) \
         VALUES (?, 'conversation:global', 'turn-1', ?, 'project.remove', '{}', ?, ?, ?)",
        &[
            id,
            &index.to_string(),
            &format!("{:064x}", index + 1),
            &format!("Subject of {id}"),
            state,
        ],
    );
}

fn refusal<T: Copy>(error: &StoreError, read: fn(&StoreError) -> Option<T>) -> Option<T> {
    read(error)
}

/// A hold on the open approvals of task `t1`, ending `END_NOV_21`.
fn hold_t1(store: &SqliteStore, id: &str, date: &str, end: i64) -> orchestrator_store::DecisionHold {
    store
        .set_decision_hold(id, HoldSubject::Approval, "t1", date, end, NOW_NOV_17)
        .unwrap()
}

/// `p1` with the task `t1` and one requested approval.
fn store_with_open_approval(directory: &Path) -> (SqliteStore, PathBuf) {
    let (store, path) = store_with_project(directory);
    add_task(&path, "t1", "p1", "Queued");
    add_approval(&path, "a1", "t1", "requested");
    (store, path)
}

#[test]
fn migration_0045_applies_to_a_0044_style_database_and_keeps_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let version: u32 = entry
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split('_')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        if version < 45 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        let newest: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert!(newest < 45);
        connection.close().await.unwrap();
    });
    sql(
        &path,
        "INSERT INTO projects (id, root_path, settings) VALUES ('old', '/work/old', '{}')",
        &[],
    );
    sql(
        &path,
        "INSERT INTO projects (id, root_path, settings) VALUES ('other', '/work/other', '{}')",
        &[],
    );
    add_task(&path, "t-old", "old", "Running");
    add_task(&path, "t-done", "old", "Succeeded");
    add_approval(&path, "a-old", "t-old", "requested");
    sql(
        &path,
        "INSERT INTO durable_events (task_id, kind, payload) VALUES ('t-old', 'task.transition', '{}')",
        &[],
    );
    add_conversation(&path);
    add_proposal(&path, "proposal-old", 0, "pending");

    let dump = |path: &Path| -> Vec<Vec<String>> {
        [
            "SELECT json_array(id, root_path, settings, archived_at) FROM projects ORDER BY id",
            "SELECT json_array(id, project_id, state, version) FROM tasks ORDER BY id",
            "SELECT json_array(id, task_id, status) FROM approvals ORDER BY id",
            "SELECT json_array(sequence, task_id, kind) FROM durable_events ORDER BY sequence",
            "SELECT json_array(rowid, id, scope, kind, digest, state) FROM action_proposals ORDER BY rowid",
        ]
        .into_iter()
        .map(|statement| scalars(path, statement))
        .collect()
    };
    let before = dump(&path);
    assert!(scalars(
        &path,
        "SELECT name FROM sqlite_master WHERE name IN ('task_lineage', 'decision_holds')"
    )
    .is_empty());

    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 45"
        ),
        vec!["1"]
    );
    assert_eq!(dump(&path), before);
    assert_eq!(count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM task_lineage"), "0");
    assert_eq!(count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM decision_holds"), "0");

    // The new tables work on the migrated rows.
    let link = store
        .add_task_lineage("t-done", "t-old", LineageRelation::RevisedFrom, Some("go on"))
        .unwrap();
    assert_eq!(link.parent_task_id, "t-old");
    let hold = store
        .set_decision_hold("h1", HoldSubject::Proposal, "proposal-old", "2026-11-18", END_NOV_18, NOW_NOV_17)
        .unwrap();
    assert_eq!(hold.state, HoldState::Active);
    assert!(store.archive_project("other").unwrap());
}

#[test]
fn a_hold_hides_until_its_date_and_comes_back_by_the_callers_clock() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_open_approval(directory.path());

    let hold = hold_t1(&store, "h1", "2026-11-21", END_NOV_21);
    assert_eq!(hold.subject_kind, HoldSubject::Approval);
    assert_eq!(hold.subject_id, "t1");
    assert_eq!(hold.until_ms, END_NOV_21);
    assert_eq!(hold.state, HoldState::Active);
    assert_eq!(hold.released_at, None);

    assert_eq!(store.decision_holds_in_force(NOW_NOV_17).unwrap(), vec![hold.clone()]);
    assert_eq!(store.decision_holds_in_force(END_NOV_21 - 1).unwrap().len(), 1);
    // The date passes by the caller's clock alone: nothing is written.
    assert!(store.decision_holds_in_force(END_NOV_21).unwrap().is_empty());
    assert!(store.decision_holds_in_force(END_NOV_21 + DAY_MS).unwrap().is_empty());
    assert_eq!(store.active_decision_holds().unwrap(), vec![hold]);
    assert_eq!(
        scalars(&path, "SELECT json_array(state, released_at) FROM decision_holds"),
        vec![r#"["active",null]"#]
    );
    // A hold never touches the decision it hides.
    assert_eq!(
        scalars(&path, "SELECT status FROM approvals WHERE id = 'a1'"),
        vec!["requested"]
    );
}

#[test]
fn a_hold_can_be_replaced_and_cleared() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_open_approval(directory.path());

    let first = hold_t1(&store, "h1", "2026-11-18", END_NOV_18);
    // The same date again is a repeat: the existing hold comes back, nothing is added.
    assert_eq!(hold_t1(&store, "h-repeat", "2026-11-18", END_NOV_18), first);
    assert_eq!(count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM decision_holds"), "1");

    // A new date releases the old hold and adds a new one, so one stays active.
    let second = hold_t1(&store, "h2", "2026-11-21", END_NOV_21);
    assert_eq!(second.id, "h2");
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(id, state) FROM decision_holds ORDER BY rowid"
        ),
        vec![r#"["h1","released"]"#, r#"["h2","active"]"#]
    );
    assert_eq!(store.decision_hold("h1").unwrap().unwrap().state, HoldState::Released);
    assert!(store.decision_hold("h1").unwrap().unwrap().released_at.is_some());

    // Clearing releases the active hold and returns it; clearing again finds nothing.
    let cleared = store
        .clear_decision_hold(HoldSubject::Approval, "t1")
        .unwrap()
        .unwrap();
    assert_eq!((cleared.id.as_str(), cleared.state), ("h2", HoldState::Released));
    assert!(store.decision_holds_in_force(NOW_NOV_17).unwrap().is_empty());
    assert!(store.active_decision_holds().unwrap().is_empty());
    assert_eq!(store.clear_decision_hold(HoldSubject::Approval, "t1").unwrap(), None);

    // Release by id is idempotent, and a new hold is allowed after a release.
    assert_eq!(store.release_decision_hold("h2").unwrap(), cleared);
    let third = hold_t1(&store, "h3", "2026-11-18", END_NOV_18);
    assert_eq!(third.state, HoldState::Active);
    assert_eq!(
        store.release_decision_hold("h3").unwrap().state,
        HoldState::Released
    );
    let missing = store.release_decision_hold("nope").unwrap_err();
    assert!(matches!(missing, StoreError::RecordNotFound { .. }));
    assert_eq!(count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM decision_holds"), "3");
}

#[test]
fn a_hold_refuses_a_bad_date_a_past_date_and_a_settled_decision() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_open_approval(directory.path());
    let set = |id: &str, kind, subject: &str, date: &str, end: i64, now: i64| {
        store
            .set_decision_hold(id, kind, subject, date, end, now)
            .unwrap_err()
    };
    let reason = |error: StoreError| HoldError::of(&error);

    // The date must be real and agree with the end of its UTC day.
    for (date, end) in [
        ("2026-02-30", END_NOV_18),
        ("2026-13-01", END_NOV_18),
        ("2026-11-1", END_NOV_18),
        ("2026-11-18", END_NOV_18 - 1),
        ("2026-11-19", END_NOV_18),
        ("9999-12-31", 253_402_300_799_999),
    ] {
        let error = set("h", HoldSubject::Approval, "t1", date, end, NOW_NOV_17);
        assert_eq!(reason(error), Some(HoldError::BadDate), "{date}");
    }
    // A day that has already ended by the caller's clock is refused.
    let error = set("h", HoldSubject::Approval, "t1", "2026-11-18", END_NOV_18, END_NOV_18);
    assert_eq!(reason(error), Some(HoldError::NotInFuture));
    let error = set("", HoldSubject::Approval, "t1", "2026-11-18", END_NOV_18, NOW_NOV_17);
    assert_eq!(reason(error), Some(HoldError::BadId));
    // A task with no open approval, and a proposal that is not pending, have nothing to hold.
    let error = set("h", HoldSubject::Approval, "t-none", "2026-11-18", END_NOV_18, NOW_NOV_17);
    assert_eq!(reason(error), Some(HoldError::NothingOpen));
    add_conversation(&path);
    add_proposal(&path, "p-done", 0, "executed");
    add_proposal(&path, "p-open", 1, "pending");
    let error = set("h", HoldSubject::Proposal, "p-done", "2026-11-18", END_NOV_18, NOW_NOV_17);
    assert_eq!(reason(error), Some(HoldError::NothingOpen));
    store
        .set_decision_hold("h-open", HoldSubject::Proposal, "p-open", "2026-11-18", END_NOV_18, NOW_NOV_17)
        .unwrap();
    // An approval that settled after the hold was set cannot be held again once released.
    sql(&path, "UPDATE approvals SET status = 'approved' WHERE id = 'a1'", &[]);
    let error = set("h-late", HoldSubject::Approval, "t1", "2026-11-21", END_NOV_21, NOW_NOV_17);
    assert_eq!(reason(error), Some(HoldError::NothingOpen));
    // An id in use by another hold is refused, and refusing it changes nothing.
    let error = set("h-open", HoldSubject::Proposal, "p-open", "2026-11-21", END_NOV_21, NOW_NOV_17);
    assert_eq!(reason(error), Some(HoldError::IdInUse));
    assert_eq!(store.active_decision_holds().unwrap().len(), 1);
}

#[test]
fn the_database_refuses_a_hold_it_should_not_take_and_any_change_but_release() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_open_approval(directory.path());
    add_task(&path, "t2", "p1", "Queued");
    add_approval(&path, "a2", "t2", "approved");
    hold_t1(&store, "h1", "2026-11-18", END_NOV_18);

    let insert = "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
                  VALUES (?, 'approval', ?, ?, ?, 'user')";
    let end_18 = END_NOV_18.to_string();
    // A settled approval, a second active hold, a bad day end and a bad date are all refused.
    assert!(try_sql(&path, insert, &["x1", "t2", "2026-11-18", &end_18]).is_err());
    assert!(try_sql(&path, insert, &["x2", "t1", "2026-11-18", &end_18]).is_err());
    assert!(try_sql(&path, insert, &["x3", "t1", "2026-11-18", "1794901798000"]).is_err());
    assert!(try_sql(&path, insert, &["x4", "t1", "18 November", &end_18]).is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('x5', 'approval', 't1', '2026-11-18', 4102531199999, 'user')",
        &[]
    )
    .is_err());

    // Only release is an update; nothing is deleted; a released hold stays released.
    assert!(try_sql(&path, "UPDATE decision_holds SET until_ms = until_ms + 86400000", &[]).is_err());
    assert!(try_sql(&path, "UPDATE decision_holds SET subject_id = 't2'", &[]).is_err());
    assert!(try_sql(&path, "DELETE FROM decision_holds", &[]).is_err());
    store.release_decision_hold("h1").unwrap();
    assert!(try_sql(&path, "UPDATE decision_holds SET state = 'active', released_at = NULL", &[]).is_err());
    assert!(try_sql(&path, "DELETE FROM decision_holds", &[]).is_err());
    assert_eq!(
        scalars(&path, "SELECT state FROM decision_holds"),
        vec!["released"]
    );
}

#[test]
fn clearing_history_can_release_the_holds_of_the_proposals_it_scrubs() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_conversation(&path);
    add_proposal(&path, "p1", 0, "pending");
    add_proposal(&path, "p2", 1, "pending");
    for (hold, proposal) in [("h1", "p1"), ("h2", "p2")] {
        store
            .set_decision_hold(hold, HoldSubject::Proposal, proposal, "2026-11-18", END_NOV_18, NOW_NOV_17)
            .unwrap();
    }
    assert_eq!(store.release_proposal_holds(&["p1".to_owned()]).unwrap(), 1);
    assert_eq!(store.release_proposal_holds(&["p1".to_owned()]).unwrap(), 0);
    let active = store.active_decision_holds().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].subject_id, "p2");
}

#[test]
fn a_revise_lineage_has_one_parent_and_no_cycle() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    for id in ["a", "b", "c"] {
        add_task(&path, id, "p1", "Running");
    }
    let lineage_error = |error: StoreError| LineageError::of(&error);

    let link = store
        .add_task_lineage("b", "a", LineageRelation::RevisedFrom, Some("also add tests"))
        .unwrap();
    assert_eq!(
        (link.task_id.as_str(), link.parent_task_id.as_str(), link.relation),
        ("b", "a", LineageRelation::RevisedFrom)
    );
    assert_eq!(link.added_intent.as_deref(), Some("also add tests"));
    // The same link again returns the stored row and writes nothing.
    assert_eq!(
        store
            .add_task_lineage("b", "a", LineageRelation::RevisedFrom, Some("also add tests"))
            .unwrap(),
        link
    );
    assert_eq!(count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM task_lineage"), "1");
    assert_eq!(store.task_lineage("b").unwrap(), Some(link.clone()));
    assert_eq!(store.task_lineage("a").unwrap(), None);

    // One parent only: another parent, relation or intent for `b` is refused.
    for (parent, relation, intent) in [
        ("c", LineageRelation::RevisedFrom, Some("also add tests")),
        ("a", LineageRelation::PromotedFrom, Some("also add tests")),
        ("a", LineageRelation::RevisedFrom, None),
    ] {
        let error = store.add_task_lineage("b", parent, relation, intent).unwrap_err();
        assert_eq!(lineage_error(error), Some(LineageError::AlreadyLinked));
    }
    // No self parent, and no cycle through a chain (c -> b -> a, then a -> c or a -> b).
    let error = store
        .add_task_lineage("a", "a", LineageRelation::RevisedFrom, None)
        .unwrap_err();
    assert_eq!(lineage_error(error), Some(LineageError::SelfParent));
    store
        .add_task_lineage("c", "b", LineageRelation::PromotedFrom, None)
        .unwrap();
    for parent in ["b", "c"] {
        let error = store
            .add_task_lineage("a", parent, LineageRelation::RevisedFrom, None)
            .unwrap_err();
        assert_eq!(lineage_error(error), Some(LineageError::Cycle), "{parent}");
    }
    assert_eq!(store.task_lineage("a").unwrap(), None);
    // Unknown tasks and an over-long intent are refused before anything is written.
    let error = store
        .add_task_lineage("zz", "a", LineageRelation::RevisedFrom, None)
        .unwrap_err();
    assert!(matches!(error, StoreError::RecordNotFound { .. }));
    let long = "x".repeat(2049);
    let error = store
        .add_task_lineage("a", "c", LineageRelation::RevisedFrom, Some(&long))
        .unwrap_err();
    assert_eq!(lineage_error(error), Some(LineageError::IntentTooLong));
    assert_eq!(
        store
            .task_lineage_children("a")
            .unwrap()
            .iter()
            .map(|child| child.task_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
}

#[test]
fn lineage_is_append_only_and_the_old_task_cannot_be_deleted() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "old", "p1", "Running");
    add_task(&path, "new", "p1", "Queued");
    store
        .add_task_lineage("new", "old", LineageRelation::RevisedFrom, None)
        .unwrap();

    assert!(try_sql(&path, "UPDATE task_lineage SET added_intent = 'changed'", &[]).is_err());
    assert!(try_sql(&path, "UPDATE task_lineage SET parent_task_id = 'new', task_id = 'old'", &[]).is_err());
    assert!(try_sql(&path, "DELETE FROM task_lineage", &[]).is_err());
    assert!(try_sql(&path, "DELETE FROM tasks WHERE id = 'old'", &[]).is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO task_lineage (task_id, parent_task_id, relation) VALUES ('old', 'old', 'revised_from')",
        &[]
    )
    .is_err());
    assert_eq!(
        scalars(&path, "SELECT json_array(task_id, parent_task_id, relation) FROM task_lineage"),
        vec![r#"["new","old","revised_from"]"#]
    );
}

#[test]
fn a_project_with_a_running_task_cannot_be_archived() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t-run", "p1", "Running");

    let error = store.archive_project("p1").unwrap_err();
    assert_eq!(ArchiveError::of(&error), Some(ArchiveError::ActiveTasks));
    assert!(!store.project_is_archived("p1").unwrap());
    assert!(store.archived_project_ids().unwrap().is_empty());

    // Queued and verifying work refuse too; a requested approval refuses; settled work does not.
    for state in ["Queued", "Verifying"] {
        sql(&path, "UPDATE tasks SET state = ? WHERE id = 't-run'", &[&format!("\"{state}\"")]);
        let error = store.archive_project("p1").unwrap_err();
        assert_eq!(ArchiveError::of(&error), Some(ArchiveError::ActiveTasks), "{state}");
    }
    sql(&path, "UPDATE tasks SET state = '\"Succeeded\"' WHERE id = 't-run'", &[]);
    add_approval(&path, "a1", "t-run", "requested");
    let error = store.archive_project("p1").unwrap_err();
    assert_eq!(ArchiveError::of(&error), Some(ArchiveError::PendingApprovals));
    sql(&path, "UPDATE approvals SET status = 'rejected' WHERE id = 'a1'", &[]);

    assert!(store.archive_project("p1").unwrap());
    assert!(store.project_is_archived("p1").unwrap());
    let missing = store.archive_project("nope").unwrap_err();
    assert!(matches!(missing, StoreError::RecordNotFound { .. }));
}

#[test]
fn archiving_keeps_every_task_and_history_row() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    register(&store, "p2");
    add_task(&path, "t1", "p1", "Succeeded");
    add_task(&path, "t2", "p1", "Cancelled");
    add_task(&path, "t3", "p2", "Running");
    add_approval(&path, "a1", "t1", "approved");
    sql(
        &path,
        "INSERT INTO durable_events (task_id, kind, payload) VALUES ('t1', 'task.transition', '{}')",
        &[],
    );
    store
        .add_task_lineage("t2", "t1", LineageRelation::RevisedFrom, None)
        .unwrap();
    let rows = |path: &Path| -> Vec<Vec<String>> {
        [
            "SELECT json_array(id, project_id, state, version) FROM tasks ORDER BY id",
            "SELECT json_array(id, task_id, status) FROM approvals ORDER BY id",
            "SELECT json_array(sequence, task_id, kind) FROM durable_events ORDER BY sequence",
            "SELECT json_array(task_id, parent_task_id) FROM task_lineage",
            "SELECT json_array(id, root_path, settings) FROM projects ORDER BY id",
        ]
        .into_iter()
        .map(|statement| scalars(path, statement))
        .collect()
    };
    let before = rows(&path);

    assert!(store.archive_project("p1").unwrap());
    // Archiving again is a repeat: nothing changes and it does not refuse.
    assert!(!store.archive_project("p1").unwrap());
    assert_eq!(rows(&path), before);
    assert_eq!(store.archived_project_ids().unwrap(), vec!["p1".to_owned()]);
    assert_eq!(store.get_project("p1").unwrap().root_path, "/work/p1");
    assert_eq!(store.list_projects().unwrap().len(), 2);
    // The other project is untouched, even with a running task.
    assert!(!store.project_is_archived("p2").unwrap());
    assert_eq!(
        scalars(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM projects WHERE archived_at IS NULL"),
        vec!["1"]
    );
}

#[test]
fn registering_the_same_path_clears_the_archive_marker() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t1", "p1", "Succeeded");

    // Nothing to clear on a project that is not archived.
    assert!(!store.restore_project("p1").unwrap());
    assert!(store.archive_project("p1").unwrap());
    assert_eq!(
        scalars(&path, "SELECT CAST(archived_at IS NOT NULL AS TEXT) FROM projects"),
        vec!["1"]
    );

    assert!(store.restore_project("p1").unwrap());
    assert!(!store.project_is_archived("p1").unwrap());
    assert!(store.archived_project_ids().unwrap().is_empty());
    assert_eq!(
        scalars(&path, "SELECT CAST(archived_at IS NULL AS TEXT) FROM projects"),
        vec!["1"]
    );
    // Registering again repeats harmlessly, the task row stays, and the project can be
    // archived once more.
    assert!(!store.restore_project("p1").unwrap());
    assert_eq!(count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM tasks"), "1");
    assert!(store.archive_project("p1").unwrap());
    let missing = store.restore_project("nope").unwrap_err();
    assert!(matches!(missing, StoreError::RecordNotFound { .. }));
}

#[test]
fn holds_are_one_active_per_subject_and_kind() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_open_approval(directory.path());
    add_conversation(&path);
    // The same id names a task for an approval hold and a proposal for a proposal hold, so
    // the two never share a slot.
    add_proposal(&path, "t1", 0, "pending");

    hold_t1(&store, "h-approval", "2026-11-18", END_NOV_18);
    store
        .set_decision_hold("h-proposal", HoldSubject::Proposal, "t1", "2026-11-18", END_NOV_18, NOW_NOV_17)
        .unwrap();
    assert_eq!(store.active_decision_holds().unwrap().len(), 2);
    assert!(try_sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('dup', 'approval', 't1', '2026-11-18', 1795046399999, 'user')",
        &[]
    )
    .is_err());
}

// Keeps `refusal` honest for any typed refusal the store adds later.
#[test]
fn a_typed_refusal_reads_back_only_from_its_own_kind() {
    let error = StoreError::from(HoldError::BadDate);
    assert_eq!(refusal(&error, HoldError::of), Some(HoldError::BadDate));
    assert_eq!(refusal(&error, LineageError::of), None);
    assert_eq!(refusal(&error, ArchiveError::of), None);
}
