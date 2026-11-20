//! Per-project defaults (DPES-1105 phase 4b, migration 0043).
//!
//! A project has a row only when the user sets a default. A project with no row reads as
//! [`ProjectDefaultsRow::local`], and a read never writes. The mode is stored as its wire name
//! (`local-only`, `direct-pr`, `no-mistakes`); the typed mode lives in core, and the daemon
//! converts. The database bounds each text column and refuses control characters; the store
//! checks the same rules first, so bad input is a [`ProjectDefaultsError`] and never a raw
//! constraint message. The Git ref rules for a branch name are the daemon's
//! (`is_safe_branch_name`), not repeated here.
//!
//! A row is replaced, never deleted: the delete gate in the migration refuses it.
//! [`SqliteStore::reset_project_defaults`] writes the defaults back instead.

use std::fmt;
use std::io;

use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

const MODES: [&str; 3] = ["local-only", "direct-pr", "no-mistakes"];
const MERGE_METHODS: [&str; 3] = ["merge", "squash", "rebase"];
const MAX_CHECKS: usize = 8;
const MAX_ARGS: usize = 16;
const MAX_ARG_BYTES: usize = 256;
const MAX_CHECKS_JSON_BYTES: usize = 4096;

const COLUMNS: &str =
    "project_id, mode, base_branch, branch_prefix, checks_json, merge_method, updated_at";

/// One project's defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectDefaultsRow {
    pub project_id: String,
    /// `local-only`, `direct-pr` or `no-mistakes`.
    pub mode: String,
    /// `None` means HEAD of the primary checkout.
    pub base_branch: Option<String>,
    /// `None` means the machine-wide prefix.
    pub branch_prefix: Option<String>,
    /// Argument arrays; empty means no checks.
    pub checks: Vec<Vec<String>>,
    /// `merge`, `squash` or `rebase`.
    pub merge_method: String,
    /// Empty for a project with no row.
    pub updated_at: String,
}

impl ProjectDefaultsRow {
    /// What a project with no row reads as.
    pub fn local(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_owned(),
            mode: "local-only".to_owned(),
            base_branch: None,
            branch_prefix: None,
            checks: Vec::new(),
            merge_method: "squash".to_owned(),
            updated_at: String::new(),
        }
    }
}

/// A change to one project's defaults. A field that is not named stays as it is. For the two
/// branch fields, `Some(None)` clears the value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectDefaultsPatch {
    pub mode: Option<String>,
    pub base_branch: Option<Option<String>>,
    pub branch_prefix: Option<Option<String>>,
    pub checks: Option<Vec<Vec<String>>>,
    pub merge_method: Option<String>,
}

/// Why the store refused a default. Carried inside a `StoreError::Io` of kind `InvalidInput`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectDefaultsError {
    UnknownMode,
    UnknownMergeMethod,
    BaseBranch,
    BranchPrefix,
    Checks,
}

impl fmt::Display for ProjectDefaultsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownMode => "the project mode must be local-only, direct-pr or no-mistakes",
            Self::UnknownMergeMethod => "the merge method must be merge, squash or rebase",
            Self::BaseBranch => {
                "the base branch must be 1 to 100 characters with no control character"
            }
            Self::BranchPrefix => {
                "the branch prefix must be 2 to 64 characters with no control character"
            }
            Self::Checks => {
                "checks must be at most 8 commands of 1 to 16 arguments, \
                 each 1 to 256 bytes with no NUL, and at most 4096 bytes in all"
            }
        })
    }
}

impl std::error::Error for ProjectDefaultsError {}

