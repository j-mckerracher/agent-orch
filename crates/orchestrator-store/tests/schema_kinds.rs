//! The schema after every migration takes every kind the code knows (DPES-1108 integration).
//!
//! Phases 5 to 8 were built in parallel, and SQLite cannot alter a CHECK, so 0068 and 0069 each
//! rebuild `action_proposals` (and 0069 `wake_records`) with a kind list. A rebuild that started
//! from an older definition would drop a kind another phase added, and the store would then
//! refuse that kind's proposals or wake records at run time. This test applies every migration
//! to an empty database and writes one row of each kind through a raw connection, so the
//! database's own CHECKs are what is tested.

use std::path::Path;

use orchestrator_core::orchestrator_action::ActionKind;
use orchestrator_store::{SqliteStore, WakeKind};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

/// Every wake kind. The match below has no wildcard, so a new kind does not compile until it is
/// listed here too.
const WAKE_KINDS: [WakeKind; 14] = [
    WakeKind::TaskFailed,
    WakeKind::TaskStalled,
    WakeKind::DeliveryWaiting,
    WakeKind::ProposalWaiting,
    WakeKind::SignInNeeded,
    WakeKind::DaemonStart,
    WakeKind::WorkCheck,
    WakeKind::ScoutReportReady,
    WakeKind::AgentFailing,
    WakeKind::CandidateReady,
    WakeKind::WorkerNeedsDecision,
    WakeKind::WorkerBlocked,
    WakeKind::RoutedRequest,
    WakeKind::CoordinatorReport,
];

#[allow(dead_code)]
fn listed(kind: WakeKind) {
    match kind {
        WakeKind::TaskFailed
        | WakeKind::TaskStalled
        | WakeKind::DeliveryWaiting
        | WakeKind::ProposalWaiting
        | WakeKind::SignInNeeded
        | WakeKind::DaemonStart
        | WakeKind::WorkCheck
        | WakeKind::ScoutReportReady
        | WakeKind::AgentFailing
        | WakeKind::CandidateReady
        | WakeKind::WorkerNeedsDecision
        | WakeKind::WorkerBlocked
        | WakeKind::RoutedRequest
        | WakeKind::CoordinatorReport => {}
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement with text binds on a second connection, as another process would.
fn run(path: &Path, sql: &'static str, binds: &[&str]) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        let mut query = sqlx::query(sql);
        for bind in binds {
            query = query.bind(*bind);
        }
        query.execute(&mut connection).await?;
        Ok(())
    })
}

fn columns(path: &Path, table: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new().filename(path);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info(?)")
            .bind(table)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

#[test]
fn every_migration_applied_to_an_empty_database_takes_every_action_and_wake_kind() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    drop(SqliteStore::new(&path).unwrap());

    for statement in [
        "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-1', 'conversation:global', 'completed', 'rt', '1.0.0')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-2', 'conversation:global', 'completed', 'rt', '1.0.0')",
    ] {
        run(&path, statement, &[]).unwrap();
    }

    // One proposal of each kind core parses; a turn holds at most ten (`idx BETWEEN 0 AND 9`).
    for (position, kind) in ActionKind::ALL.into_iter().enumerate() {
        let (id, turn) = (
            format!("proposal-{position}"),
            if position < 10 { "turn-1" } else { "turn-2" },
        );
        let (idx, digest) = ((position % 10).to_string(), format!("{position:064x}"));
        run(
            &path,
            "INSERT INTO action_proposals \
             (id, scope, turn_id, idx, kind, params_json, digest, subject) \
             VALUES (?, 'conversation:global', ?, CAST(? AS INTEGER), ?, '{}', ?, 's')",
            &[&id, turn, &idx, kind.as_str(), &digest],
        )
        .unwrap_or_else(|error| panic!("{}: {error}", kind.as_str()));
    }
    assert!(run(
        &path,
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('unknown', 'conversation:global', 'turn-2', 9, 'task.delete', '{}', ?, 's')",
        &[&format!("{:064x}", 99)],
    )
    .is_err());

    // One wake record of each kind the store knows.
    for kind in WAKE_KINDS {
        assert_eq!(WakeKind::parse(kind.as_str()), Some(kind));
        let needs_model = if kind.needs_model() { "1" } else { "0" };
        let state = if kind.needs_model() {
            "pending"
        } else {
            "noticed"
        };
        run(
            &path,
            "INSERT INTO wake_records \
             (id, scope, event_kind, subject, dedupe_key, notice, needs_model, state) \
             VALUES (?, 'conversation:global', ?, 'subject', ?, 'A notice.', \
                     CAST(? AS INTEGER), ?)",
            &[
                &format!("wake-{}", kind.as_str()),
                kind.as_str(),
                &format!("key:{}", kind.as_str()),
                needs_model,
                state,
            ],
        )
        .unwrap_or_else(|error| panic!("{}: {error}", kind.as_str()));
    }

    // Every phase's columns survive the rebuilds.
    assert!(columns(&path, "conversation_turns").contains(&"pull_request_links".to_owned()));
    let tasks = columns(&path, "tasks");
    for column in ["coordinator_project_id", "routed_request_id"] {
        assert!(tasks.contains(&column.to_owned()), "{column}");
    }
    assert!(columns(&path, "orchestrator_settings").contains(&"autonomous_actions".to_owned()));
    let proposals = columns(&path, "action_proposals");
    for column in ["denial", "grant_tried"] {
        assert!(proposals.contains(&column.to_owned()), "{column}");
    }
}
