//! Pull requests and the merge ledger (DPES-1105 phase 4c, migration 0051).
//!
//! The store's public API does the writes. A second raw connection reads the tables and tries
//! the writes the database itself must refuse, as another process would. Time is injected and
//! no test sleeps.

use std::fs;
use std::path::{Path, PathBuf};

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::landing::{
    merge_verdict, CheckState, MergeMethod, MergeRefusal, PrState, FRESH_MAX_AGE_MS,
};
use orchestrator_core::project_mode::ProjectMode;
use orchestrator_core::records::Project;
use orchestrator_store::{
    MergeClaim, MergeState, PullRequestChange, PullRequestError, PullRequestSeen, SqliteStore,
    Store, StoreError, StoredCheck,
};
use serde_json::json;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const NOW: i64 = 1_794_901_798_000;

fn head(digit: char) -> String {
    digit.to_string().repeat(40)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection, as another process would.
fn try_sql(path: &Path, sql: &'static str) -> Result<u64, sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        Ok(sqlx::query(sql)
            .execute(&mut connection)
            .await?
            .rows_affected())
    })
}

fn scalars(path: &Path, sql: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

fn contract(id: &str, project: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: project.into(),
        parent_id: None,
        objective: format!("ship {id}"),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::PullRequest,
        autonomy: AutonomyPolicy::ApprovalRequired,
        max_retries: 0,
        max_repairs: 0,
        model_selection: None,
        permission: Default::default(),
    }
}

/// A store with projects p1 and p2, tasks t1 and t2 in p1 and t3 in p2.
fn seeded(directory: &Path) -> (SqliteStore, PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    for id in ["p1", "p2"] {
        store
            .register_project(&Project {
                id: id.into(),
                root_path: format!("/work/{id}"),
                settings: json!({}),
            })
            .unwrap();
    }
    for (task, project) in [("t1", "p1"), ("t2", "p1"), ("t3", "p2")] {
        store.create(&contract(task, project)).unwrap();
    }
    (store, path)
}

fn check(name: &str, state: CheckState) -> StoredCheck {
    StoredCheck {
        name: name.to_owned(),
        state,
    }
}

fn seen(task: &str, project: &str, number: u64) -> PullRequestSeen {
    PullRequestSeen {
        task_id: task.to_owned(),
        project_id: project.to_owned(),
        number,
        branch: format!("moa/{task}"),
        base_branch: "main".to_owned(),
        state: PrState::Open,
        head_sha: head('a'),
        mergeable: Some(true),
        checks: vec![check("build", CheckState::Passed)],
        title: "Fix it".to_owned(),
    }
}

fn event_count(path: &Path) -> String {
    scalars(
        path,
        "SELECT CAST(COUNT(*) AS TEXT) FROM durable_events WHERE kind = 'pr.changed'",
    )
    .remove(0)
}

fn refused_with(error: &StoreError) -> Option<PullRequestError> {
    PullRequestError::of(error)
}

fn claim(store: &SqliteStore, proposal: &str, task: &str, number: u64) -> MergeClaim {
    store
        .claim_pull_request_merge(proposal, task, number, &head('a'), MergeMethod::Squash)
        .unwrap()
}