impl From<ProjectDefaultsError> for StoreError {
    fn from(error: ProjectDefaultsError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

fn valid_text(value: &str, lengths: std::ops::RangeInclusive<usize>) -> bool {
    lengths.contains(&value.chars().count()) && !value.chars().any(char::is_control)
}

fn validate_checks(checks: &[Vec<String>]) -> Result<(), ProjectDefaultsError> {
    let shaped = checks.len() <= MAX_CHECKS
        && checks.iter().all(|command| {
            (1..=MAX_ARGS).contains(&command.len())
                && command
                    .iter()
                    .all(|arg| (1..=MAX_ARG_BYTES).contains(&arg.len()) && !arg.contains('\0'))
        });
    match checks_json(checks) {
        Some(json) if shaped && json.len() <= MAX_CHECKS_JSON_BYTES => Ok(()),
        _ => Err(ProjectDefaultsError::Checks),
    }
}

fn checks_json(checks: &[Vec<String>]) -> Option<String> {
    serde_json::to_string(checks).ok()
}

fn validate(row: &ProjectDefaultsRow) -> Result<(), ProjectDefaultsError> {
    if !MODES.contains(&row.mode.as_str()) {
        return Err(ProjectDefaultsError::UnknownMode);
    }
    if !MERGE_METHODS.contains(&row.merge_method.as_str()) {
        return Err(ProjectDefaultsError::UnknownMergeMethod);
    }
    if matches!(&row.base_branch, Some(value) if !valid_text(value, 1..=100)) {
        return Err(ProjectDefaultsError::BaseBranch);
    }
    if matches!(&row.branch_prefix, Some(value) if !valid_text(value, 2..=64)) {
        return Err(ProjectDefaultsError::BranchPrefix);
    }
    validate_checks(&row.checks)
}

impl SqliteStore {
    /// The stored defaults of a project, or `None` when the user has set none. Writes nothing.
    pub fn project_defaults(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectDefaultsRow>, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_row(&mut connection, &project_id).await
        })
    }

    /// The defaults a project runs with: its row, or [`ProjectDefaultsRow::local`]. Writes
    /// nothing.
    pub fn project_defaults_or_local(
        &self,
        project_id: &str,
    ) -> Result<ProjectDefaultsRow, StoreError> {
        Ok(self
            .project_defaults(project_id)?
            .unwrap_or_else(|| ProjectDefaultsRow::local(project_id)))
    }

