//! SQLite persistence and window summaries for per-session AI usage receipts
//! (M7-06, migration 0020).
//!
//! A receipt is one provider observation. `NULL` figures are unknown and are
//! never aggregated as zero: a total is `None` unless at least one receipt in
//! scope carried that figure. Receipts are keyed by
//! `(task, session, source, receipt_key)`, so replaying an observation is a
//! no-op and a strictly newer `reported_at_ms` replaces the earlier values
//! (late-arriving reconcile). An equal or older observation never overwrites.
//! A receipt is attributed to the window containing its `window_end_ms`, not
//! to the moment it was stored, so a late receipt lands in the right day.
//!
//! Session start is the `session.created` durable event. Sessions created
//! before that event existed (stamped only by migration 0006's snapshot) have
//! no start time and are excluded from coverage counts. Their receipts, if any,
//! are still summed by `window_end_ms`.
//!
//! Updated: 2026-11-10T18:30:00+00:00.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use sqlx::Row;

use crate::{SqliteStore, StoreError};

const MAX_RECEIPT_KEY_BYTES: usize = 128;
/// Longest model name kept. Provider text beyond this is cut on a character
/// boundary rather than rejected, so an odd name never drops the figures.
const MAX_MODEL_BYTES: usize = 128;

fn truncate_model(model: String) -> String {
    if model.len() <= MAX_MODEL_BYTES {
        return model;
    }
    let mut end = MAX_MODEL_BYTES;
    while !model.is_char_boundary(end) {
        end -= 1;
    }
    model[..end].to_owned()
}

/// Who produced a receipt's figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageSource {
    /// The provider or its CLI stated the figures.
    ProviderReported,
    /// The daemon derived the figures. Kept apart from reported ones.
    Estimated,
}

impl UsageSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::Estimated => "estimated",
        }
    }

    fn from_column(value: &str) -> Self {
        if value == "estimated" {
            Self::Estimated
        } else {
            Self::ProviderReported
        }
    }
}

/// One observation to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReceipt {
    pub task_id: String,
    pub session_id: String,
    pub source: UsageSource,
    /// Stable within `(task, session, source)`; 1 to 128 bytes.
    pub receipt_key: String,
    pub window_start_ms: u64,
    /// Attribution time, Unix milliseconds.
    pub window_end_ms: u64,
    /// When the daemon observed it; orders revisions of the same key.
    pub reported_at_ms: u64,
    pub input_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    /// Nano-units and ISO-style currency code, both or neither.
    pub cost: Option<(u64, String)>,
    /// Provider text; stored cut to `MAX_MODEL_BYTES`.
    pub model: Option<String>,
}

/// What `record_usage_receipt` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptOutcome {
    Inserted,
    /// A strictly newer observation replaced different values.
    Replaced,
    /// Identical, equal-age, or older: the stored receipt is unchanged.
    Unchanged,
}

/// Summed figures for one source. `None` is unknown, never zero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageFigures {
    pub input_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    /// Nano-units per currency. Currencies are never summed together.
    pub costs: BTreeMap<String, u128>,
}

fn add_known(total: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *total = Some(total.unwrap_or(0).saturating_add(value));
    }
}

impl UsageFigures {
    fn add(&mut self, row: &ReceiptRow) {
        add_known(&mut self.input_tokens, row.input_tokens);
        add_known(&mut self.cache_read_tokens, row.cache_read_tokens);
        add_known(&mut self.cache_write_tokens, row.cache_write_tokens);
        add_known(&mut self.output_tokens, row.output_tokens);
        add_known(&mut self.reasoning_tokens, row.reasoning_tokens);
        if let Some((nanos, currency)) = &row.cost {
            *self.costs.entry(currency.clone()).or_insert(0) += u128::from(*nanos);
        }
    }

