//! Usage receipts for chat turns (DPES-1105 phase 3a, migration 0040).
//!
//! A receipt is figures only, never text, and has no foreign key: a history clear deletes
//! turns, and the wake token budget must not reset because the user cleared a transcript.
//! `usage_receipts` cannot hold a chat receipt (it references a task session), so chat turns
//! have their own table. The rule for replacing a receipt is the one of
//! [`SqliteStore::record_usage_receipt`]: a strictly newer observation of the same key
//! replaces different figures.

use std::io;

use sqlx::Row;

use crate::sqlite_usage::ReceiptOutcome;
use crate::{SqliteStore, StoreError};

const MAX_RECEIPT_KEY_BYTES: usize = 128;
/// Longest model name kept, cut on a character boundary like `usage_receipts`.
const MAX_MODEL_BYTES: usize = 128;

/// What opened a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnTrigger {
    /// A message the user sent. Every turn before migration 0040.
    User,
    /// An automatic wake turn started by the daemon.
    Wake,
}

impl TurnTrigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Wake => "wake",
        }
    }

    pub(crate) fn from_column(value: &str) -> Result<Self, StoreError> {
        match value {
            "user" => Ok(Self::User),
            "wake" => Ok(Self::Wake),
            other => Err(crate::sqlite_conversations::corrupt(format!(
                "unknown turn trigger {other}"
            ))),
        }
    }
}

/// Figures one chat turn reported. `None` is unknown, never zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationUsageReceipt {
    pub conversation_id: String,
    pub turn_id: String,
    pub trigger: TurnTrigger,
    /// 1 to 128 bytes, from the provider's usage report.
    pub receipt_key: String,
    pub reported_at_ms: u64,
    pub input_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    /// Nano-units and ISO-style currency code, both or neither.
    pub cost: Option<(u64, String)>,
    /// Provider text; stored cut to 128 bytes.
    pub model: Option<String>,
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

fn truncate_model(mut model: String) -> String {
    if model.len() > MAX_MODEL_BYTES {
        let mut end = MAX_MODEL_BYTES;
        while !model.is_char_boundary(end) {
            end -= 1;
        }
        model.truncate(end);
    }
    model
}

