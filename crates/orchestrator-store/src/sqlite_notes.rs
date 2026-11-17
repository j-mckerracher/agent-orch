//! The user's orchestrator notes (DPES-1105 phase 5b, migration 0050).
//!
//! A note is one short line of user-written text that the conversation snapshot lists as
//! data. Only `notes.set` (user-only) writes them, replacing a scope's whole list in one
//! transaction. The store checks the core rules again, so a caller that skipped them still
//! cannot plant a refused note, and a refusal is a stable code, never the note text. A history
//! clear deletes the cleared conversation's notes in its own transaction
//! ([`delete_scope_notes`]).

use std::io;

use orchestrator_core::notes::{check_notes, MAX_NOTES_PER_SCOPE};
use sqlx::SqliteConnection;

use crate::{ConversationTarget, SqliteStore, StoreError};

/// The scope of the notes every conversation lists.
const GLOBAL_SCOPE: &str = "global";
const WORKSTREAM_PREFIX: &str = "workstream:";

fn invalid(detail: &str) -> StoreError {
    StoreError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        detail.to_owned(),
    ))
}

fn scope_is_valid(scope: &str) -> bool {
    scope == GLOBAL_SCOPE
        || scope
            .strip_prefix(WORKSTREAM_PREFIX)
            .is_some_and(|id| !id.is_empty())
}

/// Deletes one scope's notes inside the caller's transaction. Returns the rows deleted.
pub(crate) async fn delete_scope_notes(
    connection: &mut SqliteConnection,
    scope: &str,
) -> Result<u64, StoreError> {
    let deleted = sqlx::query("DELETE FROM orchestrator_notes WHERE scope = ?")
        .bind(scope)
        .execute(connection)
        .await?;
    Ok(deleted.rows_affected())
}

async fn read_scope(
    connection: &mut SqliteConnection,
    scope: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT body FROM orchestrator_notes WHERE scope = ? ORDER BY position")
            .bind(scope)
            .fetch_all(connection)
            .await?,
    )
}

impl SqliteStore {
    /// One scope's notes in position order. `scope` is `global` or `workstream:` plus an id.
    pub fn orchestrator_notes(&self, scope: &str) -> Result<Vec<String>, StoreError> {
        if !scope_is_valid(scope) {
            return Err(invalid("the notes scope is not valid"));
        }
        let scope = scope.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_scope(&mut connection, &scope).await
        })
    }

    /// Replaces one scope's whole list in one `BEGIN IMMEDIATE` transaction, so a refused or
    /// failed write leaves the old list. The list is checked with the core rules first.
    pub fn set_orchestrator_notes(&self, scope: &str, notes: &[String]) -> Result<(), StoreError> {
        if !scope_is_valid(scope) {
            return Err(invalid("the notes scope is not valid"));
        }
        if let Err(refusal) = check_notes(notes) {
            return Err(invalid(refusal.code()));
        }
        let scope = scope.to_owned();
        let notes = notes.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            delete_scope_notes(&mut transaction, &scope).await?;
            for (position, body) in notes.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO orchestrator_notes (scope, position, body) VALUES (?, ?, ?)",
                )
                .bind(&scope)
                .bind(i64::try_from(position).unwrap_or(i64::MAX))
                .bind(body)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// The notes a conversation's snapshot lists: a workstream's own first, then the global
    /// ones, at most [`MAX_NOTES_PER_SCOPE`] lines in all. One read transaction.
    pub fn conversation_notes(
        &self,
        target: &ConversationTarget,
    ) -> Result<Vec<String>, StoreError> {
        let own = target.notes_scope();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            let mut notes = read_scope(&mut connection, &own).await?;
            if own != GLOBAL_SCOPE {
                notes.extend(read_scope(&mut connection, GLOBAL_SCOPE).await?);
            }
            notes.truncate(MAX_NOTES_PER_SCOPE);
            Ok(notes)
        })
    }
}
