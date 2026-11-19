//! Migration 0071 (DPES-1108 phase 8): wake caps re-sized for autonomous supervision, and the
//! pull request links a completed turn keeps.

use std::borrow::Cow;
use std::path::Path;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::landing::{CheckState, PrState};
use orchestrator_core::orchestrator_action::{parse_block, BlockScope};
use orchestrator_core::records::Project;
use orchestrator_core::supervision::MonitoringMode;
use orchestrator_store::{
    ConversationTarget, NewWakeRecord, PullRequestLink, PullRequestSeen, SqliteStore, Store,
    StoredCheck, TurnBlock, TurnTrigger, WakeKind, MAX_TURN_LINKS,
};
use serde_json::json;
use sqlx::migrate::Migrator;

static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");
const NOW: u64 = 1_794_901_798_000;

#[test]
fn migration_0071_moves_the_old_default_wake_caps_and_keeps_chosen_ones() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // (hourly, daily) before, then after. NULL is no limit.
    for (before, after) in [
        ((Some(6), Some(40)), (Some(20), Some(160))),
        ((Some(12), Some(40)), (Some(12), Some(160))),
        ((Some(6), Some(90)), (Some(20), Some(90))),
        ((None, None), (None, None)),
    ] {
        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            let legacy = Migrator {
                migrations: Cow::Owned(
                    MIGRATIONS
                        .iter()
                        .filter(|m| m.version < 71)
                        .cloned()
                        .collect(),
                ),
                ..Migrator::DEFAULT
            };
            legacy.run(&pool).await.unwrap();
            let stored: (Option<i64>, Option<i64>) = sqlx::query_as(
                "SELECT wake_hourly_cap, wake_daily_cap FROM orchestrator_settings WHERE id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(stored, (Some(6), Some(40)), "the 0041 defaults");
            sqlx::query(
                "UPDATE orchestrator_settings SET wake_hourly_cap = ?, wake_daily_cap = ? \
                 WHERE id = 1",
            )
            .bind(before.0)
            .bind(before.1)
            .execute(&pool)
            .await
            .unwrap();
            MIGRATIONS.run(&pool).await.unwrap();
            let moved: (Option<i64>, Option<i64>) = sqlx::query_as(
                "SELECT wake_hourly_cap, wake_daily_cap FROM orchestrator_settings WHERE id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(moved, after, "{before:?}");
        });
    }
}

#[test]
fn a_new_store_reads_the_new_default_wake_caps() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let limits = store.orchestrator_settings().unwrap().wake_limits;
    assert_eq!(
        (limits.hourly, limits.daily, limits.daily_tokens),
        (Some(20), Some(160), None)
    );
}

fn contract(id: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: "p1".into(),
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
        work_item: None,
    }
}

/// A task, its pull request number if one is recorded, and the repository bound to it.
type Seed<'a> = (&'a str, Option<u64>, Option<(&'a str, &'a str)>);

/// A store with project p1 and `tasks` in it. Each `(task, number, repo)` with a number gets a
/// recorded pull request, and a bound repository when `repo` is set.
fn seeded(directory: &Path, tasks: &[Seed<'_>]) -> SqliteStore {
    let store = SqliteStore::new(&directory.join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: "p1".into(),
            root_path: "/work/p1".into(),
            settings: json!({}),
        })
        .unwrap();
    for (task, number, repo) in tasks {
        store.create(&contract(task)).unwrap();
        if let Some(number) = number {
            store
                .observe_pull_request(
                    &PullRequestSeen {
                        task_id: (*task).to_owned(),
                        project_id: "p1".to_owned(),
                        number: *number,
                        branch: format!("moa/{task}"),
                        base_branch: "main".to_owned(),
                        state: PrState::Open,
                        head_sha: "a".repeat(40),
                        mergeable: Some(true),
                        checks: vec![StoredCheck {
                            name: "build".to_owned(),
                            state: CheckState::Passed,
                        }],
                        title: "Fix it".to_owned(),
                    },
                    i64::try_from(NOW).unwrap(),
                )
                .unwrap();
        }
        if let Some((owner, name)) = repo {
            store.bind_pull_request_repo(task, owner, name).unwrap();
        }
    }
    store
}

