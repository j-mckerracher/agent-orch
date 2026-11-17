//! The home directory: where a task runs when it is not clear which project it belongs to.
//!
//! The user sets it on the Settings page (`settings.set` with `home_directory`). The daemon
//! resolves what was typed (a leading `~` is the user's home) to a canonical absolute
//! directory, registers that directory as an in-place project (no Git repository needed, no
//! worktree, report-only tasks) and saves the canonical path. A directory that is already a
//! registered project keeps the project it is, so a Git project stays a worktree project.
//!
//! A refusal is `InvalidParams` and carries no path.

use std::path::PathBuf;

use orchestrator_core::orchestrator_action::ActionBlock;
use orchestrator_core::records::IN_PLACE_SETTING;

use super::*;

/// What the user typed, as a canonical absolute directory.
pub(super) fn resolve(typed: &str) -> Result<PathBuf, RpcError> {
    let typed = typed.trim();
    if typed.is_empty() || typed.contains('\0') {
        return Err(RpcError::InvalidParams);
    }
    let expanded = expand_home(typed)?;
    if !expanded.is_absolute() {
        return Err(RpcError::InvalidParams);
    }
    let canonical = std::fs::canonicalize(&expanded).map_err(|_| RpcError::InvalidParams)?;
    if !canonical.is_dir() {
        return Err(RpcError::InvalidParams);
    }
    Ok(strip_verbatim(canonical))
}

/// `~` and `~/rest` (or `~\rest`) become the user's home; every other path is unchanged.
fn expand_home(typed: &str) -> Result<PathBuf, RpcError> {
    let rest = match typed.strip_prefix('~') {
        None => return Ok(PathBuf::from(typed)),
        Some("") => "",
        Some(rest) if rest.starts_with(['/', '\\']) => &rest[1..],
        // `~name` names another user's home, which this setting does not look up.
        Some(_) => return Err(RpcError::InvalidParams),
    };
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
        .ok_or(RpcError::InvalidParams)?;
    Ok(PathBuf::from(home).join(rest))
}

/// The block with each task for an in-place project that asks for `LocalReview` set to
/// `ReportOnly`, the only delivery such a task has. The reply sink calls it before it stores
/// the block, so the card shows, and the digest binds, what will run.
pub(super) fn with_in_place_delivery(store: &SqliteStore, block: ActionBlock) -> ActionBlock {
    block.with_in_place_delivery(|code| {
        super::projects_defaults::project_id_for_code(store, code)
            .and_then(|id| store.get_project(&id).ok())
            .is_some_and(|project| project.is_in_place())
    })
}

/// `fs::canonicalize` answers `\\?\C:\..` on Windows. Git and the agents see `C:\..`.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            let mut chars = rest.chars();
            if chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':') {
                return PathBuf::from(rest);
            }
        }
    }
    path
}

impl DaemonApi {
    /// Makes sure `root` is a registered, unarchived project. A directory nobody registered
    /// becomes an in-place project; one already registered is left as it is. The caller holds
    /// the idempotency gate.
    pub(super) fn ensure_home_project(&self, root: &str) -> Result<(), RpcError> {
        match self.store.get_project(root) {
            Ok(_) => {
                self.store.restore_project(root).map_err(map_store_error)?;
                Ok(())
            }
            Err(StoreError::RecordNotFound { .. }) => self
                .store
                .register_project(&Project {
                    id: root.to_owned(),
                    root_path: root.to_owned(),
                    settings: json!({ IN_PLACE_SETTING: true }),
                })
                .map_err(map_store_error),
            Err(error) => Err(map_store_error(error)),
        }
    }
}
