use super::*;
use orchestrator_core::task::{Session, SessionState};
use orchestrator_store::{UsageReceipt, UsageSource};

// 2026-01-02T00:00:00Z
const UTC_MIDNIGHT: u64 = 1_767_312_000_000;
const HOUR: u64 = 3_600_000;
const LA: i64 = -8 * 3600;

fn new_api() -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    api.store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/private/project-root".into(),
            settings: json!({}),
        })
        .unwrap();
    (directory, api)
}

fn task_with_sessions(api: &DaemonApi, sessions: &[(&str, &str)]) {
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };
    use orchestrator_core::task::{Attempt, AttemptState, TaskEvent};

    api.store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "Usage".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::ReportOnly,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
        })
        .unwrap();
    api.store.transition("task-1", TaskEvent::Admit).unwrap();
    api.store
        .add_attempt(
            "task-1",
            &Attempt {
                id: "attempt-1".into(),
                task_id: "task-1".into(),
                attempt_number: 1,
                state: AttemptState::Pending,
            },
        )
        .unwrap();
    for (id, adapter) in sessions {
        api.store
            .add_session(
                "task-1",
                &Session {
                    id: (*id).into(),
                    attempt_id: "attempt-1".into(),
                    adapter: (*adapter).into(),
                    state: SessionState::Spawning,
                },
            )
            .unwrap();
    }
}

fn receipt(session: &str, key: &str, end_ms: u64) -> UsageReceipt {
    UsageReceipt {
        task_id: "task-1".into(),
        session_id: session.into(),
        source: UsageSource::ProviderReported,
        receipt_key: key.into(),
        window_start_ms: end_ms - 1000,
        window_end_ms: end_ms,
        reported_at_ms: end_ms,
        input_tokens: Some(100),
        cache_read_tokens: None,
        cache_write_tokens: None,
        output_tokens: Some(20),
        reasoning_tokens: None,
        cost: None,
        model: None,
    }
}

fn now_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

/// A window around the real clock, so sessions just created are inside it.
fn around_now() -> Value {
    let now = now_ms();
    json!({
        "timezone": "UTC",
        "utc_offset_seconds": 0,
        "window_start_unix_ms": now - HOUR,
        "window_end_unix_ms": now + HOUR,
    })
}

fn provider<'a>(summary: &'a Value, name: &str) -> &'a Value {
    summary["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["provider"] == name)
        .unwrap_or_else(|| panic!("provider {name} missing"))
}

#[test]
fn silent_provider_is_unknown_and_never_zero() {
    let (_directory, api) = new_api();
    task_with_sessions(
        &api,
        &[
            ("s-codex", "codex"),
            ("s-claude", "claude-code"),
            ("s-pi", "pi"),
        ],
    );
    let now = now_ms();
    // Tokens without a cost (codex).
    api.store
        .record_usage_receipt(&receipt("s-codex", "turn.completed:1", now))
        .unwrap();
    // Tokens and a cost (claude).
    let mut priced = receipt("s-claude", "result", now);
    priced.cost = Some((8_638_800, "USD".into()));
    api.store.record_usage_receipt(&priced).unwrap();
    // pi reports nothing.

    let summary = api.usage_summary(around_now()).unwrap();
    assert_eq!(summary["window"]["timezone"], "UTC");

    let codex = provider(&summary, "codex");
    assert_eq!(codex["token_status"], "complete");
    assert_eq!(codex["cost_status"], "unknown");
    assert_eq!(codex["reported"]["input_tokens"], 100);
    assert_eq!(codex["reported"]["costs"], json!([]));

    let claude = provider(&summary, "claude-code");
    assert_eq!(claude["cost_status"], "complete");
    assert_eq!(
        claude["reported"]["costs"],
        json!([{"currency": "USD", "amount": "0.0086388"}])
    );

    let pi = provider(&summary, "pi");
    assert_eq!(pi["sessions"], 1);
    assert_eq!(pi["token_status"], "unknown");
    assert_eq!(pi["cost_status"], "unknown");
    assert!(pi["reported"]["input_tokens"].is_null());
    assert!(pi["reported"]["output_tokens"].is_null());

    let totals = &summary["totals"];
    assert_eq!(totals["sessions"], 3);
    assert_eq!(totals["token_status"], "partial");
    assert_eq!(totals["cost_status"], "partial");
    assert_eq!(totals["reported"]["input_tokens"], 200);
}

#[test]
fn an_empty_day_reports_no_providers_and_unknown_totals() {
    let (_directory, api) = new_api();
    let summary = api
        .usage_summary(json!({
            "timezone": "America/Los_Angeles",
            "utc_offset_seconds": LA,
            "now_unix_ms": UTC_MIDNIGHT,
        }))
        .unwrap();
    assert_eq!(summary["providers"], json!([]));
    assert_eq!(summary["totals"]["token_status"], "unknown");
    assert!(summary["totals"]["reported"]["input_tokens"].is_null());
    assert_eq!(summary["window"]["utc_offset_seconds"], LA);
}

