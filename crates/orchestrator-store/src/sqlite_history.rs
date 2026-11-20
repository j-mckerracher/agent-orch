//! Clearing and removing conversation history (DPES-1105).
//!
//! The append-only rule of migration 0031 has one exception, and the clear
//! transactions here are its only users. A clear deletes rows of
//! `conversation_entries`, `conversation_turns` or `coordinator_entries` inside one
//! `BEGIN IMMEDIATE` transaction. Migration 0032 aborts every other delete, because
//! the delete triggers check the clear gate, and only these transactions open it.
//! Each clear also drops the idempotency fingerprints of the sends it deleted, and
//! writes one `conversation_audit` row that holds no content.

use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::{ConversationError, ConversationTarget, SqliteStore, StoreError};

/// Fingerprint kept for a send whose message was cleared. The message is gone, and
/// a hash of a short message could be checked against a guess, so the hash goes too.
/// The idempotency layer treats this value as matching any request, because the
/// request text it would compare against no longer exists.
pub(crate) const CLEARED_FINGERPRINT: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// What a clear of one conversation or coordinator log did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearOutcome {
    /// False when nothing was there to delete. Then nothing changed: no generation
    /// bump and no audit row.
    pub cleared: bool,
    pub entries_deleted: u64,
    pub turns_deleted: u64,
    /// The scope's generation after the call. A reader holding an older value
    /// discards what it read, because the log it holds may include cleared messages.
    pub clear_generation: i64,
}

/// What a remove or restore of a project coordinator did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorChange {
    /// False when the coordinator was already in the requested state.
    pub changed: bool,
    pub removed: bool,
    pub entries_deleted: u64,
    pub clear_generation: i64,
}

/// A project coordinator's state. Coordinators are opt-in (migration 0053): a project
/// has one only after `restore_coordinator` adds it, so a project with no row has no
/// coordinator and generation 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorScope {
    pub removed: bool,
    pub clear_generation: i64,
}

/// One row of the history audit: what happened, to which scope, and how many rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub sequence: i64,
    pub action: String,
    /// `conversation:global`, `conversation:workstream:<id>` or `coordinator:<project_id>`.
    pub scope: String,
    /// Always absent today: the daemon has no caller identity to record.
    pub actor: Option<String>,
    pub entries_deleted: i64,
    pub turns_deleted: i64,
    pub created_at: String,
}

impl SqliteStore {
    /// Clear one project coordinator's log. The project and its tasks are untouched.
    ///
    /// Fails with `RecordNotFound` for an unknown project or a removed coordinator.
    pub fn clear_coordinator_history(
        &self,
        project_id: &str,
    ) -> Result<ClearOutcome, ConversationError> {
        self.clear_conversation_history(&ConversationTarget::Project(project_id.to_owned()))
    }

    /// Remove a project's coordinator: delete its log permanently and mark it removed.
    /// Idempotent: a coordinator that is already removed is left as it is.
    pub fn remove_coordinator(
        &self,
        project_id: &str,
    ) -> Result<CoordinatorChange, ConversationError> {
        let outcome = self.clear_conversation_history_as(
            &ConversationTarget::Project(project_id.to_owned()),
            true,
        )?;
        Ok(CoordinatorChange {
            changed: outcome.cleared,
            removed: true,
            entries_deleted: outcome.entries_deleted,
            clear_generation: outcome.clear_generation,
        })
    }

