//! The away window and the return brief counts (DPES-1105 phase 5b, migration 0048).
//!
//! The store's public API does the reads and the mode writes. A second raw connection seeds the
//! rows the daemon would have written and reads the columns back. Time is injected: no test
//! reads a clock or sleeps.

use std::path::Path;

use orchestrator_core::supervision::MonitoringMode;
use orchestrator_store::SqliteStore;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

/// 2027-01-15 08:00:00 UTC.
const FROM: u64 = 1_800_000_000_000;
const HOUR: u64 = 3_600_000;
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const INSIDE: &str = "2027-01-15 08:30:00";
const BEFORE: &str = "2027-01-15 06:00:00";
const AFTER: &str = "2027-01-15 11:00:00";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn sql(path: &Path, statement: &str, binds: &[&str]) {
    runtime().block_on(async {
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

fn open(directory: &Path) -> (SqliteStore, std::path::PathBuf) {
    let path = directory.join("store.sqlite");
    (SqliteStore::new(&path).unwrap(), path)
}

fn add_task(path: &Path, id: &str, objective: &str) {
    sql(
        path,
        "INSERT INTO projects (id, root_path, settings) VALUES ('/work/p1', '/work/p1', '{}') \
         ON CONFLICT DO NOTHING",
        &[],
    );
    let contract = serde_json::json!({"objective": objective}).to_string();
    sql(
        path,
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES (?, '/work/p1', NULL, '\"Failed\"', ?, 1)",
        &[id, &contract],
    );
}

fn transition(path: &Path, task: &str, state: &str, at: &str) {
    let payload = serde_json::json!({"state": state}).to_string();
    sql(
        path,
        "INSERT INTO durable_events (task_id, kind, payload, created_at) \
         VALUES (?, 'task.transition', ?, ?)",
        &[task, &payload, at],
    );
}

fn proposal_event(path: &Path, proposal: &str, to_state: &str, actor: &str, at: &str) {
    let from = if to_state == "approved" {
        "pending"
    } else {
        "approved"
    };
    sql(
        path,
        "INSERT INTO action_proposal_events \
         (proposal_id, scope, from_state, to_state, actor, digest, created_at) \
         VALUES (?, 'conversation:global', ?, ?, ?, ?, ?)",
        &[proposal, from, to_state, actor, DIGEST, at],
    );
}

#[test]
fn entering_away_stamps_the_start_once_and_leaving_closes_the_window() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = open(directory.path());
    assert_eq!(store.away_windows().unwrap(), Default::default());

    let before = store
        .set_monitoring_mode(MonitoringMode::Away, FROM)
        .unwrap();
    assert_eq!(before, MonitoringMode::On);
    assert_eq!(
        store.orchestrator_settings().unwrap().mode,
        MonitoringMode::Away
    );
    assert_eq!(store.away_windows().unwrap().away_since_ms, Some(FROM));

    // Away to away changes nothing, the start included.
    let again = store
        .set_monitoring_mode(MonitoringMode::Away, FROM + HOUR)
        .unwrap();
    assert_eq!(again, MonitoringMode::Away);
    assert_eq!(store.away_windows().unwrap().away_since_ms, Some(FROM));

    store
        .set_monitoring_mode(MonitoringMode::On, FROM + 2 * HOUR)
        .unwrap();
    let windows = store.away_windows().unwrap();
    assert_eq!(windows.away_since_ms, None);
    assert_eq!(windows.brief_from_ms, Some(FROM));
    assert_eq!(windows.brief_to_ms, Some(FROM + 2 * HOUR));
}

#[test]
fn a_mode_change_between_non_away_modes_leaves_the_windows_alone() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = open(directory.path());
    store
        .set_monitoring_mode(MonitoringMode::Quiet, FROM)
        .unwrap();
    store
        .set_monitoring_mode(MonitoringMode::Off, FROM + HOUR)
        .unwrap();
    assert_eq!(store.away_windows().unwrap(), Default::default());
}

#[test]
fn an_unacknowledged_brief_keeps_its_start_when_the_user_goes_away_again() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = open(directory.path());
    store
        .set_monitoring_mode(MonitoringMode::Away, FROM)
        .unwrap();
    store
        .set_monitoring_mode(MonitoringMode::On, FROM + HOUR)
        .unwrap();
    store
        .set_monitoring_mode(MonitoringMode::Away, FROM + 3 * HOUR)
        .unwrap();
    assert_eq!(
        store.away_windows().unwrap().away_since_ms,
        Some(FROM + 3 * HOUR)
    );
    store
        .set_monitoring_mode(MonitoringMode::Quiet, FROM + 4 * HOUR)
        .unwrap();
    let windows = store.away_windows().unwrap();
    assert_eq!(windows.brief_from_ms, Some(FROM), "the first start stays");
    assert_eq!(windows.brief_to_ms, Some(FROM + 4 * HOUR));
}

