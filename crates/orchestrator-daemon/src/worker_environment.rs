//! What a worker's environment needs before its agent starts (DPES-1126).
//!
//! The pstack workflow agent and the input files a task names are copied into the attempt's
//! status directory, which is the one path outside the worktree the agent may write. The agent
//! never reaches the shared plugin cache or the user's files. A preflight refuses a launch the
//! environment cannot honour, before the agent starts, and each refusal is a `cannot_start:`
//! reason the task records. No reason names a path, because a reason reaches the task page.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use orchestrator_agents::workflow_agent::{self, WorkflowAgent};
use orchestrator_core::brief::{ShellAccess, StagedInput};
use orchestrator_core::contract::TaskKind;
use orchestrator_core::permission::PermissionLevel;
use orchestrator_core::work_item::WorkItem;
use orchestrator_core::worker_status::STATUS_FILE_NAME;
use orchestrator_git::{slot_attached_to, Repository, SlotDirs};

use super::{Checkout, Job};

/// The copied plugin, under the attempt's status directory.
pub(crate) const PLUGIN_DIR: &str = "plugin";
/// The copied input files, under the attempt's status directory.
const INPUTS_DIR: &str = "inputs";

/// Implementation work at restricted permission, where the agent has no shell.
pub(crate) const RESTRICTED_NO_SHELL: &str = "cannot_start: this task is set to restricted \
    permission, which gives the agent no shell, and implementation work needs one. Set the task \
    to auto or full permission and start it again.";
/// Implementation work at extended permission, where the agent's shell reads git only.
pub(crate) const EXTENDED_READ_ONLY_SHELL: &str = "cannot_start: this task is set to extended \
    permission, which gives the agent only read-only git, and implementation work needs a shell \
    that runs tests and commits. Set the task to auto or full permission and start it again.";
pub(crate) const PLUGIN_MISSING: &str =
    "cannot_start: the pstack workflow agent plugin is not installed";
pub(crate) const PLUGIN_UNSTAGEABLE: &str =
    "cannot_start: the pstack workflow agent plugin cannot be copied for this attempt";
pub(crate) const STAGED_PLUGIN_MISSING: &str =
    "cannot_start: this attempt's copy of the pstack workflow agent plugin is missing";
pub(crate) const INPUT_MISSING: &str = "cannot_start: an input file the task names is missing";
pub(crate) const INPUT_UNREADABLE: &str =
    "cannot_start: an input file the task names cannot be read";
pub(crate) const STATUS_UNWRITABLE: &str =
    "cannot_start: the attempt's status directory is not writable";
pub(crate) const SLOT_UNHEALTHY: &str =
    "cannot_start: the task's worktree is not a healthy Git checkout";

/// What a first launch staged and what its slot grants the agent.
pub(crate) struct Launch {
    pub(crate) inputs: Vec<StagedInput>,
    /// The slot's Git directories. `None` for an in-place task, which has no slot.
    pub(crate) slot: Option<SlotDirs>,
}

/// The shell an agent has at `level`, from the flags its launch gives it. Codex sandboxes its
/// shell at every level it runs; Claude and Copilot give none at Restricted, read-only git at
/// Extended, and a checked shell at Auto.
pub(crate) fn shell_access(agent: &str, level: PermissionLevel) -> ShellAccess {
    match (agent, level) {
        (_, PermissionLevel::Full) => ShellAccess::Unchecked,
        ("codex", _) => ShellAccess::Checked,
        (_, PermissionLevel::Restricted) => ShellAccess::None,
        (_, PermissionLevel::Extended) => ShellAccess::ReadOnlyGit,
        (_, PermissionLevel::Auto) => ShellAccess::Checked,
    }
}

/// The refusal for an implementation task whose level gives it no shell that can build or test.
/// Read-only tasks are never refused here.
fn shell_refusal(kind: TaskKind, agent: &str, level: PermissionLevel) -> Option<&'static str> {
    if kind != TaskKind::Implement {
        return None;
    }
    match shell_access(agent, level) {
        ShellAccess::None => Some(RESTRICTED_NO_SHELL),
        ShellAccess::ReadOnlyGit => Some(EXTENDED_READ_ONLY_SHELL),
        ShellAccess::Checked | ShellAccess::Unchecked => None,
    }
}