#[test]
fn pr_migration_0051_applies_to_a_0050_database_and_keeps_every_row() {
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
        if version <= 50 {
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
        assert_eq!(newest, 50);
        sqlx::query(
            "INSERT INTO projects (id, root_path, settings) VALUES ('old', '/work/old', '{}')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        connection.close().await.unwrap();
    });

    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 51"
        ),
        vec!["1"]
    );
    assert_eq!(
        scalars(&path, "SELECT json_array(id, root_path) FROM projects"),
        vec![r#"["old","/work/old"]"#]
    );
    assert_eq!(
        scalars(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM pull_requests"),
        vec!["0"]
    );
    assert_eq!(store.pull_request("anything").unwrap(), None);
    assert!(store.open_pull_request_merges().unwrap().is_empty());
}

#[test]
fn pr_first_observation_creates_the_row_and_one_event_with_the_task_id_only() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());

    let change = store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    assert_eq!(change, PullRequestChange::Created);

    let row = store.pull_request("t1").unwrap().unwrap();
    assert_eq!((row.number, row.state), (5, PrState::Open));
    assert_eq!(row.head_sha, head('a'));
    assert_eq!(row.branch, "moa/t1");
    assert_eq!(row.base_branch, "main");
    assert_eq!(row.mergeable, Some(true));
    assert_eq!(
        (row.required_total, row.required_passed, row.required_failed),
        (1, 1, 0)
    );
    assert_eq!(row.checks, vec![check("build", CheckState::Passed)]);
    assert_eq!(
        (row.observed_ms, row.changed_ms, row.poll_failures),
        (NOW, NOW, 0)
    );
    assert_eq!(store.pull_request("t2").unwrap(), None);
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(task_id, payload) FROM durable_events WHERE kind = 'pr.changed'"
        ),
        vec![r#"["t1","{\"task_id\":\"t1\"}"]"#]
    );
}

#[test]
fn pr_an_unchanged_read_records_the_time_without_an_event_or_a_new_change_time() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    store.record_pull_request_poll_failure("t1").unwrap();

    let mut again = seen("t1", "p1", 5);
    again.title = "Fix it, reworded".to_owned();
    let change = store.observe_pull_request(&again, NOW + 120_000).unwrap();
    assert_eq!(change, PullRequestChange::Unchanged);

    let row = store.pull_request("t1").unwrap().unwrap();
    assert_eq!(row.observed_ms, NOW + 120_000);
    assert_eq!(row.changed_ms, NOW);
    assert_eq!(row.poll_failures, 0);
    assert_eq!(row.title, "Fix it, reworded");
    assert_eq!(event_count(&path), "1");
}

#[test]
fn pr_a_changed_head_state_base_checks_or_mergeable_appends_an_event() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();

    let variants: [fn(&mut PullRequestSeen); 4] = [
        |s| s.head_sha = head('b'),
        |s| s.base_branch = "develop".to_owned(),
        |s| s.mergeable = None,
        |s| s.checks = vec![check("build", CheckState::Failed)],
    ];
    let mut expected_events = 1;
    for (index, change) in variants.into_iter().enumerate() {
        let mut next = seen("t1", "p1", 5);
        change(&mut next);
        let at = NOW + 1_000 * (index as i64 + 1);
        assert_eq!(
            store.observe_pull_request(&next, at).unwrap(),
            PullRequestChange::Changed
        );
        expected_events += 1;
        assert_eq!(store.pull_request("t1").unwrap().unwrap().changed_ms, at);
        // Restore the baseline so each variant is judged against the same row.
        assert_eq!(
            store
                .observe_pull_request(&seen("t1", "p1", 5), at + 1)
                .unwrap(),
            PullRequestChange::Changed
        );
        expected_events += 1;
    }
    assert_eq!(event_count(&path), expected_events.to_string());
}

#[test]
fn pr_a_merged_or_closed_row_is_final_and_a_later_observation_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    store
        .observe_pull_request(&seen("t2", "p1", 6), NOW)
        .unwrap();

    let mut merged = seen("t1", "p1", 5);
    merged.state = PrState::Merged;
    assert_eq!(
        store.observe_pull_request(&merged, NOW + 1).unwrap(),
        PullRequestChange::Changed
    );
    let mut closed = seen("t2", "p1", 6);
    closed.state = PrState::Closed;
    store.observe_pull_request(&closed, NOW + 1).unwrap();

    let events = event_count(&path);
    for (task, number) in [("t1", 5), ("t2", 6)] {
        let before = store.pull_request(task).unwrap().unwrap();
        // The pull request reads open again, or with a new head: ignored.
        let mut reopened = seen(task, "p1", number);
        reopened.head_sha = head('c');
        assert_eq!(
            store.observe_pull_request(&reopened, NOW + 9).unwrap(),
            PullRequestChange::Settled
        );
        assert_eq!(store.pull_request(task).unwrap().unwrap(), before);
        assert_eq!(store.record_pull_request_poll_failure(task).unwrap(), None);
    }
    assert_eq!(event_count(&path), events);
    assert_eq!(
        store.pull_request("t1").unwrap().unwrap().state,
        PrState::Merged
    );
}

