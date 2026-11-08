use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::git::git;

/// A validated Git repository. Operations never change the primary checkout:
/// no branch switches, resets, or deletions of uncommitted work.
pub struct Repository {
    /// Canonical work-tree root of the repository.
    path: PathBuf,
    /// Git data directory (`.git` file for worktrees, `gitdir:` for submodules).
    git_common_dir: PathBuf,
}

impl Repository {
    /// Validate that `path` is a Git work tree and resolve its canonical
    /// paths. Any work tree (including one checked out inside another
    /// repository's worktree) is accepted; a dirty state is not an error.
    pub fn new(path: &Path) -> Result<Self, Error> {
        let toplevel = match git(
            "discover repository",
            path,
            &["rev-parse", "--show-toplevel"],
        ) {
            Ok(out) => out,
            Err(Error::Git { stderr, .. }) => {
                return Err(Error::not_a_repository(path.to_path_buf(), stderr));
            }
            Err(other) => return Err(other),
        };
        let toplevel = PathBuf::from(&toplevel);
        let common = git(
            "discover git data dir",
            &toplevel,
            &["rev-parse", "--git-common-dir"],
        )?;
        let git_common_dir = toplevel.join(&common);
        Ok(Self {
            git_common_dir,
            path: toplevel,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn git_common_dir(&self) -> &Path {
        &self.git_common_dir
    }

    /// Resolve a base ref to its commit. A dirty primary checkout is
    /// allowed: `git worktree add` never reads or writes the primary work
    /// tree, so the user's uncommitted changes are preserved and are not
    /// carried into the task branch.
    pub fn resolve_base_ref(&self, refspec: &str) -> Result<String, Error> {
        let sha = match git(
            "resolve base ref",
            &self.path,
            &["rev-parse", "--verify", "--quiet", &format!("{refspec}^{{commit}}")],
        ) {
            Ok(sha) => sha,
            // `rev-parse --verify` exits non-zero (often with empty stderr)
            // for any ref that does not resolve to a commit.
            Err(Error::Git { stderr, .. }) => {
                return Err(Error::InvalidBaseRef {
                    refspec: refspec.to_string(),
                    detail: if stderr.is_empty() {
                        "ref did not resolve to a commit".to_string()
                    } else {
                        stderr
                    },
                });
            }
            Err(other) => return Err(other),
        };
        if sha.is_empty() {
            return Err(Error::InvalidBaseRef {
                refspec: refspec.to_string(),
                detail: "ref did not resolve to a commit".to_string(),
            });
        }
        Ok(sha)
    }

    /// Reject a worktrees root that lives inside any of this repository's
    /// work trees (the primary checkout or a linked worktree). Task
    /// worktrees must live outside every work tree (plan section 8) so they
    /// can never collide with or mask user files in the repository.
    pub fn worktrees_root_is_outside(&self, root: &Path) -> Result<(), Error> {
        let root = canonicalize_or_parent(root)?;
        let listing = git("list worktrees", &self.path, &["worktree", "list", "--porcelain"])?;
        for line in listing.lines() {
            let Some(path) = line.strip_prefix("worktree ") else {
                continue;
            };
            let wt = PathBuf::from(path);
            let wt = std::fs::canonicalize(&wt).unwrap_or(wt);
            if root.starts_with(&wt) {
                return Err(Error::WorktreesRootInsideRepository {
                    root: root.to_path_buf(),
                    toplevel: wt,
                });
            }
        }
        Ok(())
    }
}

/// Canonicalize `path`, or its deepest existing ancestor with the remaining
/// components appended, so a not-yet-created root can still be validated.
fn canonicalize_or_parent(path: &Path) -> std::io::Result<PathBuf> {
    if let Ok(abs) = std::fs::canonicalize(path) {
        return Ok(abs);
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot normalize a worktrees root without a parent directory",
        ));
    };
    Ok(std::fs::canonicalize(parent)?.join(name))
}
