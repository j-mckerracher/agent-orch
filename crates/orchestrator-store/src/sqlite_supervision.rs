//! Reads the wake scheduler needs from the task tables (DPES-1105 phase 3b).
//!
//! These are read-only. They answer which attempts are really running, how much work each
//! scope holds (the `W` of the check interval), and the few facts a wake notice may name.
//! A task held Queued, by the scheduler or by a closed load gate, has no Running attempt and
//! no session, so it appears in none of these: it is never counted as stalled and adds no
//! work unit.

use std::collections::BTreeMap;

use orchestrator_core::contract::{TaskContract, TaskKind};
use orchestrator_core::records::Project;
use orchestrator_core::task::{AttemptState, SessionState, TaskState};
use sqlx::Row;

use crate::sqlite_conversations::ConversationTarget;
use crate::{SqliteStore, StoreError};

/// Longest objective a notice may carry, in bytes.
const OBJECTIVE_BYTES: usize = 80;
const GLOBAL_SCOPE: &str = "conversation:global";
const WORKSTREAM_PREFIX: &str = "conversation:workstream:";

/// A session in Spawning or Running whose attempt is Running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningAttempt {
    pub task_id: String,
    pub attempt_id: String,
    pub session_id: String,
    /// Conversation id: the task's workstream, else global.
    pub scope: String,
    /// Time of the session's `session.created` event, in Unix ms.
    pub started_ms: u64,
}

/// The work one scope holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeLoad {
    pub scope: String,
    /// Tasks in Running. Global counts every such task.
    pub running_tasks: u32,
    /// A workstream that is a sub-orchestrator: it has a running task, a pending action
    /// proposal or a pending wake record. Always false for Global.
    pub sub_active: bool,
    /// `W` for the interval policy: `running_tasks`, and for Global one more unit for each
    /// active sub-orchestrator.
    pub work_units: u32,
}

/// What a wake notice may say about a task. The text fields are agent output or a failure
/// reason; the daemon redacts them before any prompt sees them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskWakeFacts {
    pub scope: String,
    /// The objective on one line, cut to 80 bytes.
    pub objective: String,
    /// The latest attempt's failure reason, cut to 160 bytes. `None` when there is none.
    pub failure_reason: Option<String>,
    /// Files the latest attempt changed. `None` until recorded.
    pub changed_files: Option<u32>,
    /// The latest attempt's summary, cut to 4 KiB. `None` until recorded.
    pub summary: Option<String>,
    /// Whether the task leaves a report rather than a change. This is the executor's test for
    /// storing a report: an Investigate task, or a task of a project that runs in place.
    pub report_only: bool,
    /// The note of the latest attempt's newest `needs-decision` or `blocked` status line, cut
    /// to 160 bytes. Worker text, already redacted when stored. `None` when there is none.
    pub wait_note: Option<String>,
}

/// Longest failure reason a wake fact carries, in bytes.
const FAILURE_REASON_BYTES: usize = 160;
/// Longest summary a wake fact carries, in bytes.
const SUMMARY_BYTES: usize = 4096;