#[test]
fn pr_observation_refuses_bad_input_and_foreign_targets() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();

    let refuse = |change: fn(&mut PullRequestSeen), at: i64| {
        let mut bad = seen("t2", "p1", 6);
        change(&mut bad);
        store
            .observe_pull_request(&bad, at)
            .map_err(|error| refused_with(&error))
    };
    assert_eq!(
        refuse(|s| s.number = 0, NOW),
        Err(Some(PullRequestError::BadNumber))
    );
    assert_eq!(
        refuse(|s| s.number = 1_000_000, NOW),
        Err(Some(PullRequestError::BadNumber))
    );
    assert_eq!(
        refuse(|s| s.head_sha = head('A'), NOW),
        Err(Some(PullRequestError::BadSha))
    );
    assert_eq!(
        refuse(|s| s.head_sha = head('a')[..39].to_owned(), NOW),
        Err(Some(PullRequestError::BadSha))
    );
    assert_eq!(
        refuse(|s| s.branch = "a\nb".to_owned(), NOW),
        Err(Some(PullRequestError::BadBranch))
    );
    assert_eq!(
        refuse(|s| s.base_branch = String::new(), NOW),
        Err(Some(PullRequestError::BadBranch))
    );
    assert_eq!(
        refuse(|s| s.branch = "b".repeat(256), NOW),
        Err(Some(PullRequestError::BadBranch))
    );
    assert_eq!(refuse(|_| {}, 0), Err(Some(PullRequestError::BadTime)));
    assert_eq!(
        refuse(
            |s| s.checks = (0..101)
                .map(|i| check(&format!("c{i}"), CheckState::Passed))
                .collect(),
            NOW
        ),
        Err(Some(PullRequestError::TooManyChecks))
    );
    // t2 is in p1, not p2.
    assert_eq!(
        refuse(|s| s.project_id = "p2".to_owned(), NOW),
        Err(Some(PullRequestError::ProjectMismatch))
    );
    // Number 5 is t1's in p1.
    assert_eq!(
        refuse(|s| s.number = 5, NOW),
        Err(Some(PullRequestError::NumberTaken))
    );
    // t1 already follows number 5.
    let mut other = seen("t1", "p1", 7);
    other.head_sha = head('d');
    assert_eq!(
        refused_with(&store.observe_pull_request(&other, NOW).unwrap_err()),
        Some(PullRequestError::Retargeted)
    );
    assert!(matches!(
        store.observe_pull_request(&seen("nope", "p1", 8), NOW),
        Err(StoreError::RecordNotFound { entity: "task", .. })
    ));
    assert_eq!(store.pull_request("t2").unwrap(), None);
    assert_eq!(store.pull_request("t1").unwrap().unwrap().number, 5);
}

#[test]
fn pr_title_and_check_names_are_cleaned_and_capped_before_they_are_stored() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    let mut remote = seen("t1", "p1", 5);
    remote.title = format!("a\u{1b}[31mb\n{}", "t".repeat(500));
    remote.checks = vec![check(
        &format!("x\u{7}{}", "n".repeat(300)),
        CheckState::Pending,
    )];
    store.observe_pull_request(&remote, NOW).unwrap();

    let row = store.pull_request("t1").unwrap().unwrap();
    assert_eq!(row.title.chars().count(), 200);
    assert!(row.title.starts_with("a[31mb"));
    assert!(!row.title.chars().any(char::is_control));
    assert_eq!(row.checks[0].name.chars().count(), 80);
    assert!(!row.checks[0].name.chars().any(char::is_control));
}

