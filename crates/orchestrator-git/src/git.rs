use std::ffi::OsStr;
use std::process::Command;

use crate::Error;

/// Git for Windows rejects paths over MAX_PATH unless this is set. A data
/// directory plus the 69-character task id (`task-` + 64 hex) and Git's ref
/// lock/reflog suffixes can pass 259 characters. It is a no-op elsewhere.
pub(super) const LONG_PATHS: [&str; 2] = ["-c", "core.longpaths=true"];

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
