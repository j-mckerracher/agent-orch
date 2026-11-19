//! Preferences and learnings (DPES-1108 phase 6, migration 0067).
//!
//! A document is the entries of one (kind, scope); [`orchestrator_core::memory`] holds the
//! rules. Every write runs in one `BEGIN IMMEDIATE` transaction that reads the document, checks
//! the budget and then writes, so two writers cannot both fit the last bytes. An entry that
//! leaves a document (removed, or replaced by a new text) is copied to `memory_archive` in the
//! same transaction, so nothing a user or an orchestrator wrote is ever lost, and the user can
//! restore it. The store checks the core rules again, so a caller that skipped them still
//! cannot plant a refused entry; a refusal is a stable code, never the entry text.

use std::io;

use orchestrator_core::memory::{
    budget, check_budget, check_entry, entry_bytes, MemoryKind, MemoryScope, OverBudget,
};
use sqlx::SqliteConnection;

use crate::{ConversationTarget, SqliteStore, StoreError};

/// Most archived entries one listing returns, newest first.
pub const MEMORY_ARCHIVE_LISTED: usize = 50;

/// Who wrote an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryWriter {
    /// The user, through Settings.
    User,
    /// An orchestrator conversation, through an approved `memory.set` proposal.
    Orchestrator,
}

impl MemoryWriter {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Orchestrator => "orchestrator",
        }
    }

    fn parse(value: &str) -> Self {
        if value == "orchestrator" {
            Self::Orchestrator
        } else {
            Self::User
        }
    }
}

/// One entry of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEntry {
    pub key: String,
    pub text: String,
    pub written_by: MemoryWriter,
    pub created_at_ms: u64,
    pub confirmed_at_ms: u64,
}

impl MemoryEntry {
    /// The bytes this entry takes from its document's budget.
    pub fn bytes(&self) -> usize {
        entry_bytes(&self.key, &self.text)
    }
}

/// One entry that left its document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedMemoryEntry {
    pub id: i64,
    pub key: String,
    pub text: String,
    pub written_by: MemoryWriter,
    pub archived_at_ms: u64,
    /// `removed` or `replaced`.
    pub reason: String,
    pub restored: bool,
}

/// What a write did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryWriteOutcome {
    /// A new key.
    Created,
    /// The key held another text, which is now archived as `replaced`.
    Updated,
    /// The key held the same text; only its last-confirmed time moved.
    Confirmed,
    /// Nothing was written: the document would have grown past its budget.
    OverBudget(OverBudget),
}

impl MemoryWriteOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Confirmed => "confirmed",
            Self::OverBudget(_) => "over_budget",
        }
    }
}

/// What a restore did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRestoreOutcome {
    Restored,
    /// Nothing was written: the document would have grown past its budget.
    OverBudget(OverBudget),
    /// The document holds this key again; the user edits or removes it first.
    KeyInUse,
    /// The archived row was restored before.
    AlreadyRestored,
    NotFound,
}

/// The documents one conversation's turns read: the shared preferences, its own preferences
/// (none for the global conversation, whose own are the shared ones) and its own learnings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConversationMemory {
    pub shared: Vec<MemoryEntry>,
    pub local: Vec<MemoryEntry>,
    pub learnings: Vec<MemoryEntry>,
}

fn invalid(detail: &str) -> StoreError {
    StoreError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        detail.to_owned(),
    ))
}

fn ms(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn db_ms(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

type EntryRow = (String, String, String, i64, i64);
/// `id, key, text, written_by, archived_at_ms, reason, restored_at_ms` of an archived row.
type ArchiveRow = (i64, String, String, String, i64, String, Option<i64>);
/// `kind, scope, key, text, written_by, created_at_ms, restored_at_ms` of an archived row.
type RestoreRow = (String, String, String, String, String, i64, Option<i64>);

async fn read_document(
    connection: &mut SqliteConnection,
    kind: MemoryKind,
    scope: &str,
) -> Result<Vec<MemoryEntry>, StoreError> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "SELECT key, text, written_by, created_at_ms, confirmed_at_ms FROM memory_entries \
         WHERE kind = ? AND scope = ? ORDER BY confirmed_at_ms DESC, key",
    )
    .bind(kind.as_str())
    .bind(scope)
    .fetch_all(connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(key, text, written_by, created_at_ms, confirmed_at_ms)| MemoryEntry {
                key,
                text,
                written_by: MemoryWriter::parse(&written_by),
                created_at_ms: ms(created_at_ms),
                confirmed_at_ms: ms(confirmed_at_ms),
            },
        )
        .collect())
}

