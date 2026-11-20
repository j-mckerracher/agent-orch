//! A task run's status channel and resumable session (DPES-1108 phase 4).
//!
//! A task worker is a headless run with no input channel. It reports on one status file in the
//! daemon's data directory, outside its worktree, and a reply reaches it only by resuming the
//! same CLI session with the reply as the next prompt. Each CLI was checked against its own
//! `--help` (Claude Code 2.1.311, Codex CLI 0.169.0, Copilot CLI 1.0.113):
//!
//! - Claude Code: `--session-id <uuid>` names a new session and `--resume <id>` continues it in
//!   print mode. The permission flags are per run, so a resume repeats the same ones. Task runs
//!   keep no `--no-session-persistence` (only chat turns pass it), so the session is saved.
//!   `--add-dir` grants the status directory.
//! - Copilot CLI: `--session-id=<uuid>` names a new session and `--resume=<id>` continues one.
//!   The permission flags are per run and repeated. `--add-dir` passes path verification for the
//!   status directory and `--allow-tool write(<dir>/**)` lets the write rule cover it.
//! - Codex: `exec` names its own session and reports it in its `thread.started` event;
//!   `codex exec resume [OPTIONS] <id> <prompt>` continues it. Task runs pass no `--ephemeral`.
//!   `exec` takes `--add-dir` for the status directory. `exec resume` has no `--sandbox`,
//!   `--add-dir`, `-C` or `--approve-for-me`: see [`crate::CodexAdapter::args_for_resume`].
//!
//! Nothing here widens a level. Every argument added is the one status directory, a session
//! id, or a level's own flags repeated.

use std::path::Path;

use orchestrator_core::permission::PermissionLevel;

/// The session a task run starts or continues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskSession<'a> {
    /// A new session with this id. Codex chooses its own id, so it ignores this one.
    New(&'a str),
    /// Continue the session with this id.
    Resume(&'a str),
}

/// Why `agent` at `level` cannot resume a task session, or `None` when it can. A run that
/// cannot resume at the same level fails closed: the daemon never resumes at another level.
pub fn resume_refusal(agent: &str, level: PermissionLevel) -> Option<&'static str> {
    match (agent, level) {
        ("codex", PermissionLevel::Auto) => Some(
            "Codex cannot continue an Auto task's session: `codex exec resume` has no \
             --approve-for-me, and its `-c` equivalent is not verified live yet",
        ),
        ("codex", PermissionLevel::Extended) => {
            Some("Codex has no mapping for the Extended permission level")
        }
        ("codex" | "claude-code" | crate::COPILOT_AGENT, _) => None,
        _ => Some("this agent has no daemon launcher"),
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `--add-dir <dir>` for each directory a run may write beyond its worktree: the status
/// directory, then the git directories a slot commits through. Claude, Codex `exec` and Copilot
/// all take this form.
pub fn add_dir_args(dirs: &[&Path]) -> Vec<String> {
    dirs.iter()
        .flat_map(|dir| ["--add-dir".to_owned(), path_text(dir)])
        .collect()
}

/// Copilot's `--add-dir` for each directory, plus below Full a write rule for it; `--allow-all`
/// covers it at Full. The session flag is meant to follow, so the variadic `--allow-tool` list
/// ends at a flag.
pub fn copilot_dir_args(dirs: &[&Path], level: PermissionLevel) -> Vec<String> {
    let mut args = Vec::new();
    for dir in dirs {
        args.extend(["--add-dir".to_owned(), path_text(dir)]);
        if level != PermissionLevel::Full {
            args.extend([
                "--allow-tool".to_owned(),
                format!("write({}/**)", path_text(dir)),
            ]);
        }
    }
    args
}

/// Claude Code's session flag and its value.
pub fn claude_session_args(session: TaskSession<'_>) -> [String; 2] {
    match session {
        TaskSession::New(id) => ["--session-id".to_owned(), id.to_owned()],
        TaskSession::Resume(id) => ["--resume".to_owned(), id.to_owned()],
    }
}

/// Copilot's session flag, with its value in the same argument.
pub fn copilot_session_arg(session: TaskSession<'_>) -> String {
    match session {
        TaskSession::New(id) => format!("--session-id={id}"),
        TaskSession::Resume(id) => format!("--resume={id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn claude_and_copilot_name_a_new_session_and_continue_it_with_the_same_id() {
        let dir = Path::new("/data/task-status/t-1/1");
        assert_eq!(
            add_dir_args(&[dir]),
            ["--add-dir", "/data/task-status/t-1/1"]
        );
        assert_eq!(
            claude_session_args(TaskSession::New("u-1")),
            ["--session-id".to_owned(), "u-1".to_owned()]
        );
        assert_eq!(
            claude_session_args(TaskSession::Resume("u-1")),
            ["--resume".to_owned(), "u-1".to_owned()]
        );
        assert_eq!(
            copilot_dir_args(&[dir], PermissionLevel::Restricted),
            [
                "--add-dir",
                "/data/task-status/t-1/1",
                "--allow-tool",
                "write(/data/task-status/t-1/1/**)",
            ]
        );
        assert_eq!(
            copilot_dir_args(&[dir], PermissionLevel::Full),
            ["--add-dir", "/data/task-status/t-1/1"]
        );
        assert_eq!(
            copilot_session_arg(TaskSession::New("u-1")),
            "--session-id=u-1"
        );
        assert_eq!(
            copilot_session_arg(TaskSession::Resume("u-1")),
            "--resume=u-1"
        );
    }

    #[test]
    fn only_codex_auto_and_extended_refuse_to_resume() {
        for level in [
            PermissionLevel::Restricted,
            PermissionLevel::Extended,
            PermissionLevel::Auto,
            PermissionLevel::Full,
        ] {
            assert_eq!(resume_refusal("claude-code", level), None, "{level:?}");
            assert_eq!(resume_refusal(crate::COPILOT_AGENT, level), None);
            let codex = resume_refusal("codex", level);
            assert_eq!(
                codex.is_some(),
                matches!(level, PermissionLevel::Auto | PermissionLevel::Extended),
                "{level:?}"
            );
        }
        assert!(resume_refusal("pi", PermissionLevel::Restricted).is_some());
    }

    #[test]
    fn git_grants_are_the_slot_gitdir_and_the_refs_and_objects_only() {
        let status = Path::new("/data/task-status/t-1/1");
        let git = [
            PathBuf::from("/repo/.git/worktrees/slot-1"),
            PathBuf::from("/repo/.git/objects"),
            PathBuf::from("/repo/.git/refs/heads"),
            PathBuf::from("/repo/.git/logs/refs/heads"),
        ];
        let dirs: Vec<&Path> = std::iter::once(status)
            .chain(git.iter().map(PathBuf::as_path))
            .collect();
        assert_eq!(
            add_dir_args(&dirs),
            [
                "--add-dir",
                "/data/task-status/t-1/1",
                "--add-dir",
                "/repo/.git/worktrees/slot-1",
                "--add-dir",
                "/repo/.git/objects",
                "--add-dir",
                "/repo/.git/refs/heads",
                "--add-dir",
                "/repo/.git/logs/refs/heads",
            ]
        );
        assert_eq!(
            copilot_dir_args(&dirs[..2], PermissionLevel::Auto),
            [
                "--add-dir",
                "/data/task-status/t-1/1",
                "--allow-tool",
                "write(/data/task-status/t-1/1/**)",
                "--add-dir",
                "/repo/.git/worktrees/slot-1",
                "--allow-tool",
                "write(/repo/.git/worktrees/slot-1/**)",
            ]
        );
    }
}
