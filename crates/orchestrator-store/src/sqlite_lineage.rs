//! Task lineage (DPES-1105 phase 4d, migration 0045).
//!
//! A revise (or a promotion) links the new task to the task it came from. A task has at most
//! one parent, the link is written once and never changes, and the parent task is never
//! deleted. The store refuses a link that would make a task its own ancestor.
//!
//! Writing the same link again is a no-op that returns the stored row, so an execution that
//! reruns after a crash links once. A different link for a task that already has one is
//! refused.

use std::fmt;
use std::io;

use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

const MAX_INTENT_CHARS: usize = 2048;
/// More parent steps than this means the table was changed outside the store.
const MAX_DEPTH: usize = 10_000;
const COLUMNS: &str = "task_id, parent_task_id, relation, added_intent, created_at";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineageRelation {
    RevisedFrom,
    PromotedFrom,
}

impl LineageRelation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RevisedFrom => "revised_from",
            Self::PromotedFrom => "promoted_from",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "revised_from" => Some(Self::RevisedFrom),
            "promoted_from" => Some(Self::PromotedFrom),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskLineage {
    pub task_id: String,
    pub parent_task_id: String,
    pub relation: LineageRelation,
    pub added_intent: Option<String>,
    pub created_at: String,
}

/// Why the store refused a link. Carried inside a `StoreError::Io` of kind `InvalidInput`;
/// [`LineageError::of`] reads it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineageError {
    /// A task cannot be its own parent.
    SelfParent,
    /// The parent is already a descendant of the task.
    Cycle,
    /// The task already has a different parent, relation or intent.
    AlreadyLinked,
    /// The added intent is longer than 2048 characters.
    IntentTooLong,
}

impl fmt::Display for LineageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SelfParent => "a task cannot be its own parent",
            Self::Cycle => "the link would make a task its own ancestor",
            Self::AlreadyLinked => "the task already has a different lineage",
            Self::IntentTooLong => "the added intent must be at most 2048 characters",
        })
    }
}

impl std::error::Error for LineageError {}

impl From<LineageError> for StoreError {
    fn from(error: LineageError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl LineageError {
    /// The refusal inside `error`, if it is one.
    pub fn of(error: &StoreError) -> Option<Self> {
        match error {
            StoreError::Io(cause) if cause.kind() == io::ErrorKind::InvalidInput => cause
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<Self>())
                .copied(),
            _ => None,
        }
    }
}

impl SqliteStore {
    /// Links `task_id` to `parent_task_id`, in one transaction. Both tasks must exist
    /// (`RecordNotFound` otherwise). The same link again returns the stored row and writes
    /// nothing. Returns the stored row.
    pub fn add_task_lineage(
        &self,
        task_id: &str,
        parent_task_id: &str,
        relation: LineageRelation,
        added_intent: Option<&str>,
    ) -> Result<TaskLineage, StoreError> {
        if task_id == parent_task_id {
            return Err(LineageError::SelfParent.into());
        }
        if added_intent.is_some_and(|intent| intent.chars().count() > MAX_INTENT_CHARS) {
            return Err(LineageError::IntentTooLong.into());
        }
        let task_id = task_id.to_owned();
        let parent_task_id = parent_task_id.to_owned();
        let added_intent = added_intent.map(str::to_owned);
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            for id in [&task_id, &parent_task_id] {
                let found = sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut *transaction)
                    .await?;
                if found.is_none() {
                    return Err(StoreError::RecordNotFound {
                        entity: "task",
                        id: id.clone(),
                    });
                }
            }
            if let Some(existing) = read_lineage(&mut transaction, &task_id).await? {
                let same = existing.parent_task_id == parent_task_id
                    && existing.relation == relation
                    && existing.added_intent == added_intent;
                return if same {
                    Ok(existing)
                } else {
                    Err(StoreError::from(LineageError::AlreadyLinked))
                };
            }
            if is_ancestor(&mut transaction, &task_id, &parent_task_id).await? {
                return Err(StoreError::from(LineageError::Cycle));
            }
            sqlx::query(
                "INSERT INTO task_lineage (task_id, parent_task_id, relation, added_intent) \
                 VALUES (?, ?, ?, ?) ON CONFLICT(task_id) DO NOTHING",
            )
            .bind(&task_id)
            .bind(&parent_task_id)
            .bind(relation.as_str())
            .bind(&added_intent)
            .execute(&mut *transaction)
            .await?;
            let saved = read_lineage(&mut transaction, &task_id)
                .await?
                .ok_or_else(|| corrupt("the task lineage just written is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// The link of a task to its parent, if it has one.
    pub fn task_lineage(&self, task_id: &str) -> Result<Option<TaskLineage>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_lineage(&mut connection, &task_id).await
        })
    }

    /// The links whose parent is `parent_task_id`, in the order they were written.
    pub fn task_lineage_children(
        &self,
        parent_task_id: &str,
    ) -> Result<Vec<TaskLineage>, StoreError> {
        let parent_task_id = parent_task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `COLUMNS` is a fixed list of column names; the id is bound.
                "SELECT {COLUMNS} FROM task_lineage WHERE parent_task_id = ? ORDER BY rowid"
            )))
            .bind(&parent_task_id)
            .fetch_all(&pool)
            .await?;
            rows.iter().map(lineage_from_row).collect()
        })
    }
}

/// Whether `task_id` is `candidate` or one of its ancestors.
async fn is_ancestor(
    connection: &mut SqliteConnection,
    task_id: &str,
    candidate: &str,
) -> Result<bool, StoreError> {
    let mut current = candidate.to_owned();
    for _ in 0..MAX_DEPTH {
        if current == task_id {
            return Ok(true);
        }
        match read_lineage(connection, &current).await? {
            Some(link) => current = link.parent_task_id,
            None => return Ok(false),
        }
    }
    Err(corrupt("the task lineage chain does not end".to_owned()))
}

async fn read_lineage(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<TaskLineage>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `COLUMNS` is a fixed list of column names; the id is bound.
        "SELECT {COLUMNS} FROM task_lineage WHERE task_id = ?"
    )))
    .bind(task_id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(lineage_from_row).transpose()
}

fn lineage_from_row(row: &SqliteRow) -> Result<TaskLineage, StoreError> {
    let relation: String = row.try_get("relation")?;
    Ok(TaskLineage {
        task_id: row.try_get("task_id")?,
        parent_task_id: row.try_get("parent_task_id")?,
        relation: LineageRelation::parse(&relation)
            .ok_or_else(|| corrupt(format!("unknown lineage relation {relation:?}")))?,
        added_intent: row.try_get("added_intent")?,
        created_at: row.try_get("created_at")?,
    })
}
