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
            "Codex cannot continue an Auto task's session at the same permission level: \
             `codex exec resume` has no --approve-for-me",
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

/// Claude Code: the status directory and the session, appended to the task arguments.
pub fn claude_args(status_dir: &Path, session: TaskSession<'_>) -> Vec<String> {
    let (flag, id) = match session {
        TaskSession::New(id) => ("--session-id", id),
        TaskSession::Resume(id) => ("--resume", id),
    };
    vec![
        "--add-dir".to_owned(),
        path_text(status_dir),
        flag.to_owned(),
        id.to_owned(),
    ]
}

/// Copilot: the status directory, a write rule for it below Full (`--allow-all` covers it at
/// Full), and the session, appended to the task arguments. The session flag comes last, so the
/// variadic `--allow-tool` list ends at a flag.
pub fn copilot_args(
    status_dir: &Path,
    level: PermissionLevel,
    session: TaskSession<'_>,
) -> Vec<String> {
    let dir = path_text(status_dir);
    let mut args = vec!["--add-dir".to_owned(), dir.clone()];
    if level != PermissionLevel::Full {
        args.push("--allow-tool".to_owned());
        args.push(format!("write({dir}/**)"));
    }
    args.push(match session {
        TaskSession::New(id) => format!("--session-id={id}"),
        TaskSession::Resume(id) => format!("--resume={id}"),
    });
    args
}

/// Codex `exec`: the status directory, inserted after `exec`.
pub fn codex_launch_args(status_dir: &Path) -> [String; 2] {
    ["--add-dir".to_owned(), path_text(status_dir)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_and_copilot_name_a_new_session_and_continue_it_with_the_same_id() {
        let dir = Path::new("/data/tasks/t-1/1");
        assert_eq!(
            claude_args(dir, TaskSession::New("u-1")),
            ["--add-dir", "/data/tasks/t-1/1", "--session-id", "u-1"]
        );
        assert_eq!(
            claude_args(dir, TaskSession::Resume("u-1")),
            ["--add-dir", "/data/tasks/t-1/1", "--resume", "u-1"]
        );
        assert_eq!(
            copilot_args(dir, PermissionLevel::Restricted, TaskSession::New("u-1")),
            [
                "--add-dir",
                "/data/tasks/t-1/1",
                "--allow-tool",
                "write(/data/tasks/t-1/1/**)",
                "--session-id=u-1"
            ]
        );
        assert_eq!(
            copilot_args(dir, PermissionLevel::Full, TaskSession::Resume("u-1")),
            ["--add-dir", "/data/tasks/t-1/1", "--resume=u-1"]
        );
        assert_eq!(
            codex_launch_args(dir),
            ["--add-dir".to_owned(), "/data/tasks/t-1/1".to_owned()]
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
}