#[test]
fn pr_check_counts_feed_the_merge_rule_and_unknown_never_passes() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    let mut mixed = seen("t1", "p1", 5);
    mixed.checks = vec![
        check("a", CheckState::Passed),
        check("b", CheckState::Failed),
        check("c", CheckState::Pending),
        check("d", CheckState::Unknown),
    ];
    store.observe_pull_request(&mixed, NOW).unwrap();
    let row = store.pull_request("t1").unwrap().unwrap();
    assert_eq!(
        (
            row.required_total,
            row.required_passed,
            row.required_failed,
            row.required_unknown
        ),
        (4, 1, 1, 1)
    );

    let verdict = |row: &orchestrator_store::PullRequestRow| {
        merge_verdict(
            &head('a'),
            &row.observation(),
            ProjectMode::NoMistakes,
            MergeMethod::Squash,
            MergeMethod::Squash,
            NOW + 1_000,
            FRESH_MAX_AGE_MS,
        )
    };
    assert_eq!(verdict(&row), Err(MergeRefusal::ChecksRed));

    let mut unknown_only = seen("t1", "p1", 5);
    unknown_only.checks = vec![
        check("a", CheckState::Passed),
        check("d", CheckState::Unknown),
    ];
    store.observe_pull_request(&unknown_only, NOW).unwrap();
    assert_eq!(
        verdict(&store.pull_request("t1").unwrap().unwrap()),
        Err(MergeRefusal::ChecksUnknown)
    );

    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    assert_eq!(verdict(&store.pull_request("t1").unwrap().unwrap()), Ok(()));

    let mut none = seen("t1", "p1", 5);
    none.checks = Vec::new();
    store.observe_pull_request(&none, NOW).unwrap();
    assert_eq!(
        verdict(&store.pull_request("t1").unwrap().unwrap()),
        Err(MergeRefusal::ChecksMissing)
    );
}

#[test]
fn pr_poll_failures_count_up_and_a_good_read_resets_them() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    assert_eq!(store.record_pull_request_poll_failure("t1").unwrap(), None);
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    assert_eq!(
        store.record_pull_request_poll_failure("t1").unwrap(),
        Some(1)
    );
    assert_eq!(
        store.record_pull_request_poll_failure("t1").unwrap(),
        Some(2)
    );
    assert_eq!(store.pull_request("t1").unwrap().unwrap().poll_failures, 2);
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW + 1)
        .unwrap();
    assert_eq!(store.pull_request("t1").unwrap().unwrap().poll_failures, 0);
}

#[test]
fn pr_list_filters_by_project_and_orders_by_the_latest_change() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    store
        .observe_pull_request(&seen("t2", "p1", 6), NOW + 10)
        .unwrap();
    store
        .observe_pull_request(&seen("t3", "p2", 5), NOW + 5)
        .unwrap();

    let ids = |project: Option<&str>, limit: usize| -> Vec<String> {
        store
            .list_pull_requests(project, limit)
            .unwrap()
            .into_iter()
            .map(|row| row.task_id)
            .collect()
    };
    assert_eq!(ids(None, 10), ["t2", "t3", "t1"]);
    assert_eq!(ids(Some("p1"), 10), ["t2", "t1"]);
    assert_eq!(ids(Some("p2"), 10), ["t3"]);
    assert_eq!(ids(None, 1), ["t2"]);
    assert!(ids(Some("none"), 10).is_empty());
    assert!(ids(None, 0).is_empty());
}

