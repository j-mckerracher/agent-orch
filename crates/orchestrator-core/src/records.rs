//! Durable records that do not own lifecycle transition rules.
//!
//! Paths and status strings remain opaque here. The store persists paths, but
//! it does not open workspaces, logs, or artifact files. Updated: 2026-11-07T22:12:39-07:00.

/// A registered repository and its project-specific settings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Project {
    /// Stable project identifier referenced by task contracts.
    pub id: String,
    /// Repository root path, persisted as supplied by the caller.
    pub root_path: String,
    /// Project settings encoded as a JSON value.
    pub settings: serde_json::Value,
}

/// A task-owned Git worktree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    /// Stable workspace identifier.
    pub id: String,
    /// Task that owns this worktree.
    pub task_id: String,
    /// Worktree path. The store records it and does not access the filesystem.
    pub path: String,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Commit from which the worktree was created.
    pub base_commit: String,
}

/// Evidence from one verification run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerificationRun {
    /// Stable verification-run identifier.
    pub id: String,
    /// Task whose candidate was checked.
    pub task_id: String,
    /// Candidate commit checked by the verifier.
    pub commit: String,
    /// Verifier-owned status value. Core does not define its vocabulary.
    pub status: String,
    /// Optional path to the verifier log, which remains outside SQLite.
    pub log_path: Option<String>,
}

/// A retained output associated with a task.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Artifact {
    /// Stable artifact identifier.
    pub id: String,
    /// Task that produced this artifact.
    pub task_id: String,
    /// Artifact kind, such as a diff or report.
    pub kind: String,
    /// File path. Artifact contents remain outside SQLite.
    pub path: String,
    /// Checksum supplied by the artifact owner.
    pub checksum: String,
}