    fn merge(&mut self, other: &Self) {
        add_known(&mut self.input_tokens, other.input_tokens);
        add_known(&mut self.cache_read_tokens, other.cache_read_tokens);
        add_known(&mut self.cache_write_tokens, other.cache_write_tokens);
        add_known(&mut self.output_tokens, other.output_tokens);
        add_known(&mut self.reasoning_tokens, other.reasoning_tokens);
        for (currency, nanos) in &other.costs {
            *self.costs.entry(currency.clone()).or_insert(0) += nanos;
        }
    }
}

/// One provider's (or the grand) usage over a window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageBucket {
    /// Sessions that started in the window or have a receipt in it.
    pub sessions: u64,
    /// Of those, sessions with at least one receipt carrying a token count.
    pub sessions_with_tokens: u64,
    /// Of those, sessions with at least one receipt carrying a cost.
    pub sessions_with_cost: u64,
    pub reported: UsageFigures,
    pub estimated: UsageFigures,
}

/// Usage for one provider (`sessions.adapter`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUsage {
    pub provider: String,
    pub bucket: UsageBucket,
}

/// Usage over `[start_ms, end_ms)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageWindowSummary {
    pub totals: UsageBucket,
    /// Providers that had a session or receipt in the window, sorted by name.
    pub providers: Vec<ProviderUsage>,
}

struct ReceiptRow {
    adapter: String,
    task_id: String,
    session_id: String,
    source: UsageSource,
    input_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    cost: Option<(u64, String)>,
}

impl ReceiptRow {
    fn has_tokens(&self) -> bool {
        [
            self.input_tokens,
            self.cache_read_tokens,
            self.cache_write_tokens,
            self.output_tokens,
            self.reasoning_tokens,
        ]
        .iter()
        .any(Option::is_some)
    }
}

#[derive(Default)]
struct ProviderAccumulator {
    sessions: BTreeSet<(String, String)>,
    with_tokens: BTreeSet<(String, String)>,
    with_cost: BTreeSet<(String, String)>,
    reported: UsageFigures,
    estimated: UsageFigures,
}

fn invalid(detail: &'static str) -> StoreError {
    StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, detail))
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| invalid("usage figure exceeds the storable range"))
}

fn optional_i64(value: Option<u64>) -> Result<Option<i64>, StoreError> {
    value.map(to_i64).transpose()
}

fn optional_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Option<u64>, StoreError> {
    Ok(row
        .try_get::<Option<i64>, _>(column)?
        .and_then(|value| u64::try_from(value).ok()))
}

fn validate(receipt: &UsageReceipt) -> Result<(), StoreError> {
    if receipt.task_id.is_empty() || receipt.session_id.is_empty() {
        return Err(invalid("usage receipt needs a task and session id"));
    }
    if receipt.receipt_key.is_empty() || receipt.receipt_key.len() > MAX_RECEIPT_KEY_BYTES {
        return Err(invalid("usage receipt key must be 1 to 128 bytes"));
    }
    if receipt.window_end_ms < receipt.window_start_ms {
        return Err(invalid("usage receipt window ends before it starts"));
    }
    if let Some((_, currency)) = &receipt.cost {
        if !(3..=8).contains(&currency.len()) {
            return Err(invalid("usage receipt currency must be 3 to 8 bytes"));
        }
    }
    Ok(())
}