#[test]
fn pr_the_database_refuses_target_changes_bad_values_and_edits_of_a_settled_row() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();

    for sql in [
        "UPDATE pull_requests SET number = 6 WHERE task_id = 't1'",
        "UPDATE pull_requests SET branch = 'x' WHERE task_id = 't1'",
        "UPDATE pull_requests SET project_id = 'p2' WHERE task_id = 't1'",
        "UPDATE pull_requests SET task_id = 't2' WHERE task_id = 't1'",
        "UPDATE pull_requests SET head_sha = 'ABC' WHERE task_id = 't1'",
        "UPDATE pull_requests SET mergeable = 'maybe' WHERE task_id = 't1'",
        "UPDATE pull_requests SET state = 'draft' WHERE task_id = 't1'",
        "UPDATE pull_requests SET required_passed = 9 WHERE task_id = 't1'",
        "UPDATE pull_requests SET checks_json = 'not json' WHERE task_id = 't1'",
        "UPDATE pull_requests SET title = 'a' || char(10) || 'b' WHERE task_id = 't1'",
        "UPDATE pull_requests SET observed_ms = 0 WHERE task_id = 't1'",
    ] {
        assert!(try_sql(&path, sql).is_err(), "{sql}");
    }
    assert_eq!(
        try_sql(
            &path,
            "UPDATE pull_requests SET state = 'merged' WHERE task_id = 't1'"
        )
        .unwrap(),
        1
    );
    for sql in [
        "UPDATE pull_requests SET poll_failures = 3 WHERE task_id = 't1'",
        "UPDATE pull_requests SET state = 'open' WHERE task_id = 't1'",
        "UPDATE pull_requests SET head_sha = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE task_id = 't1'",
    ] {
        assert!(try_sql(&path, sql).is_err(), "{sql}");
    }
}

#[test]
fn pr_a_second_claim_of_one_proposal_never_claims_twice() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();

    let MergeClaim::Claimed(first) = claim(&store, "prop-1", "t1", 5) else {
        panic!("the first claim must record the attempt");
    };
    assert_eq!(first.state, MergeState::Claimed);
    assert_eq!((first.number, first.method), (5, MergeMethod::Squash));
    assert_eq!(first.head_sha, head('a'));
    assert_eq!((first.refusal, first.merge_sha), (None, None));

    for _ in 0..3 {
        match claim(&store, "prop-1", "t1", 5) {
            MergeClaim::Existing(again) => assert_eq!(again.proposal_id, "prop-1"),
            other => panic!("a repeat claim must not claim: {other:?}"),
        }
    }
    // Even after the attempt moved on, the proposal's claim stays taken.
    store
        .advance_pull_request_merge("prop-1", MergeState::Refused, Some("checks_red"), None)
        .unwrap();
    assert!(matches!(
        claim(&store, "prop-1", "t1", 5),
        MergeClaim::Existing(attempt) if attempt.state == MergeState::Refused
    ));
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM pull_request_merges"
        ),
        vec!["1"]
    );
}

#[test]
fn landing_two_callers_on_one_claim_call_the_host_once() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    assert!(matches!(
        claim(&store, "prop-1", "t1", 5),
        MergeClaim::Claimed(_)
    ));

    // Only the first caller moves the row, so only it may call the host.
    assert!(store.issue_pull_request_merge("prop-1").unwrap());
    assert!(!store.issue_pull_request_merge("prop-1").unwrap());
    assert_eq!(
        store.pull_request_merge("prop-1").unwrap().unwrap().state,
        MergeState::Issued
    );
    // A settled attempt is never moved back to issued.
    store
        .advance_pull_request_merge("prop-1", MergeState::Merged, None, None)
        .unwrap();
    assert!(!store.issue_pull_request_merge("prop-1").unwrap());
    assert_eq!(
        store.pull_request_merge("prop-1").unwrap().unwrap().state,
        MergeState::Merged
    );
    assert!(store.issue_pull_request_merge("no-such-proposal").is_err());
}