#[test]
fn the_acknowledgement_clears_only_the_closed_window_and_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = open(directory.path());
    store
        .set_monitoring_mode(MonitoringMode::Away, FROM)
        .unwrap();
    store
        .set_monitoring_mode(MonitoringMode::On, FROM + HOUR)
        .unwrap();
    store
        .set_monitoring_mode(MonitoringMode::Away, FROM + 2 * HOUR)
        .unwrap();

    store.ack_brief().unwrap();
    store.ack_brief().unwrap();

    let windows = store.away_windows().unwrap();
    assert_eq!(windows.brief_from_ms, None);
    assert_eq!(windows.brief_to_ms, None);
    assert_eq!(
        windows.away_since_ms,
        Some(FROM + 2 * HOUR),
        "the live window stays"
    );
    assert_eq!(
        store.orchestrator_settings().unwrap().mode,
        MonitoringMode::Away
    );
}

#[test]
fn the_brief_counts_only_what_happened_inside_the_window() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    for task in ["t-in", "t-before", "t-after", "t-ok"] {
        add_task(&path, task, "Fix the login");
    }
    transition(&path, "t-in", "Failed", INSIDE);
    transition(&path, "t-before", "Failed", BEFORE);
    transition(&path, "t-after", "Failed", AFTER);
    transition(&path, "t-ok", "Succeeded", INSIDE);
    proposal_event(&path, "pr-1", "approved", "user", INSIDE);
    proposal_event(&path, "pr-2", "approved", "grant:g-1", INSIDE);
    proposal_event(&path, "pr-2", "executed", "grant:g-1", INSIDE);
    proposal_event(&path, "pr-3", "approved", "grant:g-1", BEFORE);

    let brief = store
        .return_brief(FROM, FROM + 2 * HOUR, false, FROM + 2 * HOUR)
        .unwrap();

    assert_eq!(brief.counts.tasks_failed, 1);
    assert_eq!(brief.counts.tasks_succeeded, 1);
    assert_eq!(brief.counts.proposals_approved_by_user, 1);
    assert_eq!(brief.counts.proposals_approved_by_grant, 1);
    assert_eq!(brief.counts.executed_by_grant, 1);
    assert_eq!(brief.counts.failed_under_grant, 0);
    assert!(!brief.open);
    assert_eq!(brief.attention.len(), 1);
    assert_eq!(brief.attention[0].id, "t-in");
    assert_eq!(brief.attention[0].kind, "task_failed");
}

#[test]
fn every_brief_list_is_capped_at_ten_while_the_counts_stay_exact() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    for index in 0..13 {
        let id = format!("t-{index:02}");
        add_task(&path, &id, "Fix it");
        transition(&path, &id, "Failed", INSIDE);
    }
    let brief = store
        .return_brief(FROM, FROM + 2 * HOUR, true, FROM + 2 * HOUR)
        .unwrap();
    assert_eq!(brief.counts.tasks_failed, 13);
    assert_eq!(brief.attention.len(), 10);
    assert!(brief.open);
}