    /// Add a coordinator to a project that has none, either never added or removed.
    /// It starts empty, because removal deleted its log. Idempotent: an active
    /// coordinator is left as it is.
    pub fn restore_coordinator(&self, project_id: &str) -> Result<CoordinatorChange, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_project(&mut transaction, &project_id).await?;
            let clear_generation = coordinator_generation(&mut transaction, &project_id).await?;
            if !coordinator_removed(&mut transaction, &project_id).await? {
                transaction.commit().await?;
                return Ok(CoordinatorChange {
                    changed: false,
                    removed: false,
                    entries_deleted: 0,
                    clear_generation,
                });
            }
            sqlx::query(
                "INSERT INTO coordinator_scopes (project_id, removed) VALUES (?, 0) \
                 ON CONFLICT(project_id) DO UPDATE SET removed = 0, \
                 changed_at = CURRENT_TIMESTAMP",
            )
            .bind(&project_id)
            .execute(&mut *transaction)
            .await?;
            record_audit(
                &mut transaction,
                "restore",
                &coordinator_scope_id(&project_id),
                0,
                0,
            )
            .await?;
            transaction.commit().await?;
            Ok(CoordinatorChange {
                changed: true,
                removed: false,
                entries_deleted: 0,
                clear_generation,
            })
        })
        .and_then(|outcome| {
            self.checkpoint_wal()?;
            Ok(outcome)
        })
    }

    /// The coordinator state of one project. Unknown projects and projects whose
    /// coordinator was never added read as removed with generation 0.
    pub fn coordinator_scope(&self, project_id: &str) -> Result<CoordinatorScope, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let row = sqlx::query(
                "SELECT removed, clear_generation FROM coordinator_scopes WHERE project_id = ?",
            )
            .bind(&project_id)
            .fetch_optional(&pool)
            .await?;
            Ok(match row {
                Some(row) => CoordinatorScope {
                    removed: row.try_get::<i64, _>("removed")? == 1,
                    clear_generation: row.try_get("clear_generation")?,
                },
                None => CoordinatorScope {
                    removed: true,
                    clear_generation: 0,
                },
            })
        })
    }

    /// Ids of registered projects that have no active coordinator.
    pub fn projects_without_coordinator(&self) -> Result<Vec<String>, StoreError> {
        self.call(|pool| async move {
            Ok(sqlx::query_scalar(
                "SELECT id FROM projects WHERE id NOT IN \
                 (SELECT project_id FROM coordinator_scopes WHERE removed = 0) ORDER BY id",
            )
            .fetch_all(&pool)
            .await?)
        })
    }

    /// Every audit row, oldest first.
    pub fn conversation_audit(&self) -> Result<Vec<AuditRecord>, StoreError> {
        self.call(|pool| async move {
            let rows = sqlx::query(
                "SELECT sequence, action, scope, actor, entries_deleted, turns_deleted, created_at \
                 FROM conversation_audit ORDER BY sequence",
            )
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(AuditRecord {
                        sequence: row.try_get("sequence")?,
                        action: row.try_get("action")?,
                        scope: row.try_get("scope")?,
                        actor: row.try_get("actor")?,
                        entries_deleted: row.try_get("entries_deleted")?,
                        turns_deleted: row.try_get("turns_deleted")?,
                        created_at: crate::instant::instant(&row, "created_at")?,
                    })
                })
                .collect()
        })
    }

    /// Truncate the write-ahead log into the database file. Once it is empty, the
    /// pages a clear rewrote hold no copy of the deleted text in any file of the data
    /// directory. A busy checkpoint is not an error: the old frames stay until the
    /// next checkpoint.
    pub(crate) fn checkpoint_wal(&self) -> Result<(), StoreError> {
        self.call(|pool| async move { checkpoint_wal(&pool).await.map_err(StoreError::from) })
    }
}

/// Runs a full checkpoint and truncates the log. Used after migrations and after clears.
pub(crate) async fn checkpoint_wal(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await
        .map(|_| ())
}

pub(crate) fn coordinator_scope_id(project_id: &str) -> String {
    format!("coordinator:{project_id}")
}

/// Opens or closes the clear gate. Only a clear transaction calls this with `true`,
/// and it always closes the gate again before it commits.
pub(crate) async fn set_clear_gate(
    connection: &mut SqliteConnection,
    open: bool,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE conversation_clear_gate SET open = ? WHERE id = 1")
        .bind(i64::from(open))
        .execute(connection)
        .await?;
    Ok(())
}

/// Appends one audit row. Callers pass counts only, never content.
pub(crate) async fn record_audit(
    connection: &mut SqliteConnection,
    action: &str,
    scope: &str,
    entries_deleted: u64,
    turns_deleted: u64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO conversation_audit (action, scope, entries_deleted, turns_deleted) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(action)
    .bind(scope)
    .bind(i64::try_from(entries_deleted).unwrap_or(i64::MAX))
    .bind(i64::try_from(turns_deleted).unwrap_or(i64::MAX))
    .execute(connection)
    .await?;
    Ok(())
}