fn block(actions: &str) -> TurnBlock {
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    TurnBlock::Accepted(parse_block(&held, BlockScope::Global).unwrap())
}

fn link(task: &str, number: u64, repo: &str) -> PullRequestLink {
    PullRequestLink {
        task_id: task.to_owned(),
        number,
        url: format!("https://github.com/{repo}/pull/{number}"),
    }
}

#[test]
fn a_turn_keeps_the_recorded_pull_requests_of_the_tasks_its_block_names() {
    let directory = tempfile::tempdir().unwrap();
    let store = seeded(
        directory.path(),
        &[
            ("task-aaaa0001", Some(12), Some(("octo", "demo"))),
            // No pull request recorded yet.
            ("task-aaaa0002", None, None),
            // A pull request the poller never bound to a repository.
            ("task-aaaa0003", Some(14), None),
            ("task-aaaa0004", Some(7), Some(("octo", "other"))),
        ],
    );
    store
        .begin_conversation_turn(&ConversationTarget::Global, "t1", "hi", "rt", "1.0.0")
        .unwrap();
    store
        .complete_conversation_turn(
            "t1",
            block(
                r#"{"kind": "task.retry", "task_id": "task-aaaa0004"},
                   {"kind": "task.retry", "task_id": "task-aaaa0002"},
                   {"kind": "task.cancel", "task_id": "task-aaaa0003"},
                   {"kind": "task.cancel", "task_id": "task-aaaa0001"},
                   {"kind": "workstream.create", "title": "Docs"}"#,
            ),
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        store.turn_pull_request_links("t1").unwrap(),
        vec![
            link("task-aaaa0004", 7, "octo/other"),
            link("task-aaaa0001", 12, "octo/demo"),
        ]
    );

    // A turn with no block, and a turn that is unknown, have none.
    store
        .begin_conversation_turn(&ConversationTarget::Global, "t2", "hi", "rt", "1.0.0")
        .unwrap();
    store
        .complete_conversation_turn("t2", TurnBlock::None)
        .unwrap()
        .unwrap();
    assert!(store.turn_pull_request_links("t2").unwrap().is_empty());
    assert!(store.turn_pull_request_links("nope").unwrap().is_empty());
}

#[test]
fn a_wake_turn_links_the_pull_requests_of_the_tasks_it_reports_and_keeps_at_most_five() {
    let directory = tempfile::tempdir().unwrap();
    let tasks: Vec<(String, u64)> = (1..=7u8)
        .map(|n| (format!("task-bbbb000{n}"), u64::from(n) + 100))
        .collect();
    let spec: Vec<Seed<'_>> = tasks
        .iter()
        .map(|(task, number)| (task.as_str(), Some(*number), Some(("octo", "demo"))))
        .collect();
    let store = seeded(directory.path(), &spec);
    let mut records: Vec<NewWakeRecord> = tasks
        .iter()
        .map(|(task, _)| NewWakeRecord {
            scope: "conversation:global".to_owned(),
            kind: WakeKind::CandidateReady,
            subject: task.clone(),
            dedupe_key: format!("ready:{task}"),
            notice: "A task is ready.".to_owned(),
            needs_model: true,
        })
        .collect();
    // A heartbeat's subject is not a task.
    records.push(NewWakeRecord {
        scope: "conversation:global".to_owned(),
        kind: WakeKind::WorkCheck,
        subject: "work".to_owned(),
        dedupe_key: "check:1".to_owned(),
        notice: "Work check.".to_owned(),
        needs_model: true,
    });
    let stored = store
        .record_wake_events(&records, MonitoringMode::On, 0)
        .unwrap();
    let ids: Vec<String> = stored.iter().map(|record| record.id.clone()).collect();
    store
        .claim_wake_batch("conversation:global", &ids, "b1", "w1", true, NOW)
        .unwrap();
    store
        .begin_conversation_turn_as(
            &ConversationTarget::Global,
            "w1",
            "update",
            "rt",
            "1.0.0",
            TurnTrigger::Wake,
        )
        .unwrap();
    store
        .complete_conversation_turn("w1", TurnBlock::None)
        .unwrap()
        .unwrap();

    let links = store.turn_pull_request_links("w1").unwrap();
    assert_eq!(links.len(), MAX_TURN_LINKS);
    assert_eq!(
        links,
        tasks[..MAX_TURN_LINKS]
            .iter()
            .map(|(task, number)| link(task, *number, "octo/demo"))
            .collect::<Vec<_>>()
    );
}

