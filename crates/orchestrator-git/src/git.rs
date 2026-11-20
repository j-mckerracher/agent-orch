use std::ffi::OsStr;
use std::process::Command;

use crate::Error;

/// Git for Windows rejects paths over MAX_PATH unless this is set. A data
/// directory plus the 69-character task id (`task-` + 64 hex) and Git's ref
/// lock/reflog suffixes can pass 259 characters. It is a no-op elsewhere.
pub(super) const LONG_PATHS: [&str; 2] = ["-c", "core.longpaths=true"];

/// The `-c` settings every Git call inside an agent's slot runs with. The agent can write
/// config and hooks into its slot, so these win over them: a hook or fsmonitor it wrote never
/// runs under the daemon. Command-line config outranks every file the agent can edit.
const SLOT_GUARD: [&str; 4] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
];

/// A Git command for a directory inside an agent's slot, with the [`SLOT_GUARD`] settings. Every
/// daemon-run Git call in a slot starts here, so none is added without them.
pub fn slot_git_command(slot: &std::path::Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(slot).args(LONG_PATHS).args(SLOT_GUARD);
    command
}

/// Run the system Git executable from `dir` and return trimmed stdout.
/// Args are passed as-is (no encoding); the working directory and every
/// argument is a literal value, never a user-controlled option.
pub(super) fn git<I, A>(step: &'static str, dir: &std::path::Path, args: I) -> Result<String, Error>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let stdout = git_bytes(step, dir, args)?;
    Ok(String::from_utf8_lossy(&stdout).trim().to_string())
}

/// [`git`] for a directory inside an agent's slot: same output, run with [`SLOT_GUARD`].
pub(super) fn slot_git<I, A>(
    step: &'static str,
    slot: &std::path::Path,
    args: I,
) -> Result<String, Error>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let output = slot_git_command(slot)
        .args(args)
        .output()
        .map_err(Error::Io)?;
    if !output.status.success() {
        return Err(Error::Git {
            step,
            status: output.status,
            stderr: super::error::first_line(&output.stderr),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn git_bytes<I, A>(
    step: &'static str,
    dir: &std::path::Path,
    args: I,
) -> Result<Vec<u8>, Error>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let output = Command::new("git")
        .current_dir(dir)
        .args(LONG_PATHS)
        .args(args)
        .output()
        .map_err(Error::Io)?;
    if !output.status.success() {
        return Err(Error::Git {
            step,
            status: output.status,
            stderr: super::error::first_line(&output.stderr),
        });
    }
    Ok(output.stdout)
}
