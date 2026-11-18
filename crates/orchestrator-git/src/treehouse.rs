//! The bundled treehouse program (github.com/kunchenguid/treehouse, v3.1.8).
//!
//! Treehouse keeps a pool of worktree slots under `<root>/.treehouse/` and
//! leases one to a task. A returned slot is reset and handed out again, so
//! later tasks reuse it. [`crate::create_task_worktree`] pins each lease to the
//! commit it resolved and names the task branch.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use orchestrator_private_fs::{create_private_dir, create_private_file};
use serde_json::Value;

use crate::error::{stderr_detail, Error};
use crate::process::{run_bounded, RunError};

/// Slots treehouse may hold, written to the private config so the pool never
/// falls back to treehouse's default.
pub(crate) const MAX_TREES: u32 = 1024;

#[cfg(not(test))]
const GET_TIMEOUT: Duration = Duration::from_secs(300);
/// Long enough for a real slot to be created, short enough for the timeout test.
#[cfg(test)]
const GET_TIMEOUT: Duration = Duration::from_secs(20);
const RETURN_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_STDOUT_BYTES: usize = 64 * 1024;
/// Exit status of `treehouse return` for a dirty slot it did not return.
const DIRTY_EXIT_CODE: i32 = 3;

/// Outcome of returning a leased slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Released {
    /// The slot was reset and can be leased again.
    Returned,
    /// The slot has uncommitted changes, so it stays leased for inspection.
    Dirty,
}

/// The bundled treehouse program and the private home it runs in.
pub struct Treehouse {
    program: PathBuf,
    home: PathBuf,
}

impl Treehouse {
    /// Uses the treehouse program at `program`, and writes its private config under `data_dir`.
    pub fn new(program: impl Into<PathBuf>, data_dir: &Path) -> Result<Self, Error> {
        let home = data_dir.join("treehouse-home");
        let config_dir = home.join(".config").join("treehouse");
        create_private_dir(&config_dir)?;
        let config = config_dir.join("config.toml");
        create_private_file(&config)?;
        std::fs::write(&config, format!("max_trees = {MAX_TREES}\n"))?;
        Ok(Self {
            program: program.into(),
            home,
        })
    }

    /// Leases a slot for `lease_holder` and returns its path. `base` is a local
    /// branch to cut from; `None` lets treehouse infer one, which is fine because
    /// the caller pins the commit afterwards.
    pub(crate) fn get(
        &self,
        root: &Path,
        repo: &Path,
        base: Option<&str>,
        lease_holder: &str,
    ) -> Result<PathBuf, Error> {
        let mut args = vec![
            "get",
            "--lease",
            "--json",
            "--no-fetch",
            "--lease-holder",
            lease_holder,
            "--worktree-path",
            "{pool}/{slot}/{repo}",
        ];
        if let Some(base) = base {
            args.extend(["--base", base]);
        }
        let output = run("get", GET_TIMEOUT, self.command(root, repo, args))?;
        if !output.status.success() {
            if String::from_utf8_lossy(&output.stderr).contains("worktrees are in use") {
                return Err(Error::WorktreePoolFull);
            }
            return Err(failed("get", output));
        }
        let lease: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
            Error::TreehouseUnavailable {
                detail: format!("lease output is not JSON: {error}"),
            }
        })?;
        lease
            .get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| Error::TreehouseUnavailable {
                detail: "lease output has no path".into(),
            })
    }

    /// Returns the slot at `path`, only while `lease_holder` still holds it. Never
    /// forces: a dirty slot is reported as [`Released::Dirty`] and left leased.
    pub(crate) fn ret(
        &self,
        root: &Path,
        path: &Path,
        lease_holder: &str,
    ) -> Result<Released, Error> {
        let args: [&OsStr; 4] = [
            OsStr::new("return"),
            path.as_os_str(),
            OsStr::new("--if-lease-holder"),
            OsStr::new(lease_holder),
        ];
        let output = run("return", RETURN_TIMEOUT, self.command(root, root, args))?;
        match output.status.code() {
            Some(0) => Ok(Released::Returned),
            Some(DIRTY_EXIT_CODE) => Ok(Released::Dirty),
            _ => Err(failed("return", output)),
        }
    }

    /// The treehouse child for one call, run from `cwd` with `--root` set.
    ///
    /// Treehouse reads its config and lifecycle hooks only from `HOME`, which
    /// points at the private home, so the user's hooks never run. Git still reads
    /// the user's global config, named through `GIT_CONFIG_GLOBAL`. Inherited
    /// `TREEHOUSE_*` settings are dropped; the ones treehouse needs are set here.
    fn command<I, S>(&self, root: &Path, cwd: &Path, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.program);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("TREEHOUSE_") {
                command.env_remove(name);
            }
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            command.env("GIT_CONFIG_GLOBAL", PathBuf::from(home).join(".gitconfig"));
        }
        command
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.longpaths")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env("TREEHOUSE_VCS", "git")
            .env("TREEHOUSE_NO_UPDATE_CHECK", "1")
            .env("LC_ALL", "C")
            .arg("--root")
            .arg(root)
            .args(args);
        command
    }
}

/// Runs a treehouse command with a deadline, mapping run failures to errors.
fn run(step: &'static str, timeout: Duration, mut command: Command) -> Result<Output, Error> {
    run_bounded(&mut command, timeout, MAX_STDOUT_BYTES).map_err(|error| match error {
        RunError::Spawn(error) => Error::TreehouseUnavailable {
            detail: error.to_string(),
        },
        RunError::Timeout => Error::TreehouseTimeout { step },
        RunError::Failed(code) => Error::TreehouseUnavailable {
            detail: code.into(),
        },
    })
}

fn failed(step: &'static str, output: Output) -> Error {
    Error::TreehouseFailed {
        step,
        status: output.status,
        stderr: stderr_detail(&output.stderr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Treehouse reads hooks from `HOME`, so pointing `HOME` at the private home keeps the
    /// user's hooks from running.
    #[test]
    fn user_treehouse_hooks_are_not_run() {
        let base = TempDir::new().unwrap();
        let treehouse = Treehouse::new("treehouse", base.path()).unwrap();
        let command = treehouse.command(base.path(), base.path(), ["get"]);
        let env: Vec<_> = command.get_envs().collect();
        let home = treehouse.home.as_os_str();
        assert!(env.contains(&(OsStr::new("HOME"), Some(home))), "{env:?}");
        assert!(
            env.contains(&(OsStr::new("USERPROFILE"), Some(home))),
            "{env:?}"
        );
    }

    #[test]
    fn private_config_sets_the_pool_size() {
        let base = TempDir::new().unwrap();
        Treehouse::new("treehouse", base.path()).unwrap();
        let config = base
            .path()
            .join("treehouse-home/.config/treehouse/config.toml");
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            format!("max_trees = {MAX_TREES}\n")
        );
    }
}