/// Drops the fingerprints of the completed sends whose turns belong to a conversation.
/// Must run before the turns are deleted, because the match goes through them.
pub(crate) async fn tombstone_conversation_sends(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE idempotency_records SET fingerprint = ? \
         WHERE method = 'conversations.send' AND result IS NOT NULL \
           AND json_extract(result, '$.body.turn_id') IN \
               (SELECT id FROM conversation_turns WHERE conversation_id = ?)",
    )
    .bind(CLEARED_FINGERPRINT)
    .bind(conversation_id)
    .execute(connection)
    .await?;
    Ok(())
}

/// Drops the fingerprints of the completed coordinator messages and proposals of a
/// project. Must run before the entries are deleted.
pub(crate) async fn tombstone_coordinator_messages(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE idempotency_records SET fingerprint = ? \
         WHERE method IN ('coordinator.send_message', 'coordinator.propose') \
           AND result IS NOT NULL \
           AND json_extract(result, '$.body.entry_id') IN \
               (SELECT id FROM coordinator_entries WHERE project_id = ?)",
    )
    .bind(CLEARED_FINGERPRINT)
    .bind(project_id)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn require_project(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<(), StoreError> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(&mut *connection)
        .await?;
    found.map(|_| ()).ok_or_else(|| StoreError::RecordNotFound {
        entity: "project",
        id: project_id.to_owned(),
    })
}

pub(crate) async fn coordinator_removed(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<bool, StoreError> {
    let removed: Option<i64> =
        sqlx::query_scalar("SELECT removed FROM coordinator_scopes WHERE project_id = ?")
            .bind(project_id)
            .fetch_optional(&mut *connection)
            .await?;
    Ok(removed != Some(0))
}

pub(crate) async fn coordinator_generation(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<i64, StoreError> {
    let generation: Option<i64> =
        sqlx::query_scalar("SELECT clear_generation FROM coordinator_scopes WHERE project_id = ?")
            .bind(project_id)
            .fetch_optional(&mut *connection)
            .await?;
    Ok(generation.unwrap_or(0))
}

pub(crate) async fn count_coordinator_entries(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<u64, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM coordinator_entries WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&mut *connection)
            .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// Deletes a coordinator's messages and proposals with the gate open. Returns the count.
pub(crate) async fn wipe_coordinator(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<u64, StoreError> {
    set_clear_gate(&mut *connection, true).await?;
    tombstone_coordinator_messages(&mut *connection, project_id).await?;
    let deleted = sqlx::query("DELETE FROM coordinator_entries WHERE project_id = ?")
        .bind(project_id)
        .execute(&mut *connection)
        .await?
        .rows_affected();
    set_clear_gate(&mut *connection, false).await?;
    Ok(deleted)
}

/// Advances the coordinator's generation, creating its row if needed, and sets `removed`.
pub(crate) async fn bump_coordinator(
    connection: &mut SqliteConnection,
    project_id: &str,
    removed: bool,
) -> Result<i64, StoreError> {
    let generation: i64 = sqlx::query_scalar(
        "INSERT INTO coordinator_scopes (project_id, removed, clear_generation) VALUES (?, ?, 1) \
         ON CONFLICT(project_id) DO UPDATE SET removed = excluded.removed, \
             clear_generation = clear_generation + 1, changed_at = CURRENT_TIMESTAMP \
         RETURNING clear_generation",
    )
    .bind(project_id)
    .bind(i64::from(removed))
    .fetch_one(connection)
    .await?;
    Ok(generation)
}

#[cfg(test)]
mod tests {
    use super::CLEARED_FINGERPRINT;

    #[test]
    fn cleared_fingerprint_is_a_well_formed_fingerprint() {
        assert_eq!(CLEARED_FINGERPRINT.len(), 64);
        assert!(CLEARED_FINGERPRINT.bytes().all(|byte| byte == b'0'));
    }
}