/// `text` cut to at most `max` bytes, on a character boundary.
fn cut_bytes(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String, StoreError> {
    Ok(serde_json::to_string(value)?)
}

fn workstream_scope(workstream_id: Option<String>) -> String {
    workstream_id.map_or_else(
        || GLOBAL_SCOPE.to_owned(),
        |id| ConversationTarget::Workstream(id).id(),
    )
}

/// One line of at most 80 bytes: control characters become spaces, runs of spaces collapse,
/// and the cut falls on a character boundary.
pub(crate) fn one_line(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut end = flat.len().min(OBJECTIVE_BYTES);
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    flat[..end].to_owned()
}

impl SqliteStore {
    /// Sessions in Spawning or Running whose attempt is Running, oldest session first. Only
    /// attempts that really started are returned: the executor creates the attempt inside
    /// `launch`, after the scheduler admitted the task, and the session after the worktree
    /// exists. `started_ms` comes from the `session.created` durable event.
    pub fn running_attempts(&self) -> Result<Vec<RunningAttempt>, StoreError> {
        let session_states = [
            encode(&SessionState::Spawning)?,
            encode(&SessionState::Running)?,
        ];
        let attempt_state = encode(&AttemptState::Running)?;
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT s.task_id, s.attempt_id, s.id AS session_id, ws.workstream_id, \
                        (SELECT CAST(strftime('%s', e.created_at) AS INTEGER) * 1000 \
                           FROM durable_events e \
                          WHERE e.task_id = s.task_id AND e.session_id = s.id \
                            AND e.kind = 'session.created' \
                          ORDER BY e.sequence LIMIT 1) AS started_ms \
                 FROM sessions s \
                 JOIN attempts a ON a.task_id = s.task_id AND a.id = s.attempt_id \
                 LEFT JOIN workstream_tasks ws ON ws.task_id = s.task_id \
                 WHERE s.state IN (?, ?) AND a.state = ? AND started_ms IS NOT NULL \
                 ORDER BY s.sequence",
            )
            .bind(&session_states[0])
            .bind(&session_states[1])
            .bind(&attempt_state)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let started: i64 = row.try_get("started_ms")?;
                    Ok::<_, StoreError>(RunningAttempt {
                        task_id: row.try_get("task_id")?,
                        attempt_id: row.try_get("attempt_id")?,
                        session_id: row.try_get("session_id")?,
                        scope: workstream_scope(row.try_get("workstream_id")?),
                        started_ms: u64::try_from(started).unwrap_or(0),
                    })
                })
                .collect()
        })
    }

    /// The work each scope holds, sorted by scope. A scope with no work is left out, so a
    /// scope absent from the answer arms no check. Only a Running task is work: a task in
    /// Verifying waits for the user's review and must not keep the check timer going.
    /// Workstream scopes count only their own running tasks. Global counts every running task,
    /// and one more unit for each workstream that is an active sub-orchestrator.
    pub fn supervision_load(&self) -> Result<Vec<ScopeLoad>, StoreError> {
        let running = encode(&TaskState::Running)?;
        self.call(move |pool| async move {
            // scope -> (running tasks, has other activity)
            let mut scopes: BTreeMap<String, (u32, bool)> = BTreeMap::new();
            let mut total = 0_u32;
            let rows: Vec<(Option<String>, i64)> = sqlx::query_as(
                "SELECT ws.workstream_id, COUNT(*) FROM tasks t \
                 LEFT JOIN workstream_tasks ws ON ws.task_id = t.id \
                 WHERE t.state = ? GROUP BY ws.workstream_id",
            )
            .bind(&running)
            .fetch_all(&pool)
            .await?;
            for (workstream_id, count) in rows {
                let count = u32::try_from(count).unwrap_or(u32::MAX);
                total = total.saturating_add(count);
                if let Some(id) = workstream_id {
                    scopes
                        .entry(ConversationTarget::Workstream(id).id())
                        .or_default()
                        .0 = count;
                }
            }
            let waiting: Vec<String> = sqlx::query_scalar(
                "SELECT scope FROM action_proposals WHERE state = 'pending' \
                 UNION SELECT scope FROM wake_records WHERE state = 'pending'",
            )
            .fetch_all(&pool)
            .await?;
            for scope in waiting {
                if scope.starts_with(WORKSTREAM_PREFIX) {
                    scopes.entry(scope).or_default().1 = true;
                }
            }
            let mut sub_orchestrators = 0_u32;
            let mut loads = Vec::new();
            for (scope, (running_tasks, waiting)) in scopes {
                let sub_active = running_tasks > 0 || waiting;
                if sub_active {
                    sub_orchestrators += 1;
                }
                loads.push(ScopeLoad {
                    scope,
                    running_tasks,
                    sub_active,
                    work_units: running_tasks,
                });
            }
            let global_units = total.saturating_add(sub_orchestrators);
            if global_units > 0 {
                loads.push(ScopeLoad {
                    scope: GLOBAL_SCOPE.to_owned(),
                    running_tasks: total,
                    sub_active: false,
                    work_units: global_units,
                });
            }
            loads.sort_by(|a, b| a.scope.cmp(&b.scope));
            Ok(loads)
        })
    }

    /// The scope, one-line objective and latest-attempt facts of a task, for a wake notice. `None`
    /// for an unknown task. The daemon applies its own sanitizer on top, so no path reaches a
    /// prompt.
    pub fn task_wake_facts(&self, task_id: &str) -> Result<Option<TaskWakeFacts>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let row = sqlx::query(
                "SELECT ws.workstream_id, \
                        COALESCE(json_extract(t.contract_json, '$.objective'), '') AS objective, \
                        t.contract_json, p.id AS project_id, p.root_path AS project_root, \
                        p.settings AS project_settings, \
                        latest.failure_reason, latest.changed_files, latest.summary, \
                        (SELECT e.note FROM task_status_events e \
                          WHERE e.task_id = t.id AND e.attempt_id = latest.id \
                            AND e.verb IN ('needs-decision', 'blocked') \
                          ORDER BY e.line_no DESC LIMIT 1) AS wait_note \
                 FROM tasks t LEFT JOIN workstream_tasks ws ON ws.task_id = t.id \
                 LEFT JOIN projects p ON p.id = t.project_id \
                 LEFT JOIN attempts latest ON latest.task_id = t.id \
                 AND latest.attempt_number = (SELECT MAX(a.attempt_number) FROM attempts a WHERE a.task_id = t.id) \
                 WHERE t.id = ?",
            )
            .bind(&task_id)
            .fetch_optional(&pool)
            .await?;
            row.map(|row| {
                let objective: String = row.try_get("objective")?;
                let changed_files = row
                    .try_get::<Option<i64>, _>("changed_files")?
                    .map(|count| {
                        u32::try_from(count).map_err(|_| StoreError::Corrupt {
                            task_id: task_id.clone(),
                            detail: "changed file count exceeds u32".to_owned(),
                        })
                    })
                    .transpose()?;
                let contract: TaskContract =
                    serde_json::from_str(&row.try_get::<String, _>("contract_json")?)?;
                // A task whose project row is missing is not in place.
                let in_place = match row.try_get::<Option<String>, _>("project_settings")? {
                    Some(settings) => Project {
                        id: row.try_get("project_id")?,
                        root_path: row.try_get("project_root")?,
                        settings: serde_json::from_str(&settings)?,
                    }
                    .is_in_place(),
                    None => false,
                };
                Ok::<_, StoreError>(TaskWakeFacts {
                    scope: workstream_scope(row.try_get("workstream_id")?),
                    objective: one_line(&objective),
                    failure_reason: row
                        .try_get::<Option<String>, _>("failure_reason")?
                        .map(|reason| cut_bytes(reason, FAILURE_REASON_BYTES)),
                    changed_files,
                    summary: row
                        .try_get::<Option<String>, _>("summary")?
                        .map(|summary| cut_bytes(summary, SUMMARY_BYTES)),
                    report_only: contract.kind == TaskKind::Investigate || in_place,
                    wait_note: row
                        .try_get::<Option<String>, _>("wait_note")?
                        .map(|note| cut_bytes(note, FAILURE_REASON_BYTES)),
                })
            })
            .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };
    use orchestrator_core::records::Project;
    use orchestrator_core::task::{Attempt, Session, TaskEvent};
    use orchestrator_core::workstream::{Workstream, WorkstreamState};

    use super::*;
    use crate::{LoadPatch, SettingsPatch, Store, SupervisionPatch, WakeLimitsPatch};
    use orchestrator_core::supervision::{MonitoringMode, WakeLimits};

    fn open() -> (tempfile::TempDir, SqliteStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
        store
            .register_project(&Project {
                id: "p".into(),
                root_path: "/repo".into(),
                settings: serde_json::json!({}),
            })
            .unwrap();
        (directory, store)
    }

    fn make_task(store: &SqliteStore, id: &str, objective: &str) {
        store
            .create(&TaskContract {
                id: id.into(),
                project_id: "p".into(),
                parent_id: None,
                objective: objective.into(),
                kind: TaskKind::Implement,
                agent: "codex".into(),
                dependencies: Vec::new(),
                verification: VerificationPolicy::Commands {
                    commands: vec![vec!["true".into()]],
                },
                delivery: DeliveryPolicy::LocalReview,
                autonomy: AutonomyPolicy::WorkspaceWrites,
                max_retries: 1,
                max_repairs: 1,
                model_selection: None,
                work_item: None,
                permission: Default::default(),
            })
            .unwrap();
        store.transition(id, TaskEvent::Admit).unwrap();
    }

    /// A task with a Running attempt and a Spawning session, the way the executor leaves it.
    fn run_task(store: &SqliteStore, id: &str) {
        make_task(store, id, "x");
        let attempt = format!("{id}-a1");
        // The store takes an attempt only while the task is Queued; the task starts running after.
        store
            .add_attempt(
                id,
                &Attempt {
                    id: attempt.clone(),
                    task_id: id.into(),
                    attempt_number: 1,
                    state: AttemptState::Pending,
                    failure_reason: None,
                    summary: None,
                    changed_files: None,
                },
            )
            .unwrap();
        store
            .update_attempt_state(id, &attempt, AttemptState::Running)
            .unwrap();
        store.transition(id, TaskEvent::ExecutionStarted).unwrap();
        store
            .add_session(
                id,
                &Session {
                    id: format!("{id}-s1"),
                    attempt_id: attempt,
                    adapter: "codex".into(),
                    state: SessionState::Spawning,
                },
            )
            .unwrap();
    }

    fn workstream(store: &SqliteStore, id: &str) {
        store
            .create_workstream(
                &Workstream {
                    id: id.into(),
                    title: id.into(),
                    summary: String::new(),
                    state: WorkstreamState::Active,
                    revision: 1,
                },
                "test",
            )
            .unwrap();
    }

    fn execute(store: &SqliteStore, sql: &'static str) -> Result<(), sqlx::Error> {
        store
            .call(move |pool| async move { Ok(sqlx::query(sql).execute(&pool).await.map(|_| ())) })
            .unwrap()
    }

    fn scalar(store: &SqliteStore, sql: &'static str) -> i64 {
        store
            .call(move |pool| async move { Ok(sqlx::query_scalar(sql).fetch_one(&pool).await?) })
            .unwrap()
    }

    fn is_invalid_input(error: &StoreError) -> bool {
        matches!(error, StoreError::Io(cause) if cause.kind() == std::io::ErrorKind::InvalidInput)
    }

    #[test]
    fn running_attempts_report_session_start_time() {
        let (_directory, store) = open();
        run_task(&store, "t1");
        // A queued task and a task whose attempt is only Pending have no running attempt.
        make_task(&store, "queued", "held by the load gate");
        make_task(&store, "pending", "y");
        store
            .add_attempt(
                "pending",
                &Attempt {
                    id: "pa".into(),
                    task_id: "pending".into(),
                    attempt_number: 1,
                    state: AttemptState::Pending,
                    failure_reason: None,
                    summary: None,
                    changed_files: None,
                },
            )
            .unwrap();
        store
            .transition("pending", TaskEvent::ExecutionStarted)
            .unwrap();
        execute(
            &store,
            "UPDATE durable_events SET created_at = '2026-01-02 03:04:05' \
             WHERE kind = 'session.created'",
        )
        .unwrap();
        let found = store.running_attempts().unwrap();
        assert_eq!(
            found,
            vec![RunningAttempt {
                task_id: "t1".into(),
                attempt_id: "t1-a1".into(),
                session_id: "t1-s1".into(),
                scope: "conversation:global".into(),
                // 2026-01-02 03:04:05 UTC
                started_ms: 1_767_323_045_000,
            }]
        );
    }

    #[test]
    fn a_task_held_queued_is_never_a_running_attempt_or_a_work_unit() {
        let (_directory, store) = open();
        for index in 0..30 {
            make_task(&store, &format!("held-{index}"), "waiting");
        }
        assert!(store.running_attempts().unwrap().is_empty());
        assert!(store.supervision_load().unwrap().is_empty());
    }

    #[test]
    fn supervision_load_counts_tasks_and_sub_orchestrators() {
        let (_directory, store) = open();
        workstream(&store, "w1");
        workstream(&store, "w2");
        workstream(&store, "idle");
        run_task(&store, "t1");
        run_task(&store, "t2");
        run_task(&store, "t3");
        store.link_task("w1", "t1", None).unwrap();
        store.link_task("w1", "t2", None).unwrap();
        // w2 has no running task but a pending wake record, so it is active.
        store
            .record_wake_events(
                &[crate::sqlite_wake::NewWakeRecord {
                    scope: "conversation:workstream:w2".into(),
                    kind: crate::sqlite_wake::WakeKind::TaskFailed,
                    subject: "t9".into(),
                    dedupe_key: "failed:t9:1".into(),
                    notice: "Task t9 failed.".into(),
                    needs_model: true,
                }],
                MonitoringMode::On,
                1,
            )
            .unwrap();
        let loads = store.supervision_load().unwrap();
        let by_scope: Vec<(&str, u32, bool, u32)> = loads
            .iter()
            .map(|load| {
                (
                    load.scope.as_str(),
                    load.running_tasks,
                    load.sub_active,
                    load.work_units,
                )
            })
            .collect();
        assert_eq!(
            by_scope,
            vec![
                // Global: three running tasks and two active sub-orchestrators.
                ("conversation:global", 3, false, 5),
                ("conversation:workstream:w1", 2, true, 2),
                ("conversation:workstream:w2", 0, true, 0),
            ]
        );
        let scopes: Vec<_> = store
            .running_attempts()
            .unwrap()
            .into_iter()
            .map(|attempt| (attempt.task_id, attempt.scope))
            .collect();
        assert!(scopes.contains(&("t1".to_owned(), "conversation:workstream:w1".to_owned())));
        assert!(scopes.contains(&("t3".to_owned(), "conversation:global".to_owned())));
    }

    #[test]
    fn task_facts_name_the_scope_and_a_short_one_line_objective() {
        let (_directory, store) = open();
        workstream(&store, "w1");
        make_task(&store, "t1", &format!("fix the  thing {}", "é".repeat(60)));
        store.link_task("w1", "t1", None).unwrap();
        let facts = store.task_wake_facts("t1").unwrap().unwrap();
        assert_eq!(facts.scope, "conversation:workstream:w1");
        assert!(facts.objective.starts_with("fix the thing é"));
        assert!(facts.objective.len() <= 80 && !facts.objective.contains('\n'));
        assert!(store.task_wake_facts("missing").unwrap().is_none());
    }

    #[test]
    fn a_verifying_task_is_not_a_work_unit() {
        let (_directory, store) = open();
        workstream(&store, "w1");
        run_task(&store, "t1");
        store.link_task("w1", "t1", None).unwrap();
        run_task(&store, "t2");
        // t2 is ready for the user's review: it is Verifying, not running.
        store.transition("t2", TaskEvent::CandidateReady).unwrap();
        let loads = store.supervision_load().unwrap();
        let by_scope: Vec<(&str, u32, bool, u32)> = loads
            .iter()
            .map(|load| {
                (
                    load.scope.as_str(),
                    load.running_tasks,
                    load.sub_active,
                    load.work_units,
                )
            })
            .collect();
        assert_eq!(
            by_scope,
            vec![
                // Global: one running task and one active sub-orchestrator.
                ("conversation:global", 1, false, 2),
                ("conversation:workstream:w1", 1, true, 1),
            ]
        );
        // A store whose only task is Verifying has no work, so no check timer runs.
        let (_other_directory, only_verifying) = open();
        run_task(&only_verifying, "v");
        only_verifying
            .transition("v", TaskEvent::CandidateReady)
            .unwrap();
        assert!(only_verifying.supervision_load().unwrap().is_empty());
    }

    #[test]
    fn task_facts_carry_the_latest_attempt_within_the_byte_caps() {
        let (_directory, store) = open();
        run_task(&store, "t1");
        // 201 bytes. The 160-byte cut falls inside a two-byte character, so it steps back to 159.
        store
            .record_attempt_failure("t1", "t1-a1", &format!("a{}", "é".repeat(100)))
            .unwrap();
        store
            .record_attempt_summary("t1", "t1-a1", Some(&"é".repeat(3000)), Some(4))
            .unwrap();
        let facts = store.task_wake_facts("t1").unwrap().unwrap();
        assert_eq!(facts.failure_reason.map(|reason| reason.len()), Some(159));
        assert_eq!(facts.summary.map(|summary| summary.len()), Some(4096));
        assert_eq!(facts.changed_files, Some(4));
    }

    #[test]
    fn a_report_only_task_is_flagged_in_its_wake_facts() {
        let (_directory, store) = open();
        store
            .register_project(&Project {
                id: "home".into(),
                root_path: "/home/me".into(),
                settings: serde_json::json!({"in_place": true}),
            })
            .unwrap();
        let create = |id: &str, project: &str, kind: TaskKind, delivery: DeliveryPolicy| {
            store
                .create(&TaskContract {
                    id: id.into(),
                    project_id: project.into(),
                    parent_id: None,
                    objective: id.into(),
                    kind,
                    agent: "codex".into(),
                    dependencies: Vec::new(),
                    verification: VerificationPolicy::Commands {
                        commands: vec![vec!["true".into()]],
                    },
                    delivery,
                    autonomy: AutonomyPolicy::WorkspaceWrites,
                    max_retries: 1,
                    max_repairs: 1,
                    model_selection: None,
                    work_item: None,
                    permission: Default::default(),
                })
                .unwrap();
        };
        create(
            "change",
            "p",
            TaskKind::Implement,
            DeliveryPolicy::LocalReview,
        );
        create(
            "scout",
            "p",
            TaskKind::Investigate,
            DeliveryPolicy::ReportOnly,
        );
        create(
            "home-change",
            "home",
            TaskKind::Implement,
            DeliveryPolicy::LocalReview,
        );
        // Only the executor's test counts: a ReportOnly delivery alone leaves no report.
        create(
            "delivery-only",
            "p",
            TaskKind::Implement,
            DeliveryPolicy::ReportOnly,
        );
        let report_only = |id: &str| store.task_wake_facts(id).unwrap().unwrap().report_only;
        assert!(!report_only("change"));
        assert!(report_only("scout"));
        assert!(report_only("home-change"));
        assert!(!report_only("delivery-only"));
    }

    #[test]
    fn open_tasks_carry_the_latest_attempt_time_in_state_and_report_facts() {
        let (_directory, store) = open();
        run_task(&store, "t1");
        store
            .record_attempt_failure("t1", "t1-a1", "The agent exited with code 2.")
            .unwrap();
        store
            .record_attempt_summary("t1", "t1-a1", Some("Partial work."), Some(2))
            .unwrap();
        let first = |store: &SqliteStore| {
            store
                .task_overview(10)
                .unwrap()
                .open
                .into_iter()
                .next()
                .unwrap()
        };
        let task = first(&store);
        assert_eq!(
            task.failure_reason.as_deref(),
            Some("The agent exited with code 2.")
        );
        assert_eq!(
            (
                task.changed_files,
                task.minutes_in_state,
                task.report_relayed
            ),
            (Some(2), Some(0), None)
        );
        // A captured report is not yet relayed until a relay marks it.
        assert!(store
            .capture_task_report("t1", "t1-a1", "Findings.", false, "2026-01-01T00:00:00Z")
            .unwrap());
        assert_eq!(first(&store).report_relayed, Some(false));
        assert!(store
            .mark_task_report_relayed("t1", "2026-01-01T00:00:01Z")
            .unwrap());
        assert_eq!(first(&store).report_relayed, Some(true));
    }

    #[test]
    fn settings_default_to_the_phase_three_values() {
        let (_directory, store) = open();
        let settings = store.orchestrator_settings().unwrap();
        assert_eq!(settings.mode, MonitoringMode::On);
        assert_eq!(
            (
                settings.supervision.base_interval_secs,
                settings.supervision.min_interval_secs,
                settings.supervision.attempt_wall_minutes
            ),
            (600, 60, 120)
        );
        assert_eq!(settings.wake_limits, WakeLimits::default());
        assert_eq!(
            (settings.load.cpu, settings.load.memory, settings.load.disk),
            (80, 85, 90)
        );
        assert_eq!(settings.interval_policy().base(), Duration::from_secs(600));
    }

    #[test]
    fn settings_patch_distinguishes_unchanged_from_unlimited() {
        let (_directory, store) = open();
        let cleared = store
            .update_orchestrator_settings(&SettingsPatch {
                wake_limits: WakeLimitsPatch {
                    hourly_cap: Some(None),
                    daily_token_budget: Some(Some(5_000)),
                    ..WakeLimitsPatch::default()
                },
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(cleared.wake_limits.hourly, None);
        assert_eq!(cleared.wake_limits.daily, Some(40));
        assert_eq!(cleared.wake_limits.daily_tokens, Some(5_000));
        let back = store
            .update_orchestrator_settings(&SettingsPatch {
                wake_limits: WakeLimitsPatch {
                    hourly_cap: Some(Some(3)),
                    daily_token_budget: Some(None),
                    ..WakeLimitsPatch::default()
                },
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(back.wake_limits.hourly, Some(3));
        assert_eq!(back.wake_limits.daily_tokens, None);
        assert_eq!(back.wake_limits.daily, Some(40));
    }

    #[test]
    fn settings_reject_out_of_range_and_minimum_above_base() {
        let (_directory, store) = open();
        let before = store.orchestrator_settings().unwrap();
        let supervision = |base, minimum, wall| SettingsPatch {
            supervision: SupervisionPatch {
                base_interval_secs: base,
                min_interval_secs: minimum,
                attempt_wall_minutes: wall,
            },
            ..SettingsPatch::default()
        };
        let wake = |hourly, daily, budget| SettingsPatch {
            wake_limits: WakeLimitsPatch {
                hourly_cap: hourly,
                daily_cap: daily,
                daily_token_budget: budget,
            },
            ..SettingsPatch::default()
        };
        let refused = [
            supervision(Some(59), None, None),
            supervision(None, Some(9), None),
            // The merged minimum (700) is above the stored base (600).
            supervision(None, Some(700), None),
            // The merged minimum (200) is above the merged base (100).
            supervision(Some(100), Some(200), None),
            supervision(None, None, Some(4)),
            supervision(None, None, Some(1441)),
            wake(Some(Some(0)), None, None),
            wake(None, Some(Some(0)), None),
            wake(None, None, Some(Some(0))),
            // A valid field beside an invalid one writes nothing.
            SettingsPatch {
                load: LoadPatch {
                    cpu: Some(60),
                    ..LoadPatch::default()
                },
                ..wake(Some(Some(0)), None, None)
            },
        ];
        for patch in &refused {
            let error = store.update_orchestrator_settings(patch).unwrap_err();
            assert!(is_invalid_input(&error), "{patch:?}: {error:?}");
            assert!(!error.to_string().contains("constraint"), "{error}");
        }
        assert_eq!(store.orchestrator_settings().unwrap(), before);
    }

    #[test]
    fn the_minimum_above_base_rule_lives_in_the_interval_policy_not_the_table() {
        let (_directory, store) = open();
        // The table accepts it: ALTER TABLE could not add the table-level CHECK.
        execute(
            &store,
            "UPDATE orchestrator_settings SET min_interval_secs = 900, base_interval_secs = 600",
        )
        .unwrap();
        let error = store.orchestrator_settings().unwrap_err();
        assert!(matches!(error, StoreError::Corrupt { .. }), "{error:?}");
        // The update refuses the same merge before writing.
        execute(
            &store,
            "UPDATE orchestrator_settings SET min_interval_secs = 60, base_interval_secs = 600",
        )
        .unwrap();
        let refused = store
            .update_orchestrator_settings(&SettingsPatch {
                supervision: SupervisionPatch {
                    min_interval_secs: Some(601),
                    ..SupervisionPatch::default()
                },
                ..SettingsPatch::default()
            })
            .unwrap_err();
        assert!(is_invalid_input(&refused));
        assert_eq!(
            scalar(
                &store,
                "SELECT min_interval_secs FROM orchestrator_settings"
            ),
            60
        );
    }

    #[test]
    fn the_column_checks_refuse_out_of_range_values_directly() {
        let (_directory, store) = open();
        for sql in [
            "UPDATE orchestrator_settings SET base_interval_secs = 59",
            "UPDATE orchestrator_settings SET base_interval_secs = 14401",
            "UPDATE orchestrator_settings SET min_interval_secs = 9",
            "UPDATE orchestrator_settings SET attempt_wall_minutes = 4",
            "UPDATE orchestrator_settings SET wake_hourly_cap = 0",
            "UPDATE orchestrator_settings SET wake_daily_cap = 0",
            "UPDATE orchestrator_settings SET wake_daily_token_budget = 0",
            "UPDATE orchestrator_settings SET monitoring_mode = 'bogus'",
            "UPDATE orchestrator_settings SET wake_cursor = -1",
        ] {
            assert!(execute(&store, sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn a_supervision_patch_leaves_the_load_limits_alone_and_the_reverse() {
        let (_directory, store) = open();
        let after_intervals = store
            .update_orchestrator_settings(&SettingsPatch {
                supervision: SupervisionPatch {
                    base_interval_secs: Some(1200),
                    min_interval_secs: Some(30),
                    attempt_wall_minutes: Some(60),
                },
                wake_limits: WakeLimitsPatch {
                    hourly_cap: Some(Some(2)),
                    ..WakeLimitsPatch::default()
                },
                mode: Some(MonitoringMode::Quiet),
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(
            (
                after_intervals.load.cpu,
                after_intervals.load.memory,
                after_intervals.load.disk
            ),
            (80, 85, 90)
        );
        assert_eq!(after_intervals.mode, MonitoringMode::Quiet);
        let after_load = store
            .update_orchestrator_settings(&SettingsPatch {
                load: LoadPatch {
                    cpu: Some(70),
                    ..LoadPatch::default()
                },
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(after_load.load.cpu, 70);
        assert_eq!(after_load.supervision, after_intervals.supervision);
        assert_eq!(after_load.wake_limits, after_intervals.wake_limits);
        assert_eq!(after_load.mode, MonitoringMode::Quiet);
    }

    #[test]
    fn a_settings_update_never_changes_the_wake_cursor() {
        let (_directory, store) = open();
        store
            .record_wake_events(&[], MonitoringMode::On, 42)
            .unwrap();
        assert_eq!(store.wake_cursor().unwrap(), 42);
        for patch in [
            SettingsPatch {
                load: LoadPatch {
                    disk: Some(95),
                    ..LoadPatch::default()
                },
                ..SettingsPatch::default()
            },
            SettingsPatch {
                supervision: SupervisionPatch {
                    attempt_wall_minutes: Some(30),
                    ..SupervisionPatch::default()
                },
                ..SettingsPatch::default()
            },
            SettingsPatch {
                wake_limits: WakeLimitsPatch {
                    daily_cap: Some(None),
                    ..WakeLimitsPatch::default()
                },
                ..SettingsPatch::default()
            },
            SettingsPatch {
                mode: Some(MonitoringMode::Off),
                ..SettingsPatch::default()
            },
        ] {
            store.update_orchestrator_settings(&patch).unwrap();
            assert_eq!(store.wake_cursor().unwrap(), 42);
        }
    }
}