/// Checks and stages a first launch: the shell rule, the status directory, the plugin, the named
/// inputs, then the slot. `located` is the plugin the daemon found for this launch.
pub(super) fn prepare_launch(
    job: &Job,
    checkout: &Checkout,
    kind: TaskKind,
    located: Option<WorkflowAgent>,
    item: Option<&WorkItem>,
) -> Result<Launch, &'static str> {
    if let Some(refusal) = shell_refusal(kind, &job.agent, job.permission) {
        return Err(refusal);
    }
    check_status_dir(&job.status_dir)?;
    let located = located.ok_or(PLUGIN_MISSING)?;
    workflow_agent::stage(&located, &job.status_dir.join(PLUGIN_DIR))
        .map_err(|_| PLUGIN_UNSTAGEABLE)?;
    let inputs = stage_inputs(&job.status_dir, item)?;
    let slot = if job.in_place {
        None
    } else {
        Some(slot_dirs(&checkout.path, &checkout.branch)?)
    };
    Ok(Launch { inputs, slot })
}

/// Checks what a resumed run still needs, on the status directory and slot its first launch made.
/// The slot is checked against the project's own repository before any Git runs in it, because
/// the agent has been running in it. Returns the slot's Git directories for the resumed run.
pub(super) fn check_resume(
    job: &Job,
    kind: TaskKind,
    branch: &str,
    project_root: &str,
) -> Result<Option<SlotDirs>, &'static str> {
    if let Some(refusal) = shell_refusal(kind, &job.agent, job.permission) {
        return Err(refusal);
    }
    check_status_dir(&job.status_dir)?;
    if workflow_agent::plugin_at(&job.status_dir.join(PLUGIN_DIR)).is_none() {
        return Err(STAGED_PLUGIN_MISSING);
    }
    check_inputs_readable(&job.status_dir.join(INPUTS_DIR))?;
    if job.in_place {
        return Ok(None);
    }
    let repository = Repository::new(Path::new(project_root)).map_err(|_| SLOT_UNHEALTHY)?;
    if !slot_attached_to(&job.worktree, repository.git_common_dir()) {
        return Err(SLOT_UNHEALTHY);
    }
    slot_dirs(&job.worktree, branch).map(Some)
}

/// The slot's Git directories for `branch`, or the refusal when Git cannot read the slot.
fn slot_dirs(worktree: &Path, branch: &str) -> Result<SlotDirs, &'static str> {
    orchestrator_git::slot_git_dirs(worktree, branch).map_err(|_| SLOT_UNHEALTHY)
}

/// The status file opens for writing, so the worker can report and the daemon can stage.
fn check_status_dir(status_dir: &Path) -> Result<(), &'static str> {
    OpenOptions::new()
        .append(true)
        .open(status_dir.join(STATUS_FILE_NAME))
        .map(drop)
        .map_err(|_| STATUS_UNWRITABLE)
}

/// Every copied input still opens. A directory that was never made holds no inputs.
fn check_inputs_readable(inputs: &Path) -> Result<(), &'static str> {
    let Ok(entries) = fs::read_dir(inputs) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry.map_err(|_| INPUT_UNREADABLE)?;
        File::open(entry.path()).map_err(|_| INPUT_UNREADABLE)?;
    }
    Ok(())
}

/// Copies each file the work item names into `status_dir/inputs`, numbered so two files of one
/// name stay apart. A named path that is not a file, or cannot be read, refuses the launch.
fn stage_inputs(
    status_dir: &Path,
    item: Option<&WorkItem>,
) -> Result<Vec<StagedInput>, &'static str> {
    let named = item.map(named_files).unwrap_or_default();
    if named.is_empty() {
        return Ok(Vec::new());
    }
    let dir = status_dir.join(INPUTS_DIR);
    fs::create_dir_all(&dir).map_err(|_| STATUS_UNWRITABLE)?;
    named
        .into_iter()
        .enumerate()
        .map(|(index, original)| {
            if !original.is_file() {
                return Err(INPUT_MISSING);
            }
            File::open(&original).map_err(|_| INPUT_UNREADABLE)?;
            let name = original.file_name().unwrap_or_default().to_string_lossy();
            let copy = dir.join(format!("{}-{name}", index + 1));
            fs::copy(&original, &copy).map_err(|_| STATUS_UNWRITABLE)?;
            Ok(StagedInput { original, copy })
        })
        .collect()
}