#[test]
fn a_brief_title_is_one_line_of_at_most_eighty_bytes_of_the_objective() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    let objective = format!("Line one\nsecret-tail {}", "x".repeat(300));
    add_task(&path, "t-long", &objective);
    transition(&path, "t-long", "Failed", INSIDE);
    let brief = store
        .return_brief(FROM, FROM + 2 * HOUR, false, FROM + 2 * HOUR)
        .unwrap();
    let title = &brief.attention[0].title;
    assert!(title.len() <= 80, "{title}");
    assert!(!title.contains('\n'));
}

#[test]
fn the_brief_counts_stalls_pauses_quiet_updates_and_decisions_waiting_now() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    add_task(&path, "t-stall", "Slow one");
    for (id, kind, state, outcome, skip, at) in [
        ("w-1", "task_stalled", "noticed", "", "", INSIDE),
        ("w-2", "task_stalled", "noticed", "", "", BEFORE),
        ("w-3", "task_failed", "skipped", "", "limit_hourly", INSIDE),
        ("w-4", "task_failed", "handled", "quiet", "", INSIDE),
    ] {
        let subject = if id == "w-1" || id == "w-2" {
            "t-stall"
        } else {
            id
        };
        let needs_model = if state == "noticed" { "0" } else { "1" };
        sql(
            &path,
            "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice, \
             needs_model, state, outcome, skip_reason, created_at) \
             VALUES (?, 'conversation:global', ?, ?, ?, 'notice', ?, ?, NULLIF(?, ''), \
             NULLIF(?, ''), ?)",
            &[id, kind, subject, id, needs_model, state, outcome, skip, at],
        );
    }

    let brief = store
        .return_brief(FROM, FROM + 2 * HOUR, false, FROM + 2 * HOUR)
        .unwrap();

    assert_eq!(
        brief.counts.tasks_stalled, 1,
        "one task, one stall in the window"
    );
    assert_eq!(brief.counts.replies_paused_by_limit, 1);
    assert_eq!(brief.counts.quiet_updates, 1);
    assert_eq!(brief.counts.proposals_waiting, 0);
    assert_eq!(brief.counts.decisions_held, 0);
    assert_eq!(brief.attention[0].kind, "task_stalled");
    assert_eq!(brief.attention[0].state, "stalled");
}

#[test]
fn a_grant_used_or_ended_in_the_window_is_listed_with_its_status() {
    use orchestrator_core::orchestrator_action::ActionKind;
    use orchestrator_core::standing_grant::{GrantScope, GrantTerms};
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = open(directory.path());
    let terms = GrantTerms {
        scope: GrantScope::Project("p-00000001".to_owned()),
        kinds: vec![ActionKind::TaskPropose],
        expires_in_secs: 3600,
        max_uses: 5,
        token_budget: 1000,
    };
    store.create_standing_grant("g-1", &terms, FROM).unwrap();
    store
        .create_standing_grant("g-2", &terms, FROM - HOUR / 2)
        .unwrap();
    store.revoke_standing_grant("g-2", FROM + HOUR / 4).unwrap();

    // Past g-1's expiry: it ended inside the window. g-2 was revoked inside it.
    let brief = store
        .return_brief(FROM, FROM + 2 * HOUR, false, FROM + 2 * HOUR)
        .unwrap();

    assert_eq!(brief.counts.grants_expired, 1);
    assert_eq!(brief.counts.grants_revoked, 1);
    let ids: Vec<&str> = brief.grants.iter().map(|grant| grant.id.as_str()).collect();
    assert!(ids.contains(&"g-1") && ids.contains(&"g-2"), "{ids:?}");
    let first = brief.grants.iter().find(|grant| grant.id == "g-1").unwrap();
    assert_eq!(first.status, "expired");
    assert_eq!(first.scope_label, "p-00000001");
    let second = brief.grants.iter().find(|grant| grant.id == "g-2").unwrap();
    assert_eq!(second.status, "revoked");
}