#[test]
fn pr_an_open_attempt_blocks_a_claim_by_another_proposal_on_the_same_pull_request() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    store
        .observe_pull_request(&seen("t2", "p1", 6), NOW)
        .unwrap();

    assert!(matches!(
        claim(&store, "prop-1", "t1", 5),
        MergeClaim::Claimed(_)
    ));
    // Claimed, issued and unknown each block another proposal; another pull request is free.
    for state in [MergeState::Claimed, MergeState::Issued, MergeState::Unknown] {
        match state {
            MergeState::Issued => {
                store
                    .advance_pull_request_merge("prop-1", MergeState::Issued, None, None)
                    .unwrap();
            }
            MergeState::Unknown => {
                store
                    .advance_pull_request_merge("prop-1", MergeState::Unknown, None, None)
                    .unwrap();
            }
            _ => {}
        }
        match claim(&store, "prop-2", "t1", 5) {
            MergeClaim::Busy(blocking) => {
                assert_eq!(blocking.proposal_id, "prop-1");
                assert_eq!(blocking.state, state);
            }
            other => panic!("an open attempt must block: {other:?}"),
        }
        assert_eq!(store.pull_request_merge("prop-2").unwrap(), None);
    }
    assert!(matches!(
        claim(&store, "prop-3", "t2", 6),
        MergeClaim::Claimed(_)
    ));

    // Once the first attempt is refused the pull request can be tried again.
    store
        .advance_pull_request_merge("prop-1", MergeState::Refused, Some("unconfirmed"), None)
        .unwrap();
    assert!(matches!(
        claim(&store, "prop-2", "t1", 5),
        MergeClaim::Claimed(_)
    ));
}

#[test]
fn pr_the_merge_state_machine_has_only_the_plan_edges() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    let states = [
        MergeState::Claimed,
        MergeState::Issued,
        MergeState::Merged,
        MergeState::Refused,
        MergeState::Unknown,
    ];
    let mut index = 0_u64;
    for from in states {
        for to in states {
            index += 1;
            // One task and pull request per pair, so open attempts never block each other.
            let task = format!("edge-{index}");
            store.create(&contract(&task, "p1")).unwrap();
            store
                .observe_pull_request(&seen(&task, "p1", 100 + index), NOW)
                .unwrap();
            let proposal = format!("prop-{index}");
            store
                .claim_pull_request_merge(
                    &proposal,
                    &task,
                    100 + index,
                    &head('a'),
                    MergeMethod::Squash,
                )
                .unwrap();
            let step = |state: MergeState| {
                let refusal = (state == MergeState::Refused).then_some("reached");
                store.advance_pull_request_merge(&proposal, state, refusal, None)
            };
            // Walk to `from` along legal edges.
            match from {
                MergeState::Claimed => {}
                MergeState::Issued => {
                    step(MergeState::Issued).unwrap();
                }
                MergeState::Merged => {
                    step(MergeState::Issued).unwrap();
                    step(MergeState::Merged).unwrap();
                }
                MergeState::Refused => {
                    step(MergeState::Refused).unwrap();
                }
                MergeState::Unknown => {
                    step(MergeState::Issued).unwrap();
                    step(MergeState::Unknown).unwrap();
                }
            }
            let outcome = step(to);
            if from == to {
                assert_eq!(outcome.unwrap().state, from, "{from:?} to itself");
            } else if from.can_become(to) {
                assert_eq!(outcome.unwrap().state, to, "{from:?} to {to:?}");
            } else {
                let error = outcome.unwrap_err();
                assert_eq!(
                    refused_with(&error),
                    Some(PullRequestError::IllegalTransition),
                    "{from:?} to {to:?}"
                );
                assert_eq!(
                    store.pull_request_merge(&proposal).unwrap().unwrap().state,
                    from
                );
            }
        }
    }
    // The plan's edges, spelled out.
    let edges: Vec<_> = states
        .into_iter()
        .flat_map(|from| states.into_iter().map(move |to| (from, to)))
        .filter(|(from, to)| from.can_become(*to))
        .collect();
    assert_eq!(
        edges,
        [
            (MergeState::Claimed, MergeState::Issued),
            (MergeState::Claimed, MergeState::Refused),
            (MergeState::Issued, MergeState::Merged),
            (MergeState::Issued, MergeState::Refused),
            (MergeState::Issued, MergeState::Unknown),
            (MergeState::Unknown, MergeState::Merged),
            (MergeState::Unknown, MergeState::Refused),
        ]
    );
}