/// Copies one live entry to the archive.
async fn archive_entry(
    connection: &mut SqliteConnection,
    kind: MemoryKind,
    scope: &str,
    entry: &MemoryEntry,
    reason: &str,
    by: MemoryWriter,
    now_ms: u64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO memory_archive (kind, scope, key, text, written_by, created_at_ms, \
         confirmed_at_ms, archived_at_ms, reason, archived_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(kind.as_str())
    .bind(scope)
    .bind(&entry.key)
    .bind(&entry.text)
    .bind(entry.written_by.as_str())
    .bind(db_ms(entry.created_at_ms))
    .bind(db_ms(entry.confirmed_at_ms))
    .bind(db_ms(now_ms))
    .bind(reason)
    .bind(by.as_str())
    .execute(connection)
    .await?;
    Ok(())
}

impl SqliteStore {
    /// One document's entries, most recently confirmed first.
    pub fn memory_entries(
        &self,
        kind: MemoryKind,
        scope: &MemoryScope,
    ) -> Result<Vec<MemoryEntry>, StoreError> {
        let scope = scope.as_string();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_document(&mut connection, kind, &scope).await
        })
    }

    /// Writes one entry in place: a new key is added, a known key gets the new text (the old
    /// one is archived as `replaced`), and the same text only moves its last-confirmed time.
    /// A write that would grow the document past its budget writes nothing and reports the
    /// size it would have had.
    pub fn set_memory_entry(
        &self,
        kind: MemoryKind,
        scope: &MemoryScope,
        key: &str,
        text: &str,
        by: MemoryWriter,
        now_ms: u64,
    ) -> Result<MemoryWriteOutcome, StoreError> {
        if let Err(refusal) = check_entry(key, text) {
            return Err(invalid(refusal.code()));
        }
        let limit = budget(kind, scope);
        let scope = scope.as_string();
        let (key, text) = (key.to_owned(), text.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let document = read_document(&mut transaction, kind, &scope).await?;
            let current: usize = document.iter().map(MemoryEntry::bytes).sum();
            let old = document.iter().find(|entry| entry.key == key);
            if let Err(over) = check_budget(
                current,
                old.map_or(0, MemoryEntry::bytes),
                entry_bytes(&key, &text),
                limit,
            ) {
                return Ok(MemoryWriteOutcome::OverBudget(over));
            }
            let outcome = match old {
                Some(old) if old.text == text => {
                    sqlx::query(
                        "UPDATE memory_entries SET confirmed_at_ms = ? \
                         WHERE kind = ? AND scope = ? AND key = ?",
                    )
                    .bind(db_ms(now_ms))
                    .bind(kind.as_str())
                    .bind(&scope)
                    .bind(&key)
                    .execute(&mut *transaction)
                    .await?;
                    MemoryWriteOutcome::Confirmed
                }
                Some(old) => {
                    archive_entry(&mut transaction, kind, &scope, old, "replaced", by, now_ms)
                        .await?;
                    sqlx::query(
                        "UPDATE memory_entries SET text = ?, written_by = ?, confirmed_at_ms = ? \
                         WHERE kind = ? AND scope = ? AND key = ?",
                    )
                    .bind(&text)
                    .bind(by.as_str())
                    .bind(db_ms(now_ms))
                    .bind(kind.as_str())
                    .bind(&scope)
                    .bind(&key)
                    .execute(&mut *transaction)
                    .await?;
                    MemoryWriteOutcome::Updated
                }
                None => {
                    sqlx::query(
                        "INSERT INTO memory_entries \
                         (kind, scope, key, text, written_by, created_at_ms, confirmed_at_ms) \
                         VALUES (?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(kind.as_str())
                    .bind(&scope)
                    .bind(&key)
                    .bind(&text)
                    .bind(by.as_str())
                    .bind(db_ms(now_ms))
                    .bind(db_ms(now_ms))
                    .execute(&mut *transaction)
                    .await?;
                    MemoryWriteOutcome::Created
                }
            };
            transaction.commit().await?;
            Ok(outcome)
        })
    }

    /// Moves one entry to the archive (reason `removed`). False when the document has no such
    /// key.
    pub fn remove_memory_entry(
        &self,
        kind: MemoryKind,
        scope: &MemoryScope,
        key: &str,
        by: MemoryWriter,
        now_ms: u64,
    ) -> Result<bool, StoreError> {
        let scope = scope.as_string();
        let key = key.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let document = read_document(&mut transaction, kind, &scope).await?;
            let Some(entry) = document.iter().find(|entry| entry.key == key) else {
                return Ok(false);
            };
            archive_entry(&mut transaction, kind, &scope, entry, "removed", by, now_ms).await?;
            sqlx::query("DELETE FROM memory_entries WHERE kind = ? AND scope = ? AND key = ?")
                .bind(kind.as_str())
                .bind(&scope)
                .bind(&key)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            Ok(true)
        })
    }

    /// One document's archived entries, newest first, at most [`MEMORY_ARCHIVE_LISTED`].
    pub fn memory_archive(
        &self,
        kind: MemoryKind,
        scope: &MemoryScope,
    ) -> Result<Vec<ArchivedMemoryEntry>, StoreError> {
        let scope = scope.as_string();
        self.call(move |pool| async move {
            let rows: Vec<ArchiveRow> = sqlx::query_as(
                "SELECT id, key, text, written_by, archived_at_ms, reason, restored_at_ms \
                     FROM memory_archive WHERE kind = ? AND scope = ? ORDER BY id DESC LIMIT ?",
            )
            .bind(kind.as_str())
            .bind(&scope)
            .bind(i64::try_from(MEMORY_ARCHIVE_LISTED).unwrap_or(i64::MAX))
            .fetch_all(&pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(
                    |(id, key, text, written_by, archived_at_ms, reason, restored)| {
                        ArchivedMemoryEntry {
                            id,
                            key,
                            text,
                            written_by: MemoryWriter::parse(&written_by),
                            archived_at_ms: ms(archived_at_ms),
                            reason,
                            restored: restored.is_some(),
                        }
                    },
                )
                .collect())
        })
    }

    /// Puts an archived entry back in its document, as last confirmed now, and records the
    /// restore on the archived row. The kind and scope of the row are returned with the
    /// outcome, so the caller can name the document.
    pub fn restore_memory_entry(
        &self,
        archive_id: i64,
        now_ms: u64,
    ) -> Result<(MemoryRestoreOutcome, Option<(MemoryKind, MemoryScope)>), StoreError> {
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let row: Option<RestoreRow> = sqlx::query_as(
                "SELECT kind, scope, key, text, written_by, created_at_ms, restored_at_ms \
                     FROM memory_archive WHERE id = ?",
            )
            .bind(archive_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some((kind, scope, key, text, written_by, created_at_ms, restored)) = row else {
                return Ok((MemoryRestoreOutcome::NotFound, None));
            };
            let (Some(kind), Some(scope_value)) =
                (MemoryKind::parse(&kind), MemoryScope::parse(&scope))
            else {
                return Ok((MemoryRestoreOutcome::NotFound, None));
            };
            let document = Some((kind, scope_value.clone()));
            if restored.is_some() {
                return Ok((MemoryRestoreOutcome::AlreadyRestored, document));
            }
            // An archived text that the rules refuse now stays archived.
            if let Err(refusal) = check_entry(&key, &text) {
                return Err(invalid(refusal.code()));
            }
            let entries = read_document(&mut transaction, kind, &scope).await?;
            if entries.iter().any(|entry| entry.key == key) {
                return Ok((MemoryRestoreOutcome::KeyInUse, document));
            }
            let current: usize = entries.iter().map(MemoryEntry::bytes).sum();
            if let Err(over) = check_budget(
                current,
                0,
                entry_bytes(&key, &text),
                budget(kind, &scope_value),
            ) {
                return Ok((MemoryRestoreOutcome::OverBudget(over), document));
            }
            sqlx::query(
                "INSERT INTO memory_entries \
                 (kind, scope, key, text, written_by, created_at_ms, confirmed_at_ms) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(kind.as_str())
            .bind(&scope)
            .bind(&key)
            .bind(&text)
            .bind(&written_by)
            .bind(created_at_ms)
            .bind(db_ms(now_ms))
            .execute(&mut *transaction)
            .await?;
            sqlx::query("UPDATE memory_archive SET restored_at_ms = ? WHERE id = ?")
                .bind(db_ms(now_ms))
                .bind(archive_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            Ok((MemoryRestoreOutcome::Restored, document))
        })
    }

    /// The documents a conversation's turns read, in one read transaction.
    pub fn conversation_memory(
        &self,
        target: &ConversationTarget,
    ) -> Result<ConversationMemory, StoreError> {
        let own = target.memory_scope();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            let shared = read_document(&mut connection, MemoryKind::Preference, "global").await?;
            let local = match own {
                MemoryScope::Global => Vec::new(),
                ref scope => {
                    read_document(&mut connection, MemoryKind::Preference, &scope.as_string())
                        .await?
                }
            };
            let learnings =
                read_document(&mut connection, MemoryKind::Learning, &own.as_string()).await?;
            Ok(ConversationMemory {
                shared,
                local,
                learnings,
            })
        })
    }
}