impl SqliteStore {
    /// Persist one usage observation idempotently. See the module docs for the
    /// replacement rule. Fails with `NoSession` for an unknown session.
    pub fn record_usage_receipt(
        &self,
        receipt: &UsageReceipt,
    ) -> Result<ReceiptOutcome, StoreError> {
        validate(receipt)?;
        let mut receipt = receipt.clone();
        receipt.model = receipt.model.take().map(truncate_model);
        let window_start = to_i64(receipt.window_start_ms)?;
        let window_end = to_i64(receipt.window_end_ms)?;
        let reported_at = to_i64(receipt.reported_at_ms)?;
        let input = optional_i64(receipt.input_tokens)?;
        let cache_read = optional_i64(receipt.cache_read_tokens)?;
        let cache_write = optional_i64(receipt.cache_write_tokens)?;
        let output = optional_i64(receipt.output_tokens)?;
        let reasoning = optional_i64(receipt.reasoning_tokens)?;
        let (cost_nanos, currency) = match &receipt.cost {
            Some((nanos, currency)) => (Some(to_i64(*nanos)?), Some(currency.clone())),
            None => (None, None),
        };
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let session_exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sessions WHERE task_id = ? AND id = ?",
            )
            .bind(&receipt.task_id)
            .bind(&receipt.session_id)
            .fetch_one(&mut *transaction)
            .await?;
            if session_exists == 0 {
                return Err(StoreError::NoSession {
                    session_id: receipt.session_id,
                    task_id: receipt.task_id,
                });
            }
            let existing = sqlx::query(
                "SELECT sequence, reported_at_ms FROM usage_receipts WHERE task_id = ? AND session_id = ? AND source = ? AND receipt_key = ?",
            )
            .bind(&receipt.task_id)
            .bind(&receipt.session_id)
            .bind(receipt.source.as_str())
            .bind(&receipt.receipt_key)
            .fetch_optional(&mut *transaction)
            .await?;
            let outcome = match existing {
                None => {
                    sqlx::query(
                        "INSERT INTO usage_receipts (task_id, session_id, source, receipt_key, window_start_ms, window_end_ms, reported_at_ms, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens, cost_nanos, currency, model) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&receipt.task_id)
                    .bind(&receipt.session_id)
                    .bind(receipt.source.as_str())
                    .bind(&receipt.receipt_key)
                    .bind(window_start)
                    .bind(window_end)
                    .bind(reported_at)
                    .bind(input)
                    .bind(cache_read)
                    .bind(cache_write)
                    .bind(output)
                    .bind(reasoning)
                    .bind(cost_nanos)
                    .bind(&currency)
                    .bind(&receipt.model)
                    .execute(&mut *transaction)
                    .await?;
                    ReceiptOutcome::Inserted
                }
                Some(row) if row.try_get::<i64, _>("reported_at_ms")? < reported_at => {
                    let updated = sqlx::query(
                        "UPDATE usage_receipts SET window_start_ms = ?, window_end_ms = ?, reported_at_ms = ?, input_tokens = ?, cache_read_tokens = ?, cache_write_tokens = ?, output_tokens = ?, reasoning_tokens = ?, cost_nanos = ?, currency = ?, model = ? WHERE sequence = ? AND (window_start_ms, window_end_ms, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens, cost_nanos, currency, model) IS NOT (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(window_start)
                    .bind(window_end)
                    .bind(reported_at)
                    .bind(input)
                    .bind(cache_read)
                    .bind(cache_write)
                    .bind(output)
                    .bind(reasoning)
                    .bind(cost_nanos)
                    .bind(&currency)
                    .bind(&receipt.model)
                    .bind(row.try_get::<i64, _>("sequence")?)
                    .bind(window_start)
                    .bind(window_end)
                    .bind(input)
                    .bind(cache_read)
                    .bind(cache_write)
                    .bind(output)
                    .bind(reasoning)
                    .bind(cost_nanos)
                    .bind(&currency)
                    .bind(&receipt.model)
                    .execute(&mut *transaction)
                    .await?
                    .rows_affected();
                    if updated == 0 {
                        ReceiptOutcome::Unchanged
                    } else {
                        ReceiptOutcome::Replaced
                    }
                }
                Some(_) => ReceiptOutcome::Unchanged,
            };
            transaction.commit().await?;
            Ok(outcome)
        })
    }

    /// Summarize usage over `[start_ms, end_ms)` by provider.
    ///
    /// Receipts are selected by `window_end_ms`. Coverage counts sessions that
    /// started in the window (via their `session.created` event) plus sessions
    /// that have a receipt in it.
    pub fn usage_window_summary(
        &self,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<UsageWindowSummary, StoreError> {
        if end_ms <= start_ms {
            return Err(invalid("usage window must end after it starts"));
        }
        let start = to_i64(start_ms)?;
        let end = to_i64(end_ms)?;
        self.call(move |pool| async move {
            let started = sqlx::query(
                "SELECT s.adapter AS adapter, s.task_id AS task_id, s.id AS session_id FROM sessions s JOIN durable_events e ON e.task_id = s.task_id AND e.session_id = s.id AND e.kind = 'session.created' WHERE CAST(strftime('%s', e.created_at) AS INTEGER) * 1000 >= ? AND CAST(strftime('%s', e.created_at) AS INTEGER) * 1000 < ?",
            )
            .bind(start)
            .bind(end)
            .fetch_all(&pool)
            .await?;
            let receipts = sqlx::query(
                "SELECT s.adapter AS adapter, r.task_id AS task_id, r.session_id AS session_id, r.source AS source, r.input_tokens AS input_tokens, r.cache_read_tokens AS cache_read_tokens, r.cache_write_tokens AS cache_write_tokens, r.output_tokens AS output_tokens, r.reasoning_tokens AS reasoning_tokens, r.cost_nanos AS cost_nanos, r.currency AS currency FROM usage_receipts r JOIN sessions s ON s.task_id = r.task_id AND s.id = r.session_id WHERE r.window_end_ms >= ? AND r.window_end_ms < ?",
            )
            .bind(start)
            .bind(end)
            .fetch_all(&pool)
            .await?;

            let mut providers: BTreeMap<String, ProviderAccumulator> = BTreeMap::new();
            for row in started {
                providers
                    .entry(row.try_get("adapter")?)
                    .or_default()
                    .sessions
                    .insert((row.try_get("task_id")?, row.try_get("session_id")?));
            }
            for row in receipts {
                let cost = match (
                    optional_u64(&row, "cost_nanos")?,
                    row.try_get::<Option<String>, _>("currency")?,
                ) {
                    (Some(nanos), Some(currency)) => Some((nanos, currency)),
                    _ => None,
                };
                let parsed = ReceiptRow {
                    adapter: row.try_get("adapter")?,
                    task_id: row.try_get("task_id")?,
                    session_id: row.try_get("session_id")?,
                    source: UsageSource::from_column(&row.try_get::<String, _>("source")?),
                    input_tokens: optional_u64(&row, "input_tokens")?,
                    cache_read_tokens: optional_u64(&row, "cache_read_tokens")?,
                    cache_write_tokens: optional_u64(&row, "cache_write_tokens")?,
                    output_tokens: optional_u64(&row, "output_tokens")?,
                    reasoning_tokens: optional_u64(&row, "reasoning_tokens")?,
                    cost,
                };
                let key = (parsed.task_id.clone(), parsed.session_id.clone());
                let provider = providers.entry(parsed.adapter.clone()).or_default();
                provider.sessions.insert(key.clone());
                if parsed.has_tokens() {
                    provider.with_tokens.insert(key.clone());
                }
                if parsed.cost.is_some() {
                    provider.with_cost.insert(key);
                }
                match parsed.source {
                    UsageSource::ProviderReported => provider.reported.add(&parsed),
                    UsageSource::Estimated => provider.estimated.add(&parsed),
                }
            }

            let mut summary = UsageWindowSummary::default();
            for (provider, accumulator) in providers {
                let bucket = UsageBucket {
                    sessions: accumulator.sessions.len() as u64,
                    sessions_with_tokens: accumulator.with_tokens.len() as u64,
                    sessions_with_cost: accumulator.with_cost.len() as u64,
                    reported: accumulator.reported,
                    estimated: accumulator.estimated,
                };
                summary.totals.sessions += bucket.sessions;
                summary.totals.sessions_with_tokens += bucket.sessions_with_tokens;
                summary.totals.sessions_with_cost += bucket.sessions_with_cost;
                summary.totals.reported.merge(&bucket.reported);
                summary.totals.estimated.merge(&bucket.estimated);
                summary.providers.push(ProviderUsage { provider, bucket });
            }
            Ok(summary)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };
    use orchestrator_core::records::Project;
    use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};

    const DAY_START: u64 = 1_767_312_000_000;
    const DAY_END: u64 = 1_767_398_400_000;
    const NOON: u64 = DAY_START + 12 * 3_600_000;

    fn store_with_sessions(path: &std::path::Path) -> SqliteStore {
        let store = SqliteStore::new(path).unwrap();
        store
            .register_project(&Project {
                id: "p".into(),
                root_path: "/repo".into(),
                settings: serde_json::json!({}),
            })
            .unwrap();
        store
            .create(&TaskContract {
                id: "t1".into(),
                project_id: "p".into(),
                parent_id: None,
                objective: "x".into(),
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
            })
            .unwrap();
        store.transition("t1", TaskEvent::Admit).unwrap();
        store
            .add_attempt(
                "t1",
                &Attempt {
                    id: "a1".into(),
                    task_id: "t1".into(),
                    attempt_number: 1,
                    state: AttemptState::Pending,
                },
            )
            .unwrap();
        for (id, adapter) in [
            ("s-codex", "codex"),
            ("s-claude", "claude-code"),
            ("s-pi", "pi"),
        ] {
            store
                .add_session(
                    "t1",
                    &Session {
                        id: id.into(),
                        attempt_id: "a1".into(),
                        adapter: adapter.into(),
                        state: SessionState::Spawning,
                    },
                )
                .unwrap();
        }
        // Pin every session start inside the test day.
        store
            .call(|pool| async move {
                sqlx::query("UPDATE durable_events SET created_at = '2026-01-02 03:04:05' WHERE kind = 'session.created'")
                    .execute(&pool)
                    .await?;
                Ok(())
            })
            .unwrap();
        store
    }

    fn receipt(session: &str, key: &str, end: u64, reported_at: u64) -> UsageReceipt {
        UsageReceipt {
            task_id: "t1".into(),
            session_id: session.into(),
            source: UsageSource::ProviderReported,
            receipt_key: key.into(),
            window_start_ms: end - 1000,
            window_end_ms: end,
            reported_at_ms: reported_at,
            input_tokens: Some(10),
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: Some(5),
            reasoning_tokens: None,
            cost: None,
            model: None,
        }
    }

    fn provider<'a>(summary: &'a UsageWindowSummary, name: &str) -> &'a UsageBucket {
        &summary
            .providers
            .iter()
            .find(|entry| entry.provider == name)
            .unwrap()
            .bucket
    }

    #[test]
    fn receipts_are_idempotent_and_only_newer_observations_replace() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let first = receipt("s-codex", "turn.completed:1", NOON, 100);
        assert_eq!(
            store.record_usage_receipt(&first).unwrap(),
            ReceiptOutcome::Inserted
        );
        assert_eq!(
            store.record_usage_receipt(&first).unwrap(),
            ReceiptOutcome::Unchanged
        );

        let mut older = first.clone();
        older.reported_at_ms = 50;
        older.input_tokens = Some(999);
        assert_eq!(
            store.record_usage_receipt(&older).unwrap(),
            ReceiptOutcome::Unchanged
        );
        let mut tie = first.clone();
        tie.input_tokens = Some(999);
        assert_eq!(
            store.record_usage_receipt(&tie).unwrap(),
            ReceiptOutcome::Unchanged
        );

        let mut newer = first.clone();
        newer.reported_at_ms = 200;
        newer.input_tokens = Some(11);
        assert_eq!(
            store.record_usage_receipt(&newer).unwrap(),
            ReceiptOutcome::Replaced
        );
        newer.reported_at_ms = 300;
        assert_eq!(
            store.record_usage_receipt(&newer).unwrap(),
            ReceiptOutcome::Unchanged
        );

        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        let codex = provider(&summary, "codex");
        assert_eq!(
            codex.reported.input_tokens,
            Some(11),
            "one receipt, latest values"
        );
        assert_eq!(codex.reported.output_tokens, Some(5));
    }

    #[test]
    fn receipts_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("s.sqlite");
        drop({
            let store = store_with_sessions(&path);
            store
                .record_usage_receipt(&receipt("s-pi", "response:r1", NOON, 1))
                .unwrap();
            store
        });
        let reopened = SqliteStore::new(&path).unwrap();
        let summary = reopened.usage_window_summary(DAY_START, DAY_END).unwrap();
        assert_eq!(provider(&summary, "pi").reported.input_tokens, Some(10));
        assert_eq!(
            reopened
                .record_usage_receipt(&receipt("s-pi", "response:r1", NOON, 1))
                .unwrap(),
            ReceiptOutcome::Unchanged
        );
    }

    #[test]
    fn late_receipt_is_attributed_to_its_window_end_day() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let yesterday_end = DAY_START - 60_000;
        store
            .record_usage_receipt(&receipt(
                "s-pi",
                "response:late",
                yesterday_end,
                DAY_START + 1,
            ))
            .unwrap();
        let yesterday = store
            .usage_window_summary(DAY_START - 86_400_000, DAY_START)
            .unwrap();
        assert_eq!(provider(&yesterday, "pi").reported.input_tokens, Some(10));
        let today = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        assert_eq!(provider(&today, "pi").reported.input_tokens, None);
        // The window is half-open: the boundary instant belongs to the later day.
        store
            .record_usage_receipt(&receipt("s-pi", "response:edge", DAY_START, DAY_START + 2))
            .unwrap();
        let today = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        assert_eq!(provider(&today, "pi").reported.input_tokens, Some(10));
        let yesterday = store
            .usage_window_summary(DAY_START - 86_400_000, DAY_START)
            .unwrap();
        assert_eq!(provider(&yesterday, "pi").reported.input_tokens, Some(10));
    }

    #[test]
    fn silent_providers_are_unknown_not_zero_and_sources_stay_apart() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let mut claude = receipt("s-claude", "result", NOON, 1);
        claude.cost = Some((8_638_800, "USD".into()));
        store.record_usage_receipt(&claude).unwrap();
        let mut estimate = receipt("s-claude", "result", NOON, 1);
        estimate.source = UsageSource::Estimated;
        estimate.input_tokens = Some(7);
        estimate.cost = Some((1, "EUR".into()));
        store.record_usage_receipt(&estimate).unwrap();
        store
            .record_usage_receipt(&receipt("s-codex", "turn.completed:1", NOON, 1))
            .unwrap();

        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        let pi = provider(&summary, "pi");
        assert_eq!(
            (pi.sessions, pi.sessions_with_tokens, pi.sessions_with_cost),
            (1, 0, 0)
        );
        assert_eq!(pi.reported, UsageFigures::default());
        assert_eq!(pi.estimated, UsageFigures::default());

        let codex = provider(&summary, "codex");
        assert_eq!(codex.sessions_with_tokens, 1);
        assert_eq!(codex.sessions_with_cost, 0);
        assert!(
            codex.reported.costs.is_empty(),
            "no cost reported is not a zero cost"
        );

        let claude = provider(&summary, "claude-code");
        assert_eq!(claude.reported.input_tokens, Some(10));
        assert_eq!(claude.estimated.input_tokens, Some(7));
        assert_eq!(claude.reported.costs.get("USD"), Some(&8_638_800));
        assert_eq!(claude.estimated.costs.get("EUR"), Some(&1));
        assert_eq!(
            claude.estimated.costs.get("USD"),
            None,
            "currencies never merge"
        );

        assert_eq!(summary.totals.sessions, 3);
        assert_eq!(summary.totals.sessions_with_tokens, 2);
        assert_eq!(summary.totals.reported.input_tokens, Some(20));
    }

    #[test]
    fn a_null_field_inside_a_receipt_stays_unknown_not_zero() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        // Cost-only receipt, like a Claude result that reported no tokens.
        let mut cost_only = receipt("s-claude", "result", NOON, 1);
        cost_only.input_tokens = None;
        cost_only.output_tokens = None;
        cost_only.cost = Some((5, "USD".into()));
        store.record_usage_receipt(&cost_only).unwrap();
        // Tokens without a cache figure.
        store
            .record_usage_receipt(&receipt("s-codex", "turn.completed:1", NOON, 1))
            .unwrap();

        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        let claude = provider(&summary, "claude-code");
        assert_eq!(claude.reported.input_tokens, None);
        assert_eq!(claude.reported.output_tokens, None);
        assert_eq!(
            (
                claude.sessions,
                claude.sessions_with_tokens,
                claude.sessions_with_cost
            ),
            (1, 0, 1),
            "a cost-only receipt covers the session for cost, not tokens"
        );
        let codex = provider(&summary, "codex");
        assert_eq!(codex.reported.input_tokens, Some(10));
        assert_eq!(codex.reported.cache_read_tokens, None);
        assert_eq!(codex.reported.cache_write_tokens, None);
        assert_eq!(codex.reported.reasoning_tokens, None);
        assert_eq!(summary.totals.reported.cache_read_tokens, None);
        assert_eq!(summary.totals.reported.input_tokens, Some(10));
    }

    #[test]
    fn session_coverage_follows_start_time_and_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let set_start = |session: &'static str, at: &'static str| {
            store
                .call(move |pool| async move {
                    sqlx::query("UPDATE durable_events SET created_at = ? WHERE kind = 'session.created' AND session_id = ?")
                        .bind(at)
                        .bind(session)
                        .execute(&pool)
                        .await?;
                    Ok(())
                })
                .unwrap();
        };
        // DAY_START is 2026-01-02 00:00:00 UTC; DAY_END is the next midnight.
        set_start("s-codex", "2026-01-02 00:00:00");
        set_start("s-claude", "2026-01-03 00:00:00");
        set_start("s-pi", "2026-01-01 23:59:59");
        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        let sessions = |name: &str| {
            summary
                .providers
                .iter()
                .find(|entry| entry.provider == name)
                .map(|entry| entry.bucket.sessions)
        };
        assert_eq!(sessions("codex"), Some(1), "start is inclusive");
        assert_eq!(sessions("claude-code"), None, "end is exclusive");
        assert_eq!(sessions("pi"), None, "before the window");

        // A receipt in the window covers a session that started elsewhere.
        store
            .record_usage_receipt(&receipt("s-pi", "m", NOON, 1))
            .unwrap();
        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        let pi = provider(&summary, "pi");
        assert_eq!((pi.sessions, pi.sessions_with_tokens), (1, 1));
    }

    #[test]
    fn costs_are_summed_per_provider_and_in_the_grand_total() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        for (session, key, nanos) in [
            ("s-claude", "result", 100),
            ("s-claude", "result-2", 250),
            ("s-codex", "cost", 7),
        ] {
            let mut priced = receipt(session, key, NOON, 1);
            priced.cost = Some((nanos, "USD".into()));
            store.record_usage_receipt(&priced).unwrap();
        }
        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        assert_eq!(
            provider(&summary, "claude-code").reported.costs.get("USD"),
            Some(&350)
        );
        assert_eq!(
            provider(&summary, "codex").reported.costs.get("USD"),
            Some(&7)
        );
        assert_eq!(summary.totals.reported.costs.get("USD"), Some(&357));
        assert_eq!(summary.totals.sessions_with_cost, 2);
    }

    #[test]
    fn token_sums_saturate_instead_of_wrapping() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        for key in ["a", "b", "c"] {
            let mut huge = receipt("s-codex", key, NOON, 1);
            huge.input_tokens = Some(i64::MAX as u64);
            store.record_usage_receipt(&huge).unwrap();
        }
        let summary = store.usage_window_summary(DAY_START, DAY_END).unwrap();
        assert_eq!(
            provider(&summary, "codex").reported.input_tokens,
            Some(u64::MAX)
        );
    }

    #[test]
    fn model_text_is_truncated_on_a_character_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let stored = |key: &'static str| {
            store
                .call(move |pool| async move {
                    Ok(sqlx::query_scalar::<_, Option<String>>(
                        "SELECT model FROM usage_receipts WHERE receipt_key = ?",
                    )
                    .bind(key)
                    .fetch_one(&pool)
                    .await?)
                })
                .unwrap()
        };
        let mut long = receipt("s-claude", "long", NOON, 1);
        long.model = Some("m".repeat(MAX_MODEL_BYTES + 500));
        store.record_usage_receipt(&long).unwrap();
        assert_eq!(stored("long"), Some("m".repeat(MAX_MODEL_BYTES)));

        // A 3-byte character straddling the limit is dropped whole.
        let mut wide = receipt("s-claude", "wide", NOON, 1);
        wide.model = Some(format!("{}{}", "m".repeat(MAX_MODEL_BYTES - 1), "€€"));
        store.record_usage_receipt(&wide).unwrap();
        assert_eq!(stored("wide"), Some("m".repeat(MAX_MODEL_BYTES - 1)));

        let mut exact = receipt("s-claude", "exact", NOON, 1);
        exact.model = Some("m".repeat(MAX_MODEL_BYTES));
        store.record_usage_receipt(&exact).unwrap();
        assert_eq!(stored("exact"), Some("m".repeat(MAX_MODEL_BYTES)));
        store
            .record_usage_receipt(&receipt("s-claude", "none", NOON, 1))
            .unwrap();
        assert_eq!(stored("none"), None);
    }

    #[test]
    fn an_empty_window_has_no_providers() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let summary = store
            .usage_window_summary(DAY_END, DAY_END + 86_400_000)
            .unwrap();
        assert_eq!(summary, UsageWindowSummary::default());
        assert!(store.usage_window_summary(DAY_END, DAY_END).is_err());
    }

    #[test]
    fn invalid_receipts_and_unknown_sessions_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_sessions(&directory.path().join("s.sqlite"));
        let mut ghost = receipt("ghost", "k", NOON, 1);
        assert!(matches!(
            store.record_usage_receipt(&ghost),
            Err(StoreError::NoSession { .. })
        ));
        ghost.session_id = "s-pi".into();
        ghost.receipt_key = String::new();
        assert!(store.record_usage_receipt(&ghost).is_err());
        ghost.receipt_key = "k".repeat(129);
        assert!(store.record_usage_receipt(&ghost).is_err());
        ghost.receipt_key = "k".into();
        ghost.window_start_ms = ghost.window_end_ms + 1;
        assert!(store.record_usage_receipt(&ghost).is_err());
        ghost.window_start_ms = 0;
        ghost.input_tokens = Some(u64::MAX);
        assert!(store.record_usage_receipt(&ghost).is_err());
        ghost.input_tokens = None;
        ghost.cost = Some((1, "U".into()));
        assert!(store.record_usage_receipt(&ghost).is_err());
    }

    #[test]
    fn migration_applies_to_an_existing_data_directory_without_losing_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("s.sqlite");
        drop(store_with_sessions(&path));
        // Rewind to the pre-0020 schema, as an upgraded data directory would be.
        let rewound = SqliteStore::new(&path).unwrap();
        rewound
            .call(|pool| async move {
                sqlx::query("DROP TABLE usage_receipts")
                    .execute(&pool)
                    .await?;
                sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 20")
                    .execute(&pool)
                    .await?;
                Ok(())
            })
            .unwrap();
        drop(rewound);

        let upgraded = SqliteStore::new(&path).unwrap();
        assert_eq!(upgraded.get("t1").unwrap().contract.id, "t1");
        assert_eq!(upgraded.get("t1").unwrap().sessions.len(), 3);
        let summary = upgraded.usage_window_summary(DAY_START, DAY_END).unwrap();
        assert_eq!(summary.totals.sessions, 3);
        assert_eq!(summary.totals.sessions_with_tokens, 0);
        upgraded
            .record_usage_receipt(&receipt("s-pi", "response:r1", NOON, 1))
            .unwrap();
        let versions = upgraded
            .call(|pool| async move {
                Ok(sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version",
                )
                .fetch_all(&pool)
                .await?)
            })
            .unwrap();
        assert!(versions.contains(&20));
    }
}