/// A coordinator's report and a routed request stand for the tasks serving the request
/// (DPES-1108 integration). The route and the report are approved by the orchestrator itself
/// (`agent`), as the autonomy pass approves them: the store's origin proofs accept that. The
/// coordinator turn that answers, and the global wake turn that reads the answer, both link the
/// pull request of the coordinator's task.
#[test]
fn a_coordinator_report_links_the_pull_requests_of_the_tasks_serving_its_request() {
    let directory = tempfile::tempdir().unwrap();
    let store = seeded(
        directory.path(),
        &[("task-cccc0001", Some(21), Some(("octo", "demo")))],
    );
    store.restore_coordinator("p1").unwrap();

    // Global routes work on its own: the route is approved as the agent and runs.
    store
        .begin_conversation_turn(&ConversationTarget::Global, "g1", "route it", "rt", "1.0.0")
        .unwrap();
    let route = store
        .complete_conversation_turn(
            "g1",
            block(r#"{"kind": "coordinator.route", "project": "p-3fa9c1d2", "work": "Fix it."}"#),
        )
        .unwrap()
        .unwrap();
    store.approve_action_proposal(&route[0], "agent").unwrap();
    let request = store
        .create_routed_request("r-0123456789abcdef", "p1", "Fix it.", &route[0])
        .unwrap();
    store
        .set_task_coordinator("task-cccc0001", "p1", Some(&request.id))
        .unwrap();

    // The coordinator answers on its own; its turn links the task serving the request.
    let project = ConversationTarget::Project("p1".to_owned());
    store
        .begin_conversation_turn_as(&project, "c1", "update", "rt", "1.0.0", TurnTrigger::Wake)
        .unwrap();
    let held = "```mcao-actions\n{\"mcao_actions\": 1, \"actions\": [{\"kind\": \
                \"coordinator.report\", \"verb\": \"answer\", \"note\": \"Done.\", \"request\": \
                \"r-0123456789abcdef\"}]}\n```";
    let report = store
        .complete_conversation_turn(
            "c1",
            TurnBlock::Accepted(parse_block(held, BlockScope::Project("p-3fa9c1d2")).unwrap()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        store.turn_pull_request_links("c1").unwrap(),
        vec![link("task-cccc0001", 21, "octo/demo")]
    );
    store.approve_action_proposal(&report[0], "agent").unwrap();
    let answer = store
        .record_coordinator_report(
            "rep-1",
            "p1",
            orchestrator_core::charter::ReportVerb::Answer,
            Some(&request.id),
            None,
            "Done.",
            &report[0],
        )
        .unwrap();

    // Global's wake turn for the report links the same pull request.
    let stored = store
        .record_wake_events(
            &[NewWakeRecord {
                scope: "conversation:global".to_owned(),
                kind: WakeKind::CoordinatorReport,
                subject: answer.id.clone(),
                dedupe_key: format!("coordinator_report:{}", answer.id),
                notice: "A coordinator answered.".to_owned(),
                needs_model: true,
            }],
            MonitoringMode::On,
            0,
        )
        .unwrap();
    let ids: Vec<String> = stored.iter().map(|record| record.id.clone()).collect();
    store
        .claim_wake_batch("conversation:global", &ids, "b1", "w1", true, NOW)
        .unwrap();
    store
        .begin_conversation_turn_as(
            &ConversationTarget::Global,
            "w1",
            "update",
            "rt",
            "1.0.0",
            TurnTrigger::Wake,
        )
        .unwrap();
    store
        .complete_conversation_turn("w1", TurnBlock::None)
        .unwrap()
        .unwrap();
    assert_eq!(
        store.turn_pull_request_links("w1").unwrap(),
        vec![link("task-cccc0001", 21, "octo/demo")]
    );
}