    /// Every stored row in project identifier order.
    pub fn list_project_defaults(&self) -> Result<Vec<ProjectDefaultsRow>, StoreError> {
        self.call(|pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe to assert: a constant list of column names.
                "SELECT {COLUMNS} FROM project_defaults ORDER BY project_id"
            )))
            .fetch_all(&pool)
            .await?;
            rows.iter().map(decode_row).collect()
        })
    }

    /// Writes the whole row, creating it or replacing it, and bumps `updated_at`. A project
    /// that is not registered fails with `RecordNotFound`; a bad value fails with an
    /// `InvalidInput` I/O error carrying a [`ProjectDefaultsError`], before anything is written.
    pub fn set_project_defaults(&self, row: &ProjectDefaultsRow) -> Result<(), StoreError> {
        validate(row)?;
        let checks = checks_json(&row.checks).ok_or(ProjectDefaultsError::Checks)?;
        let row = row.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_project(&mut transaction, &row.project_id).await?;
            sqlx::query(
                "INSERT INTO project_defaults \
                 (project_id, mode, base_branch, branch_prefix, checks_json, merge_method) \
                 VALUES (?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(project_id) DO UPDATE SET mode = excluded.mode, \
                 base_branch = excluded.base_branch, branch_prefix = excluded.branch_prefix, \
                 checks_json = excluded.checks_json, merge_method = excluded.merge_method, \
                 updated_at = CURRENT_TIMESTAMP",
            )
            .bind(&row.project_id)
            .bind(&row.mode)
            .bind(&row.base_branch)
            .bind(&row.branch_prefix)
            .bind(checks)
            .bind(&row.merge_method)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Changes the defaults `patch` names, in one transaction: merges the patch into the
    /// stored row (or the local defaults when there is none), checks the merged row, creates the
    /// row if it is missing, then sets only the named columns and bumps `updated_at`. An empty
    /// patch writes nothing. Returns the row as stored, or the local defaults for an empty
    /// patch on a project with no row.
    pub fn update_project_defaults(
        &self,
        project_id: &str,
        patch: &ProjectDefaultsPatch,
    ) -> Result<ProjectDefaultsRow, StoreError> {
        if *patch == ProjectDefaultsPatch::default() {
            return self.project_defaults_or_local(project_id);
        }
        let project_id = project_id.to_owned();
        let patch = patch.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_project(&mut transaction, &project_id).await?;
            let mut merged = read_row(&mut transaction, &project_id)
                .await?
                .unwrap_or_else(|| ProjectDefaultsRow::local(&project_id));
            let mut named: Vec<(&'static str, Option<String>)> = Vec::new();
            if let Some(mode) = patch.mode {
                merged.mode.clone_from(&mode);
                named.push(("mode", Some(mode)));
            }
            if let Some(base_branch) = patch.base_branch {
                merged.base_branch.clone_from(&base_branch);
                named.push(("base_branch", base_branch));
            }
            if let Some(branch_prefix) = patch.branch_prefix {
                merged.branch_prefix.clone_from(&branch_prefix);
                named.push(("branch_prefix", branch_prefix));
            }
            if let Some(checks) = patch.checks {
                let json = checks_json(&checks).ok_or(ProjectDefaultsError::Checks)?;
                merged.checks = checks;
                named.push(("checks_json", Some(json)));
            }
            if let Some(method) = patch.merge_method {
                merged.merge_method.clone_from(&method);
                named.push(("merge_method", Some(method)));
            }
            validate(&merged)?;

            // The row exists with its defaults before the named columns are set.
            sqlx::query(
                "INSERT INTO project_defaults (project_id) VALUES (?) ON CONFLICT DO NOTHING",
            )
            .bind(&project_id)
            .execute(&mut *transaction)
            .await?;
            let assignments = named
                .iter()
                .map(|(column, _)| format!("{column} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "UPDATE project_defaults SET {assignments}, updated_at = CURRENT_TIMESTAMP \
                 WHERE project_id = ?"
            );
            // Safe to assert: `assignments` is built only from the fixed column names above, and
            // every value is bound.
            let mut update = sqlx::query(sqlx::AssertSqlSafe(sql));
            for (_, value) in &named {
                update = update.bind(value.clone());
            }
            update.bind(&project_id).execute(&mut *transaction).await?;
            let saved = read_row(&mut transaction, &project_id)
                .await?
                .ok_or_else(|| corrupt("the project defaults row is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Puts a project's defaults back to the local defaults. The row stays, because a row is
    /// never deleted. A project with no row is left with none.
    pub fn reset_project_defaults(&self, project_id: &str) -> Result<(), StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            sqlx::query(
                "UPDATE project_defaults SET mode = 'local-only', base_branch = NULL, \
                 branch_prefix = NULL, checks_json = '[]', merge_method = 'squash', \
                 updated_at = CURRENT_TIMESTAMP WHERE project_id = ?",
            )
            .bind(&project_id)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }
}

async fn require_project(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<(), StoreError> {
    let found = sqlx::query_scalar::<_, i64>("SELECT 1 FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(connection)
        .await?;
    match found {
        Some(_) => Ok(()),
        None => Err(StoreError::RecordNotFound {
            entity: "project",
            id: project_id.to_owned(),
        }),
    }
}

async fn read_row(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<Option<ProjectDefaultsRow>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe to assert: a constant list of column names.
        "SELECT {COLUMNS} FROM project_defaults WHERE project_id = ?"
    )))
    .bind(project_id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(decode_row).transpose()
}

fn decode_row(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectDefaultsRow, StoreError> {
    let checks_text: String = row.try_get("checks_json")?;
    let checks = serde_json::from_str(&checks_text)
        .map_err(|_| corrupt("the stored project checks are not valid".to_owned()))?;
    Ok(ProjectDefaultsRow {
        project_id: row.try_get("project_id")?,
        mode: row.try_get("mode")?,
        base_branch: row.try_get("base_branch")?,
        branch_prefix: row.try_get("branch_prefix")?,
        checks,
        merge_method: row.try_get("merge_method")?,
        updated_at: crate::instant::instant(&row, "updated_at")?,
    })
}