#[test]
fn pr_refusal_codes_and_merge_commits_follow_the_state() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    claim(&store, "prop-1", "t1", 5);

    let bad_refusal = |to: MergeState, refusal: Option<&str>| {
        refused_with(
            &store
                .advance_pull_request_merge("prop-1", to, refusal, None)
                .unwrap_err(),
        )
    };
    let wanted = Some(PullRequestError::BadRefusal);
    assert_eq!(bad_refusal(MergeState::Refused, None), wanted);
    assert_eq!(bad_refusal(MergeState::Refused, Some("")), wanted);
    assert_eq!(bad_refusal(MergeState::Refused, Some("Checks Red")), wanted);
    let long = "a".repeat(65);
    assert_eq!(
        bad_refusal(MergeState::Refused, Some(long.as_str())),
        wanted
    );
    assert_eq!(bad_refusal(MergeState::Issued, Some("checks_red")), wanted);

    let sha = head('f');
    let upper = sha.to_uppercase();
    let bad_sha = |to: MergeState, sha: Option<&str>| {
        refused_with(
            &store
                .advance_pull_request_merge("prop-1", to, None, sha)
                .unwrap_err(),
        )
    };
    assert_eq!(
        bad_sha(MergeState::Issued, Some(sha.as_str())),
        Some(PullRequestError::BadMergeSha)
    );
    assert_eq!(
        bad_sha(MergeState::Merged, Some("short")),
        Some(PullRequestError::BadMergeSha)
    );
    assert_eq!(
        bad_sha(MergeState::Merged, Some(upper.as_str())),
        Some(PullRequestError::BadMergeSha)
    );

    store
        .advance_pull_request_merge("prop-1", MergeState::Issued, None, None)
        .unwrap();
    let merged = store
        .advance_pull_request_merge("prop-1", MergeState::Merged, None, Some(sha.as_str()))
        .unwrap();
    assert_eq!(merged.state, MergeState::Merged);
    assert_eq!(merged.merge_sha.as_deref(), Some(sha.as_str()));
    assert_eq!(merged.refusal, None);

    assert!(matches!(
        store.advance_pull_request_merge("nope", MergeState::Issued, None, None),
        Err(StoreError::RecordNotFound {
            entity: "pull request merge",
            ..
        })
    ));
}

#[test]
fn pr_a_claim_needs_the_stored_pull_request_and_a_valid_target() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();

    let attempt = |proposal: &str, task: &str, number: u64, sha: &str| {
        store.claim_pull_request_merge(proposal, task, number, sha, MergeMethod::Squash)
    };
    // No pull request for t2, and t1's is number 5, not 6.
    for (task, number) in [("t2", 6), ("t1", 6), ("nope", 5)] {
        assert!(matches!(
            attempt("p", task, number, &head('a')),
            Err(StoreError::RecordNotFound {
                entity: "pull request",
                ..
            })
        ));
    }
    let wanted = |error: StoreError| refused_with(&error);
    assert_eq!(
        attempt("p", "t1", 0, &head('a')).map_err(wanted),
        Err(Some(PullRequestError::BadNumber))
    );
    assert_eq!(
        attempt("p", "t1", 5, "abc").map_err(wanted),
        Err(Some(PullRequestError::BadSha))
    );
    assert_eq!(
        attempt("", "t1", 5, &head('a')).map_err(wanted),
        Err(Some(PullRequestError::BadProposal))
    );
    assert_eq!(
        attempt(&"p".repeat(129), "t1", 5, &head('a')).map_err(wanted),
        Err(Some(PullRequestError::BadProposal))
    );
    assert!(store.open_pull_request_merges().unwrap().is_empty());
}

