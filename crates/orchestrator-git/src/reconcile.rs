//! Read-only reconciliation of recorded task workspaces.
//!
//! Git observations return plans. This module never changes refs, removes
//! worktrees, or deletes paths. A workspace matches only while its recorded
//! branch is checked out and its ref still points to the recorded base commit.
//! CLI and scheduler wiring is deferred to M2-06.
//! Partial-delivery inspection is deferred to M3-06.
//!
//! Updated: 2026-11-08T07:32:49+00:00.

use std::path::{Path, PathBuf};

use crate::git::git;
use crate::{Error, Repository};

/// Durable workspace facts required to compare a path and checked-out branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedWorkspace {
    pub id: String,
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
}

/// Observed state of a recorded worktree path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceClassification {
    PresentAndMatching,
    Missing,
    BranchMoved,
    Foreign,
}

/// Classification of one recorded workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDecision {
    pub id: String,
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
    pub classification: WorkspaceClassification,
}

/// Full worktree observation, including paths not present in recorded data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePlan {
    /// Recorded workspace decisions in stable path and identifier order.
    pub workspaces: Vec<WorkspaceDecision>,
    /// Existing paths that are not valid recorded worktrees, including prunable paths.
    pub foreign_paths: Vec<PathBuf>,
    /// Non-primary Git worktrees absent from the recorded workspace inputs.
    pub unexpected_paths: Vec<PathBuf>,
}

struct WorktreeEntry {
    path: PathBuf,
    branch: Option<String>,
    prunable: bool,
}

/// Compares recorded paths and branches with Git's worktree list and refs.
pub fn reconcile_workspaces(
    repository: &Repository,
    recorded: &[RecordedWorkspace],
) -> Result<WorkspacePlan, Error> {
    let mut entries: Vec<_> = list_worktrees(repository)?
        .into_iter()
        .map(|entry| {
            let path = normalize_path(&entry.path)?;
            Ok::<_, Error>((entry, path))
        })
        .collect::<Result<_, _>>()?;
    entries.sort_by(|left, right| left.1.cmp(&right.1));

    let mut expected = recorded.to_vec();
    expected.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.branch.cmp(&right.branch))
            .then_with(|| left.base_commit.cmp(&right.base_commit))
    });
    let root = normalize_path(repository.path())?;
    let expected_paths: Vec<_> = expected
        .iter()
        .map(|workspace| normalize_path(&workspace.path))
        .collect::<Result<_, _>>()?;

    let mut decisions = Vec::with_capacity(expected.len());
    let mut foreign_paths = Vec::new();
    for (workspace, path) in expected.iter().zip(&expected_paths) {
        let entry = entries
            .iter()
            .find(|(_, entry_path)| entry_path == path)
            .map(|(entry, _)| entry);
        let classification = match entry {
            Some(_) if !workspace.path.exists() => WorkspaceClassification::Missing,
            Some(entry) if entry.prunable => {
                foreign_paths.push(path.clone());
                WorkspaceClassification::Foreign
            }
            Some(entry) => {
                match classify_branch(repository, entry, &workspace.branch, &workspace.base_commit)?
                {
                    true => WorkspaceClassification::PresentAndMatching,
                    false => WorkspaceClassification::BranchMoved,
                }
            }
            None if workspace.path.exists() => {
                foreign_paths.push(path.clone());
                WorkspaceClassification::Foreign
            }
            None => WorkspaceClassification::Missing,
        };
        decisions.push(WorkspaceDecision {
            id: workspace.id.clone(),
            path: path.clone(),
            branch: workspace.branch.clone(),
            base_commit: workspace.base_commit.clone(),
            classification,
        });
    }

    let mut unexpected_paths = Vec::new();
    for (entry, path) in &entries {
        if entry.prunable && entry.path.exists() {
            foreign_paths.push(path.clone());
        }
        if path != &root && !expected_paths.contains(path) {
            unexpected_paths.push(path.clone());
        }
    }
    foreign_paths.sort();
    foreign_paths.dedup();
    unexpected_paths.sort();
    unexpected_paths.dedup();
    Ok(WorkspacePlan {
        workspaces: decisions,
        foreign_paths,
        unexpected_paths,
    })
}

fn list_worktrees(repository: &Repository) -> Result<Vec<WorktreeEntry>, Error> {
    let listing = git(
        "list worktrees",
        repository.path(),
        ["worktree", "list", "--porcelain", "-z"],
    )?;
    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;
    for field in listing.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: PathBuf::from(path),
                branch: None,
                prunable: false,
            });
        } else if let Some(branch) = field.strip_prefix("branch ") {
            if let Some(entry) = &mut current {
                entry.branch = Some(branch.to_string());
            }
        } else if field.starts_with("prunable") {
            if let Some(entry) = &mut current {
                entry.prunable = true;
            }
        } else if field.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    Ok(entries)
}

fn classify_branch(
    repository: &Repository,
    entry: &WorktreeEntry,
    expected_branch: &str,
    expected_base_commit: &str,
) -> Result<bool, Error> {
    let expected_branch = expected_branch
        .strip_prefix("refs/heads/")
        .unwrap_or(expected_branch);
    let Some(actual_branch) = entry
        .branch
        .as_deref()
        .and_then(|branch| branch.strip_prefix("refs/heads/"))
    else {
        return Ok(false);
    };
    if actual_branch != expected_branch {
        return Ok(false);
    }

    let head = git("read worktree HEAD", &entry.path, ["rev-parse", "HEAD"])?;
    let branch_ref = format!("refs/heads/{expected_branch}");
    let Some(branch_head) = read_ref(repository, &branch_ref)? else {
        return Ok(false);
    };
    Ok(head == branch_head && branch_head == expected_base_commit)
}

fn read_ref(repository: &Repository, reference: &str) -> Result<Option<String>, Error> {
    git(
        "validate ref name",
        repository.path(),
        ["check-ref-format", reference],
    )?;
    let expression = format!("{reference}^{{object}}");
    match git(
        "read ref",
        repository.path(),
        [
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &expression,
        ],
    ) {
        Ok(object) => Ok(Some(object)),
        Err(Error::Git { status, .. }) if status.code() == Some(1) => Ok(None),
        Err(error) => Err(error),
    }
}

fn normalize_path(path: &Path) -> std::io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if path.is_absolute() {
                Ok(path.to_path_buf())
            } else {
                Ok(std::env::current_dir()?.join(path))
            }
        }
        Err(error) => Err(error),
    }
}