impl SqliteStore {
    /// Idempotent on (turn id, receipt key). A strictly newer `reported_at_ms` replaces
    /// different values; an identical, equal-age or older observation changes nothing.
    pub fn record_conversation_usage(
        &self,
        receipt: &ConversationUsageReceipt,
    ) -> Result<ReceiptOutcome, StoreError> {
        if receipt.conversation_id.is_empty() || receipt.turn_id.is_empty() {
            return Err(invalid(
                "chat usage receipt needs a conversation and turn id",
            ));
        }
        if receipt.receipt_key.is_empty() || receipt.receipt_key.len() > MAX_RECEIPT_KEY_BYTES {
            return Err(invalid("usage receipt key must be 1 to 128 bytes"));
        }
        if let Some((_, currency)) = &receipt.cost {
            if !(3..=8).contains(&currency.len()) {
                return Err(invalid("usage receipt currency must be 3 to 8 bytes"));
            }
        }
        let receipt = ConversationUsageReceipt {
            model: receipt.model.clone().map(truncate_model),
            ..receipt.clone()
        };
        let reported_at = to_i64(receipt.reported_at_ms)?;
        let figures = [
            optional_i64(receipt.input_tokens)?,
            optional_i64(receipt.cache_read_tokens)?,
            optional_i64(receipt.cache_write_tokens)?,
            optional_i64(receipt.output_tokens)?,
            optional_i64(receipt.reasoning_tokens)?,
        ];
        let (cost_nanos, currency) = match &receipt.cost {
            Some((nanos, currency)) => (Some(to_i64(*nanos)?), Some(currency.clone())),
            None => (None, None),
        };
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let existing = sqlx::query(
                "SELECT sequence, reported_at_ms FROM conversation_usage_receipts \
                 WHERE turn_id = ? AND receipt_key = ?",
            )
            .bind(&receipt.turn_id)
            .bind(&receipt.receipt_key)
            .fetch_optional(&mut *transaction)
            .await?;
            let outcome = match existing {
                None => {
                    sqlx::query(
                        "INSERT INTO conversation_usage_receipts \
                         (conversation_id, turn_id, trigger_kind, receipt_key, reported_at_ms, \
                          input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, \
                          reasoning_tokens, cost_nanos, currency, model) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&receipt.conversation_id)
                    .bind(&receipt.turn_id)
                    .bind(receipt.trigger.as_str())
                    .bind(&receipt.receipt_key)
                    .bind(reported_at)
                    .bind(figures[0])
                    .bind(figures[1])
                    .bind(figures[2])
                    .bind(figures[3])
                    .bind(figures[4])
                    .bind(cost_nanos)
                    .bind(&currency)
                    .bind(&receipt.model)
                    .execute(&mut *transaction)
                    .await?;
                    ReceiptOutcome::Inserted
                }
                Some(row) if row.try_get::<i64, _>("reported_at_ms")? < reported_at => {
                    let updated = sqlx::query(
                        "UPDATE conversation_usage_receipts SET reported_at_ms = ?, \
                         input_tokens = ?, cache_read_tokens = ?, cache_write_tokens = ?, \
                         output_tokens = ?, reasoning_tokens = ?, cost_nanos = ?, \
                         currency = ?, model = ? \
                         WHERE sequence = ? AND (input_tokens, cache_read_tokens, \
                         cache_write_tokens, output_tokens, reasoning_tokens, cost_nanos, \
                         currency, model) IS NOT (?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(reported_at)
                    .bind(figures[0])
                    .bind(figures[1])
                    .bind(figures[2])
                    .bind(figures[3])
                    .bind(figures[4])
                    .bind(cost_nanos)
                    .bind(&currency)
                    .bind(&receipt.model)
                    .bind(row.try_get::<i64, _>("sequence")?)
                    .bind(figures[0])
                    .bind(figures[1])
                    .bind(figures[2])
                    .bind(figures[3])
                    .bind(figures[4])
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

    /// Tokens a scope's wake turns used at or after `since_ms`: input, cache read, cache
    /// write and output (reasoning is already inside output). An unreported figure counts as
    /// 0 here; the hourly and daily caps still bound the rate. User turns are never counted.
    pub fn wake_tokens_since(
        &self,
        conversation_id: &str,
        since_ms: u64,
    ) -> Result<u64, StoreError> {
        let conversation_id = conversation_id.to_owned();
        let since = to_i64(since_ms)?;
        self.call(move |pool| async move {
            let total: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(COALESCE(input_tokens, 0) + COALESCE(cache_read_tokens, 0) \
                 + COALESCE(cache_write_tokens, 0) + COALESCE(output_tokens, 0)), 0) \
                 FROM conversation_usage_receipts \
                 WHERE conversation_id = ? AND trigger_kind = 'wake' AND reported_at_ms >= ?",
            )
            .bind(&conversation_id)
            .bind(since)
            .fetch_one(&pool)
            .await?;
            Ok(u64::try_from(total).unwrap_or(0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConversationTarget, TurnOutcome};

    fn open() -> (tempfile::TempDir, SqliteStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
        (directory, store)
    }

    fn receipt(
        turn_id: &str,
        trigger: TurnTrigger,
        reported_at_ms: u64,
    ) -> ConversationUsageReceipt {
        ConversationUsageReceipt {
            conversation_id: "conversation:global".into(),
            turn_id: turn_id.into(),
            trigger,
            receipt_key: "usage".into(),
            reported_at_ms,
            input_tokens: Some(10),
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: Some(5),
            reasoning_tokens: None,
            cost: None,
            model: Some("model-a".into()),
        }
    }

    #[test]
    fn a_chat_receipt_is_idempotent_and_a_newer_one_replaces() {
        let (_directory, store) = open();
        let first = receipt("turn-1", TurnTrigger::Wake, 1_000);
        assert_eq!(
            store.record_conversation_usage(&first).unwrap(),
            ReceiptOutcome::Inserted
        );
        assert_eq!(
            store.record_conversation_usage(&first).unwrap(),
            ReceiptOutcome::Unchanged
        );

        let mut newer = first.clone();
        newer.reported_at_ms = 2_000;
        newer.output_tokens = Some(50);
        assert_eq!(
            store.record_conversation_usage(&newer).unwrap(),
            ReceiptOutcome::Replaced
        );
        // Same figures again at a later time changes nothing.
        newer.reported_at_ms = 3_000;
        assert_eq!(
            store.record_conversation_usage(&newer).unwrap(),
            ReceiptOutcome::Unchanged
        );
        // An older observation never replaces a newer one.
        assert_eq!(
            store.record_conversation_usage(&first).unwrap(),
            ReceiptOutcome::Unchanged
        );
        assert_eq!(
            store.wake_tokens_since("conversation:global", 0).unwrap(),
            50 + 10
        );
    }

    #[test]
    fn chat_receipts_survive_a_history_clear() {
        let (_directory, store) = open();
        let target = ConversationTarget::Global;
        store
            .begin_conversation_turn_as(&target, "turn-1", "hi", "codex", "1", TurnTrigger::Wake)
            .unwrap();
        store
            .finish_conversation_turn("turn-1", &TurnOutcome::Completed)
            .unwrap();
        store
            .record_conversation_usage(&receipt("turn-1", TurnTrigger::Wake, 1_000))
            .unwrap();
        let before = store.wake_tokens_since("conversation:global", 0).unwrap();
        assert_eq!(before, 15);

        let outcome = store.clear_conversation_history(&target).unwrap();
        assert!(outcome.cleared);
        assert!(store.conversation_turn("turn-1").unwrap().is_none());
        assert_eq!(
            store.wake_tokens_since("conversation:global", 0).unwrap(),
            before
        );
    }

    #[test]
    fn wake_tokens_sum_only_wake_turns_in_the_window() {
        let (_directory, store) = open();
        let mut wake_old = receipt("wake-old", TurnTrigger::Wake, 1_000);
        wake_old.input_tokens = Some(1_000);
        let mut wake_new = receipt("wake-new", TurnTrigger::Wake, 9_000);
        wake_new.input_tokens = Some(7);
        wake_new.output_tokens = None;
        wake_new.cache_read_tokens = Some(3);
        wake_new.cache_write_tokens = Some(2);
        // Unreported figures are NULL and count as 0.
        let mut wake_null = receipt("wake-null", TurnTrigger::Wake, 9_500);
        wake_null.input_tokens = None;
        wake_null.output_tokens = None;
        let user = receipt("user-1", TurnTrigger::User, 9_000);
        let mut other_scope = receipt("wake-other", TurnTrigger::Wake, 9_000);
        other_scope.conversation_id = "conversation:workstream:w1".into();
        for item in [&wake_old, &wake_new, &wake_null, &user, &other_scope] {
            store.record_conversation_usage(item).unwrap();
        }
        // wake-old is before the window; user and other scope are excluded.
        assert_eq!(
            store
                .wake_tokens_since("conversation:global", 5_000)
                .unwrap(),
            12
        );
        assert_eq!(
            store
                .wake_tokens_since("conversation:global", 1_000)
                .unwrap(),
            1_017
        );
        assert_eq!(
            store
                .wake_tokens_since("conversation:global", 9_501)
                .unwrap(),
            0
        );
    }
}