#[test]
fn pr_open_attempts_are_listed_for_the_restart_reconcile_and_the_latest_is_found() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    store
        .observe_pull_request(&seen("t2", "p1", 6), NOW)
        .unwrap();
    assert_eq!(store.latest_pull_request_merge("t1").unwrap(), None);

    claim(&store, "prop-1", "t1", 5);
    store
        .advance_pull_request_merge("prop-1", MergeState::Refused, Some("checks_red"), None)
        .unwrap();
    claim(&store, "prop-2", "t1", 5);
    claim(&store, "prop-3", "t2", 6);
    store
        .advance_pull_request_merge("prop-3", MergeState::Issued, None, None)
        .unwrap();

    let open: Vec<_> = store
        .open_pull_request_merges()
        .unwrap()
        .into_iter()
        .map(|attempt| (attempt.proposal_id, attempt.state))
        .collect();
    assert_eq!(
        open,
        [
            ("prop-2".to_owned(), MergeState::Claimed),
            ("prop-3".to_owned(), MergeState::Issued)
        ]
    );
    assert_eq!(
        store
            .latest_pull_request_merge("t1")
            .unwrap()
            .unwrap()
            .proposal_id,
        "prop-2"
    );
    assert_eq!(
        store
            .latest_pull_request_merge("t2")
            .unwrap()
            .unwrap()
            .state,
        MergeState::Issued
    );
}

#[test]
fn pr_the_ledger_triggers_hold_from_another_connection() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();
    claim(&store, "m1", "t1", 5);

    for sql in [
        // claimed -> merged skips issued.
        "UPDATE pull_request_merges SET state = 'merged' WHERE proposal_id = 'm1'",
        // refused needs its code.
        "UPDATE pull_request_merges SET state = 'refused' WHERE proposal_id = 'm1'",
        // a merge commit only with merged.
        "UPDATE pull_request_merges SET merge_sha = 'cccccccccccccccccccccccccccccccccccccccc' WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET number = 6 WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET task_id = 't2' WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET method = 'merge' WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET head_sha = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET proposal_id = 'm2' WHERE proposal_id = 'm1'",
        "DELETE FROM pull_request_merges WHERE proposal_id = 'm1'",
        "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
         VALUES ('bad-1', 't1', 5, 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', 'squash', 'refused')",
        "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
         VALUES ('bad-2', 't2', 6, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'admin', 'claimed')",
        "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
         VALUES ('bad-3', 't2', 6, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'squash', 'refused')",
        // a second open attempt on the same pull request.
        "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
         VALUES ('bad-4', 't1', 5, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'squash', 'claimed')",
    ] {
        assert!(try_sql(&path, sql).is_err(), "{sql}");
    }
    assert_eq!(
        scalars(&path, "SELECT state FROM pull_request_merges"),
        vec!["claimed"]
    );

    // Legal edges work, and a settled attempt then stays as it is.
    assert_eq!(
        try_sql(
            &path,
            "UPDATE pull_request_merges SET state = 'refused', refusal = 'checks_red' \
             WHERE proposal_id = 'm1'"
        )
        .unwrap(),
        1
    );
    for sql in [
        "UPDATE pull_request_merges SET refusal = 'other' WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET state = 'issued', refusal = NULL WHERE proposal_id = 'm1'",
        "UPDATE pull_request_merges SET updated_at = 'x' WHERE proposal_id = 'm1'",
    ] {
        assert!(try_sql(&path, sql).is_err(), "{sql}");
    }
}

#[test]
fn pr_a_pull_request_has_at_most_one_merged_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .observe_pull_request(&seen("t1", "p1", 5), NOW)
        .unwrap();

    assert_eq!(
        try_sql(
            &path,
            "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
             VALUES ('m1', 't1', 5, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'squash', 'merged')"
        )
        .unwrap(),
        1
    );
    assert!(try_sql(
        &path,
        "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
         VALUES ('m2', 't1', 5, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'squash', 'merged')"
    )
    .is_err());
    // A claim on a merged pull request is not blocked by the index; the merge worker's fresh
    // read refuses it as not open. It still has its own, single, claim.
    assert!(matches!(
        claim(&store, "m3", "t1", 5),
        MergeClaim::Claimed(_)
    ));
    assert!(matches!(
        claim(&store, "m3", "t1", 5),
        MergeClaim::Existing(_)
    ));
}
