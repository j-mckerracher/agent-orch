use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::error::Error;
use crate::git::{git, git_bytes};

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
            ["rev-parse", "--show-toplevel"],
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
            ["rev-parse", "--git-common-dir"],
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
        // A leading dash would be read as a `git rev-parse` option.
        if refspec.starts_with('-') || refspec.contains(['\n', '\r', '\0']) {
            return Err(Error::InvalidBaseRef {
                refspec: refspec.to_string(),
                detail: "ref must not start with '-' or contain control characters".to_string(),
            });
        }
        let sha = match git(
            "resolve base ref",
            &self.path,
            [
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{refspec}^{{commit}}"),
            ],
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
    /// Reads a diff from `base_sha` to this repository, restricted to one
    /// relative path when supplied. Absolute paths must remain under this
    /// worktree; parent traversal and Git pathspec magic cannot escape scope.
    /// Updated: 2026-11-08T17:01:05+00:00.
    pub fn diff_from(&self, base_sha: &str, path: Option<&Path>) -> Result<String, Error> {
        if !is_full_object_id(base_sha) || self.resolve_base_ref(base_sha)? != base_sha {
            return Err(Error::InvalidDiffBase);
        }
        let path = path.map(|path| self.diff_path(path)).transpose()?.flatten();
        let mut args: Vec<OsString> = [
            "--literal-pathspecs",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            base_sha,
            "--",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        if let Some(path) = path {
            args.push(path.into());
        }
        let output = git_bytes("read scoped worktree diff", &self.path, args)?;
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    /// Lists files changed between two commits, in Git's byte order.
    ///
    /// Both ids must be full object ids that resolve to those exact commits.
    /// The result reads committed objects only and never the work tree.
    pub fn changed_files_between(
        &self,
        base_sha: &str,
        candidate_sha: &str,
    ) -> Result<Vec<String>, Error> {
        self.require_commit_pair(base_sha, candidate_sha)?;
        self.list_changed_files(base_sha, candidate_sha)
    }

    /// Reads a diff between two commits, restricted to one relative path.
    ///
    /// The diff is pinned to the named commits, so later work-tree edits or new
    /// commits cannot change it. Path rules match [`Repository::diff_from`].
    pub fn diff_between(
        &self,
        base_sha: &str,
        candidate_sha: &str,
        path: Option<&Path>,
    ) -> Result<String, Error> {
        self.require_commit_pair(base_sha, candidate_sha)?;
        self.read_commit_diff(base_sha, candidate_sha, path)
    }

    /// Changed files and the diff between two commits, validating the commit
    /// pair once. Same rules and errors as calling [`Repository::changed_files_between`]
    /// then [`Repository::diff_between`], with two fewer Git processes.
    pub fn snapshot_between(
        &self,
        base_sha: &str,
        candidate_sha: &str,
        path: Option<&Path>,
    ) -> Result<(Vec<String>, String), Error> {
        self.require_commit_pair(base_sha, candidate_sha)?;
        let files = self.list_changed_files(base_sha, candidate_sha)?;
        let diff = self.read_commit_diff(base_sha, candidate_sha, path)?;
        Ok((files, diff))
    }

    fn list_changed_files(
        &self,
        base_sha: &str,
        candidate_sha: &str,
    ) -> Result<Vec<String>, Error> {
        let output = git_bytes(
            "list candidate files",
            &self.path,
            [
                "--literal-pathspecs",
                "diff",
                "--name-only",
                "-z",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                base_sha,
                candidate_sha,
                "--",
            ],
        )?;
        Ok(output
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect())
    }

    fn read_commit_diff(
        &self,
        base_sha: &str,
        candidate_sha: &str,
        path: Option<&Path>,
    ) -> Result<String, Error> {
        let path = path.map(|path| self.diff_path(path)).transpose()?.flatten();
        let mut args: Vec<OsString> = [
            "--literal-pathspecs",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            base_sha,
            candidate_sha,
            "--",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        if let Some(path) = path {
            args.push(path.into());
        }
        let output = git_bytes("read candidate diff", &self.path, args)?;
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    fn require_commit_pair(&self, base_sha: &str, candidate_sha: &str) -> Result<(), Error> {
        if !is_full_object_id(base_sha) || self.resolve_base_ref(base_sha)? != base_sha {
            return Err(Error::InvalidDiffBase);
        }
        if !is_full_object_id(candidate_sha)
            || self.resolve_base_ref(candidate_sha)? != candidate_sha
        {
            return Err(Error::InvalidDiffCandidate);
        }
        Ok(())
    }

    fn diff_path(&self, path: &Path) -> Result<Option<String>, Error> {
        if path.as_os_str().is_empty() {
            return Err(Error::InvalidDiffPath);
        }
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.path)
                .map_err(|_| Error::DiffPathOutsideWorktree)?
        } else {
            path
        };
        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(Error::DiffPathOutsideWorktree);
        }
        let relative = relative.to_str().ok_or(Error::InvalidDiffPath)?;
        if relative.is_empty() || relative == "." {
            Ok(None)
        } else {
            Ok(Some(relative.to_owned()))
        }
    }

    /// Reject a worktrees root that lives inside any of this repository's
    /// work trees (the primary checkout or a linked worktree). Task
    /// worktrees must live outside every work tree (plan section 8) so they
    /// can never collide with or mask user files in the repository.
    pub fn worktrees_root_is_outside(&self, root: &Path) -> Result<(), Error> {
        let root = canonicalize_or_parent(root)?;
        let listing = git(
            "list worktrees",
            &self.path,
            ["worktree", "list", "--porcelain"],
        )?;
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
fn is_full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