/// The absolute paths with a file extension that the work item's ask and criteria name, once
/// each. A route such as `/api/orders` has no extension and names no file.
fn named_files(item: &WorkItem) -> Vec<PathBuf> {
    let texts = std::iter::once(item.ask.as_str())
        .chain(item.acceptance_criteria.iter().map(|c| c.text.as_str()));
    let mut paths: Vec<PathBuf> = Vec::new();
    for token in texts.flat_map(str::split_whitespace) {
        let token = token
            .trim_matches(|c: char| "`'\"()[]<>,;:".contains(c))
            .trim_end_matches('.');
        let path = Path::new(token);
        if path.is_absolute() && path.extension().is_some() && !paths.iter().any(|p| p == path) {
            paths.push(path.to_path_buf());
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_each_level_gives_matches_its_launch_flags() {
        use PermissionLevel::{Auto, Extended, Full, Restricted};
        assert_eq!(shell_access("claude-code", Restricted), ShellAccess::None);
        assert_eq!(
            shell_access("github-copilot", Restricted),
            ShellAccess::None
        );
        assert_eq!(
            shell_access("claude-code", Extended),
            ShellAccess::ReadOnlyGit
        );
        assert_eq!(shell_access("claude-code", Auto), ShellAccess::Checked);
        assert_eq!(shell_access("claude-code", Full), ShellAccess::Unchecked);
        assert_eq!(shell_access("codex", Restricted), ShellAccess::Checked);
    }

    #[test]
    fn only_implementation_work_is_refused_without_a_shell() {
        use PermissionLevel::{Auto, Extended, Restricted};
        assert_eq!(
            shell_refusal(TaskKind::Implement, "claude-code", Restricted),
            Some(RESTRICTED_NO_SHELL)
        );
        assert_eq!(
            shell_refusal(TaskKind::Implement, "github-copilot", Extended),
            Some(EXTENDED_READ_ONLY_SHELL)
        );
        assert_eq!(
            shell_refusal(TaskKind::Investigate, "claude-code", Restricted),
            None
        );
        assert_eq!(
            shell_refusal(TaskKind::Implement, "codex", Restricted),
            None
        );
        assert_eq!(
            shell_refusal(TaskKind::Implement, "claude-code", Auto),
            None
        );
    }

    #[test]
    fn a_named_file_is_an_absolute_path_with_an_extension() {
        use orchestrator_core::work_item::{Criterion, SourceKind, WorkSource};
        let criterion = |text: &str| Criterion {
            text: text.into(),
            drafted: false,
            confirmed: true,
        };
        // A Unix path has no drive, so it is not absolute on Windows: start from the platform's root.
        let root = if cfg!(windows) {
            Path::new(r"C:\")
        } else {
            Path::new("/")
        };
        let brief = root.join("Users/me/spec/brief.md");
        let data = root.join("tmp/data.csv");
        let item = WorkItem {
            source: WorkSource {
                kind: SourceKind::Text,
                reference: None,
            },
            title: "Spec".into(),
            ask: format!("Use `{}` and /api/orders.", brief.display()),
            acceptance_criteria: vec![
                criterion(&format!("Match the table in {}.", data.display())),
                criterion("The route /api/orders stays put."),
            ],
        };
        assert_eq!(named_files(&item), vec![brief, data]);
    }

    /// A work item whose ask names `path`.
    fn item_naming(path: &Path) -> WorkItem {
        WorkItem {
            source: orchestrator_core::work_item::WorkSource {
                kind: orchestrator_core::work_item::SourceKind::Text,
                reference: None,
            },
            title: "Spec".into(),
            ask: format!("Match {}.", path.display()),
            acceptance_criteria: Vec::new(),
        }
    }

    #[test]
    fn a_directory_that_is_not_a_git_slot_is_unhealthy() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            slot_dirs(dir.path(), "mcao/task").map(drop),
            Err(SLOT_UNHEALTHY)
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_unwritable_status_file_refuses_the_launch() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let status = dir.path().join(STATUS_FILE_NAME);
        fs::write(&status, "").unwrap();
        fs::set_permissions(&status, fs::Permissions::from_mode(0o400)).unwrap();
        if OpenOptions::new().append(true).open(&status).is_ok() {
            // A root user writes through any mode, so the refusal cannot be shown here.
            return;
        }
        assert_eq!(check_status_dir(dir.path()), Err(STATUS_UNWRITABLE));
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_named_input_refuses_the_staging() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("locked.md");
        fs::write(&input, "spec\n").unwrap();
        fs::set_permissions(&input, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&input).is_ok() {
            // A root user reads any file, so the refusal cannot be shown here.
            return;
        }
        let status = dir.path().join("status");
        let staged = stage_inputs(&status, Some(&item_naming(&input)));
        assert_eq!(staged.map(drop), Err(INPUT_UNREADABLE));
    }

    #[test]
    fn a_named_input_that_is_not_a_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let named = dir.path().join("gone.md");
        let staged = stage_inputs(dir.path(), Some(&item_naming(&named)));
        assert_eq!(staged.map(drop), Err(INPUT_MISSING));
    }
}