#[test]
fn local_midnight_boundary_decides_the_day() {
    let (_directory, api) = new_api();
    task_with_sessions(&api, &[("s-1", "codex")]);
    // The local day of 2026-01-01 in UTC-8 is [08:00Z Jan 1, 08:00Z Jan 2).
    let day_start = UTC_MIDNIGHT - 16 * HOUR;
    let day_end = UTC_MIDNIGHT + 8 * HOUR;
    for (key, end) in [
        ("before", day_start - 1),
        ("first", day_start),
        ("last", day_end - 1),
        ("after", day_end),
    ] {
        let mut value = receipt("s-1", key, end);
        value.input_tokens = Some(match key {
            "before" => 1000,
            "first" => 1,
            "last" => 10,
            _ => 100_000,
        });
        value.output_tokens = None;
        api.store.record_usage_receipt(&value).unwrap();
    }
    let summary = api
        .usage_summary(json!({
            "timezone": "America/Los_Angeles",
            "utc_offset_seconds": LA,
            "now_unix_ms": UTC_MIDNIGHT + 3 * HOUR,
        }))
        .unwrap();
    assert_eq!(summary["window"]["start_unix_ms"], day_start);
    assert_eq!(summary["window"]["end_unix_ms"], day_end);
    assert_eq!(summary["totals"]["reported"]["input_tokens"], 11);
    assert!(summary["totals"]["reported"]["output_tokens"].is_null());

    // A late receipt for that day reconciles into the same window.
    let mut late = receipt("s-1", "late", day_end - 5000);
    late.input_tokens = Some(5);
    late.output_tokens = None;
    late.reported_at_ms = day_end + 10 * HOUR;
    api.store.record_usage_receipt(&late).unwrap();
    let summary = api
        .usage_summary(json!({
            "timezone": "America/Los_Angeles",
            "utc_offset_seconds": LA,
            "now_unix_ms": UTC_MIDNIGHT + 3 * HOUR,
        }))
        .unwrap();
    assert_eq!(summary["totals"]["reported"]["input_tokens"], 16);
}

#[test]
fn estimated_figures_stay_apart_from_reported_ones() {
    let (_directory, api) = new_api();
    task_with_sessions(&api, &[("s-1", "codex")]);
    let now = now_ms();
    api.store
        .record_usage_receipt(&receipt("s-1", "turn.completed:1", now))
        .unwrap();
    let mut estimate = receipt("s-1", "estimate", now);
    estimate.source = UsageSource::Estimated;
    estimate.input_tokens = Some(7);
    estimate.output_tokens = None;
    api.store.record_usage_receipt(&estimate).unwrap();

    let summary = api.usage_summary(around_now()).unwrap();
    let totals = &summary["totals"];
    assert_eq!(totals["reported"]["input_tokens"], 100);
    assert_eq!(totals["estimated"]["input_tokens"], 7);
    assert!(totals["estimated"]["output_tokens"].is_null());
}

#[test]
fn bad_parameters_are_rejected() {
    let (_directory, api) = new_api();
    let now = now_ms();
    let bad = [
        json!({}),
        json!({"timezone": "UTC"}),
        json!({"utc_offset_seconds": 0}),
        json!({"timezone": "", "utc_offset_seconds": 0}),
        json!({"timezone": "UTC", "utc_offset_seconds": 15 * 3600}),
        json!({"timezone": "UTC", "utc_offset_seconds": -13 * 3600}),
        json!({"timezone": "x".repeat(65), "utc_offset_seconds": 0}),
        json!({"timezone": "UTC", "utc_offset_seconds": 0, "extra": 1}),
        json!({"timezone": "UTC", "utc_offset_seconds": 0, "now_unix_ms": -1}),
        json!({"timezone": "UTC", "utc_offset_seconds": 0, "window_start_unix_ms": now}),
        json!({"timezone": "UTC", "utc_offset_seconds": 0,
               "window_start_unix_ms": now, "window_end_unix_ms": now}),
        json!({"timezone": "UTC", "utc_offset_seconds": 0,
               "window_start_unix_ms": now, "window_end_unix_ms": now + 27 * HOUR}),
    ];
    for params in bad {
        assert!(
            matches!(
                api.usage_summary(params.clone()),
                Err(RpcError::InvalidParams)
            ),
            "{params}"
        );
    }
}

#[test]
fn usage_summary_is_dispatched_by_method_name() {
    let (_directory, api) = new_api();
    let value = api
        .dispatch(
            "usage.summary",
            json!({"timezone": "UTC", "utc_offset_seconds": 0, "now_unix_ms": UTC_MIDNIGHT}),
        )
        .unwrap();
    assert_eq!(value["window"]["start_unix_ms"], UTC_MIDNIGHT);
}
