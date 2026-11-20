use std::path::{Path, PathBuf};

use crate::branch_prefix::BranchPrefix;
use crate::error::{stderr_detail, Error};
use crate::git::{git, slot_git, slot_git_command};
use crate::git_path;
use crate::repository::Repository;
use crate::treehouse::{Released, Treehouse, MAX_TREES};

/// Task-owned branch worktree, created at the resolved base commit.
///
/// Invariants: the branch is named for this task and never reused across
/// tasks. The directory is a treehouse slot, which a later task can lease once
/// this one is released. Creation never modifies the primary checkout. On
/// failure the slot is returned to the pool when treehouse allows it; nothing
/// in this crate deletes worktrees, branches, or files.
#[derive(Debug)]
pub struct TaskWorktree {
    path: PathBuf,
    branch: String,
    base_sha: String,
}

impl TaskWorktree {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Resolved commit the worktree was created from.
    pub fn base_sha(&self) -> &str {
        &self.base_sha
    }
}

const MAX_NAME_ATTEMPTS: u32 = 1024;

/// Longest worktree path, in characters, that `git worktree add` can create on
/// Windows. Git runs the checkout step with `GIT_DIR=<worktree>\.git`, and
/// `setup_explicit_git_dir` dies with `'$GIT_DIR' too big` past
/// `PATH_MAX - 40` (260 - 40 = 220 on Windows), whatever `core.longpaths` says:
/// 220 minus the 5 characters of `\.git`. `core.longpaths` still lets the
/// files inside the checkout exceed `MAX_PATH`.
///
/// Observed: Git 2.46.2 for Windows failed with `'$GIT_DIR' too big` at a
/// worktree path over 260 characters; Git 2.56 on macOS (PATH_MAX 1024) accepts
/// 969 and rejects 981 characters, the same formula with its own `PATH_MAX`.
/// The exact Windows boundary is derived, and a Windows test creates a worktree
/// at exactly this length.
const WINDOWS_MAX_WORKTREE_PATH: usize = 260 - 40 - r"\.git".len();

/// Refuses a worktree path Git for Windows cannot create, before any Git
/// state exists. `limit` is `None` where Git has no such limit in practice.
fn ensure_worktree_path_fits(path: &Path, limit: Option<usize>) -> Result<(), Error> {
    let Some(max) = limit else { return Ok(()) };
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    // Git measures the UTF-8 bytes of the absolute path.
    let length = absolute.to_string_lossy().len();
    if length > max {
        return Err(Error::WorktreePathTooLong {
            path: absolute,
            length,
            max,
        });
    }
    Ok(())
}

/// Lease a treehouse slot and create the task branch in it at the resolved
/// commit of `base_ref`.
///
/// The branch is `<branch_prefix><slug>`, with a numeric suffix on collision.
/// The slot lives under `worktrees_root`, which must live outside the
/// repository's work trees. The primary checkout is never modified. On failure
/// the lease is returned, so the slot can be reused; a slot treehouse refuses to
/// return stays leased for inspection.
pub fn create_task_worktree(
    repo: &Repository,
    task_id: &str,
    base_ref: &str,
    worktrees_root: &Path,
    branch_prefix: &BranchPrefix,
    treehouse: &Treehouse,
    lease_holder: &str,
) -> Result<TaskWorktree, Error> {
    repo.worktrees_root_is_outside(worktrees_root)?;
    // The caller may pass a verbatim `\\?\C:\..` path (from `canonicalize`),
    // which Git for Windows cannot create directories under.
    let worktrees_root = &git_path::for_git(worktrees_root);
    let slug = sanitize_task_id(task_id)?;
    let sha = repo.resolve_base_ref(base_ref)?;
    ensure_worktree_path_fits(
        &deepest_slot_path(worktrees_root, repo.path()),
        cfg!(windows).then_some(WINDOWS_MAX_WORKTREE_PATH),
    )?;

    let base = base_branch(repo, base_ref);
    let path = treehouse.get(worktrees_root, repo.path(), base.as_deref(), lease_holder)?;
    let created = create_branch(repo, &path, &slug, branch_prefix, &sha);
    if created.is_err() {
        // Best effort: a slot that cannot be returned stays leased, to be released with its task.
        let _ = release_task_worktree(treehouse, worktrees_root, &path, lease_holder);
    }
    created
}

/// Return the slot at `path` to the treehouse pool, while `lease_holder` still
/// holds it. `path` must lie under `worktrees_root`.
///
/// A dirty slot is never forced: it is reported as [`Released::Dirty`] and
/// stays leased.
pub fn release_task_worktree(
    treehouse: &Treehouse,
    worktrees_root: &Path,
    path: &Path,
    lease_holder: &str,
) -> Result<Released, Error> {
    let root = git_path::for_git(worktrees_root);
    let inside = std::fs::canonicalize(path)?.starts_with(std::fs::canonicalize(&root)?);
    if !inside {
        return Err(Error::WorktreeOutsideRoot {
            path: path.to_path_buf(),
            root,
        });
    }
    treehouse.ret(&root, path, lease_holder)
}

/// The local branch treehouse should cut the slot from: `base_ref` when it names
/// one (`main` or `refs/heads/main`), or HEAD's branch. `None` for anything else,
/// so treehouse infers its default; the commit is pinned afterwards either way.
fn base_branch(repo: &Repository, base_ref: &str) -> Option<String> {
    let name = if base_ref == "HEAD" {
        git(
            "read HEAD branch",
            repo.path(),
            ["symbolic-ref", "--quiet", "--short", "HEAD"],
        )
        .ok()?
    } else {
        base_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(base_ref)
            .to_string()
    };
    git(
        "check base branch",
        repo.path(),
        [
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
    .ok()?;
    Some(name)
}

/// The deepest path treehouse can give a slot of this repository. Its pool is
/// `<root>/.treehouse/<repo>-<6 hex>`, slots are numbered up to [`MAX_TREES`],
/// and each slot holds the repository at `<repo>`. The Windows length check uses
/// this path, so it holds for whichever slot is leased.
fn deepest_slot_path(root: &Path, repo: &Path) -> PathBuf {
    let name = repo
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    root.join(".treehouse")
        .join(format!("{name}-000000"))
        .join(MAX_TREES.to_string())
        .join(&name)
}

/// Create the task branch in the leased slot at `sha`, taking the next free `-N`
/// suffix when a branch of that name already exists.
fn create_branch(
    repo: &Repository,
    path: &Path,
    slug: &str,
    branch_prefix: &BranchPrefix,
    sha: &str,
) -> Result<TaskWorktree, Error> {
    for attempt in 0..MAX_NAME_ATTEMPTS {
        let branch = if attempt == 0 {
            format!("{branch_prefix}{slug}")
        } else {
            format!("{branch_prefix}{slug}-{attempt}")
        };
        let branch_taken = git(
            "check branch availability",
            repo.path(),
            [
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_ok();
        if branch_taken {
            continue;
        }
        match switch_to_new_branch(path, &branch, sha) {
            Ok(()) => {
                verify_head(path, sha)?;
                return Ok(TaskWorktree {
                    path: path.to_path_buf(),
                    branch,
                    base_sha: sha.to_string(),
                });
            }
            // A concurrent creator took the name after the check: try the next one.
            Err(err) if matches!(&err, Error::Git { stderr, .. } if is_name_collision(stderr)) => {}
            Err(err) => {
                return Err(Error::CreationFailed {
                    path: path.to_path_buf(),
                    branch,
                    source: Box::new(err),
                });
            }
        }
    }
    Err(Error::NameExhausted {
        base: format!("{branch_prefix}{slug}"),
        attempts: MAX_NAME_ATTEMPTS,
    })
}

/// Create `branch` at `sha` and check it out in the slot at `path`. `LC_ALL=C`
/// (on this child only) fixes git's message wording so `is_name_collision` does
/// not depend on the user's locale.
fn switch_to_new_branch(path: &Path, branch: &str, sha: &str) -> Result<(), Error> {
    // `-c` names the start point, so git never guesses a branch from `sha`.
    let output = slot_git_command(path)
        .env("LC_ALL", "C")
        .args(["switch", "-c"])
        .arg(branch)
        .arg(sha)
        .output()?;
    if !output.status.success() {
        return Err(Error::Git {
            step: "switch worktree branch",
            status: output.status,
            stderr: stderr_detail(&output.stderr),
        });
    }
    Ok(())
}

/// True when git reports that the branch already exists, i.e. a concurrent
/// creator took the name. `stderr` is `stderr_detail` output (lines joined by
/// `; `). Matches git's own whole `fatal:` line, not the phrase anywhere, so an
/// unrelated failure whose path contains "already exists" is not retried.
fn is_name_collision(stderr: &str) -> bool {
    stderr.split("; ").any(|line| {
        line.strip_prefix("fatal: ").is_some_and(|message| {
            (message.starts_with("a branch named '") || message.starts_with('\''))
                && message.ends_with("' already exists")
        })
    })
}

fn verify_head(path: &Path, sha: &str) -> Result<(), Error> {
    let head = slot_git("verify worktree head", path, ["rev-parse", "HEAD"])?;
    if head != sha {
        return Err(Error::HeadMismatch {
            path: path.to_path_buf(),
            expected: sha.to_string(),
            found: head,
        });
    }
    Ok(())
}

/// The Git directories a slot is granted at launch: the shared Git directory the slot is attached
/// to, and the directories an agent's commit on `branch` writes. Read from Git before any agent
/// runs, so they are the trusted values later checks compare against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotDirs {
    /// The shared Git directory (`--git-common-dir`) of the repository the slot belongs to.
    pub common: PathBuf,
    /// The slot's own Git directory, the shared object store, and the branch's ref and reflog
    /// directories. Git writes a lock file beside a ref, so the branch's directory is the
    /// narrowest grant that lets a commit land. Never the shared config, hooks or info.
    pub grants: Vec<PathBuf>,
}

/// Reads the slot's Git directories for `branch` and creates the branch's ref directories if Git
/// has not yet. Fails when Git cannot read the slot.
pub fn slot_git_dirs(worktree: &Path, branch: &str) -> Result<SlotDirs, Error> {
    let gitdir = slot_git(
        "read slot git directory",
        worktree,
        ["rev-parse", "--path-format=absolute", "--git-dir"],
    )?;
    let common = PathBuf::from(slot_git(
        "read shared git directory",
        worktree,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    let branch_dir = Path::new(branch).parent().unwrap_or(Path::new(""));
    let refs = common.join("refs/heads").join(branch_dir);
    let reflogs = common.join("logs/refs/heads").join(branch_dir);
    // Git creates a branch's directory only for a loose ref, so an empty one may be missing.
    for dir in [&refs, &reflogs] {
        std::fs::create_dir_all(dir)?;
    }
    let grants = vec![PathBuf::from(gitdir), common.join("objects"), refs, reflogs];
    Ok(SlotDirs { common, grants })
}

/// Whether the slot at `worktree` is still attached to the repository whose shared Git directory
/// is `common`, and carries no per-worktree config. It reads the slot's files and runs no Git: an
/// agent may have rewritten them, and a Git command would then read another repository's config,
/// or a per-worktree config whose filter drivers run on `git add`.
pub fn slot_attached_to(worktree: &Path, common: &Path) -> bool {
    let Ok(pointer) = std::fs::read_to_string(worktree.join(".git")) else {
        return false;
    };
    let Some(gitdir) = pointer.trim().strip_prefix("gitdir:") else {
        return false;
    };
    let gitdir = worktree.join(gitdir.trim());
    if gitdir.join("config.worktree").exists() {
        return false;
    }
    let Ok(shared) = std::fs::read_to_string(gitdir.join("commondir")) else {
        return false;
    };
    match (
        std::fs::canonicalize(gitdir.join(shared.trim())),
        std::fs::canonicalize(common),
    ) {
        (Ok(attached), Ok(expected)) => attached == expected,
        _ => false,
    }
}

/// Sanitize a task id into a git-ref-safe fragment: no `~`, `..`, spaces,
/// `/`, `^`, `:`, `\`, `?`, `*`, `[`, no leading `-`, and no trailing
/// `.lock`. The input may be untrusted, so every disallowed character is
/// replaced rather than rejected.
pub(crate) fn sanitize_task_id(task_id: &str) -> Result<String, Error> {
    let sanitized: String = task_id
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    let trimmed = sanitized
        .trim_start_matches('-')
        .trim_end_matches(['.', '-']);
    let trimmed = trimmed.strip_suffix(".lock").unwrap_or(trimmed);
    if trimmed.is_empty() {
        return Err(Error::EmptyTaskId);
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::bundled_treehouse;
    use std::process::Command;
    use tempfile::TempDir;

    /// Run git from `dir` with global and system config pinned to
    /// non-existent files so ambient machine state cannot change behavior.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
            .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Like `git`, but reports success without asserting; used to probe for
    /// refs that must NOT exist.
    fn git_ok(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
            .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-b", "main"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "user.name", "T"]);
        std::fs::write(dir.join("base.txt"), "base\n").unwrap();
        git(dir, &["add", "base.txt"]);
        git(dir, &["commit", "-m", "initial"]);
    }

    /// Fresh repository, a worktrees root outside it (sibling in the same
    /// tempdir), and the bundled treehouse with its private home in the same
    /// tempdir. All are owned by the test and cleaned up by tempfile.
    fn fixture() -> (TempDir, PathBuf, Repository, Treehouse) {
        let base = TempDir::new().unwrap();
        let repo_dir = base.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        init_repo(&repo_dir);
        let repo = Repository::new(&repo_dir).expect("temp dir is a repository");
        let wts = base.path().join("worktrees");
        let treehouse = bundled_treehouse(base.path());
        (base, wts, repo, treehouse)
    }

    #[test]
    fn open_accepts_repo_and_isolates_primary() {
        let (tmp, wts, repo, treehouse) = fixture();
        let head = git(tmp.path().join("repo").as_path(), &["rev-parse", "HEAD"]);
        let wt = create_task_worktree(
            &repo,
            "task-a1b2c3",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "task-a1b2c3-workspace-1",
        )
        .unwrap();
        assert!(wt.path().exists());
        assert_eq!(wt.branch(), "mcao/task-a1b2c3");
        assert_eq!(wt.base_sha(), head);
        // Worktree content is the committed base (line endings per the
        // checkout on this platform).
        assert!(
            std::fs::read_to_string(wt.path().join("base.txt"))
                .unwrap()
                .starts_with("base"),
            "worktree must contain the committed base content"
        );
        assert_eq!(
            git(tmp.path().join("repo").as_path(), &["rev-parse", "HEAD"]),
            head,
            "primary HEAD must not move"
        );
        let status = git(
            tmp.path().join("repo").as_path(),
            &["status", "--porcelain"],
        );
        assert!(
            status.lines().all(|l| l.contains(".git/worktrees")),
            "primary checkout must stay clean apart from worktree metadata: {status}"
        );
    }

    /// The CLI canonicalizes the worktrees root, which on Windows yields a
    /// verbatim `\\?\C:\..` path that Git for Windows rejects with
    /// `could not create leading directories ... Invalid argument`.
    /// Not runnable off Windows (there `canonicalize` is already plain).
    #[cfg(windows)]
    #[test]
    fn verbatim_worktrees_root_creates_a_worktree() {
        let (_tmp, wts, repo, treehouse) = fixture();
        std::fs::create_dir_all(&wts).unwrap();
        let verbatim = std::fs::canonicalize(&wts).unwrap();
        assert!(
            verbatim.to_string_lossy().starts_with(r"\\?\"),
            "{verbatim:?}"
        );
        let wt = create_task_worktree(
            &repo,
            "accept-run",
            "HEAD",
            &verbatim,
            &BranchPrefix::default(),
            &treehouse,
            "accept-run-workspace-1",
        )
        .unwrap();
        assert!(wt.path().join("base.txt").exists());
        assert!(
            !wt.path().to_string_lossy().starts_with(r"\\?\"),
            "{:?}",
            wt.path()
        );
    }

    /// A checkout whose files go past MAX_PATH (260) must still be created:
    /// Git for Windows fails creating checkout files without `core.longpaths`.
    /// The slot path itself stays within `WINDOWS_MAX_WORKTREE_PATH`.
    #[test]
    fn checkout_files_longer_than_max_path_are_created() {
        let (tmp, wts, repo, treehouse) = fixture();
        let repo_dir = tmp.path().join("repo");
        let mut deep = PathBuf::new();
        for _ in 0..4 {
            deep.push("long-path-segment-long-path-segment-long-path-segment");
        }
        std::fs::create_dir_all(repo_dir.join(&deep)).unwrap();
        std::fs::write(repo_dir.join(&deep).join("deep.txt"), "deep\n").unwrap();
        git(&repo_dir, &["-c", "core.longpaths=true", "add", "."]);
        git(
            &repo_dir,
            &["-c", "core.longpaths=true", "commit", "-m", "deep file"],
        );

        let wt = create_task_worktree(
            &repo,
            "deep-checkout",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "deep-checkout-workspace-1",
        )
        .unwrap();
        let file = wt.path().join(&deep).join("deep.txt");
        assert!(file.as_os_str().len() > 260, "{file:?}");
        assert!(file.exists(), "{file:?}");
    }

    /// Builds `wts\<a>\<b>` exactly `total` characters long.
    fn root_of_length(wts: &Path, total: usize) -> PathBuf {
        let tail = total
            .checked_sub(wts.as_os_str().len())
            .filter(|tail| *tail >= 4)
            .unwrap_or_else(|| panic!("{wts:?} is too long for a {total} character root"));
        let first = (tail - 2) / 2;
        let second = tail - 2 - first;
        let mut root = wts.to_path_buf();
        root.push("a".repeat(first));
        root.push("b".repeat(second));
        root
    }

    /// Characters the pool adds to a worktrees root before the slot path: the
    /// `.treehouse` pool directory, the slot number and the repository name.
    fn slot_overhead(wts: &Path, repo: &Path) -> usize {
        deepest_slot_path(wts, repo).as_os_str().len() - wts.as_os_str().len()
    }

    /// The longest worktree path Git accepts must work: the Windows limit is
    /// derived from Git's source, and this is what proves it is not too low. The
    /// limit applies to the deepest slot treehouse can hand out, so the root is
    /// sized for slot MAX_TREES; the slot leased here is shorter.
    #[test]
    fn worktree_at_the_longest_accepted_path_is_created() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let root = root_of_length(
            &wts,
            WINDOWS_MAX_WORKTREE_PATH - slot_overhead(&wts, repo.path()),
        );
        std::fs::create_dir_all(&root).unwrap();
        let wt = create_task_worktree(
            &repo,
            "longest",
            "HEAD",
            &root,
            &BranchPrefix::default(),
            &treehouse,
            "longest-workspace-1",
        )
        .unwrap();
        assert!(
            wt.path().as_os_str().len() <= WINDOWS_MAX_WORKTREE_PATH,
            "{:?}",
            wt.path()
        );
        assert!(wt.path().join("base.txt").exists());
    }

    #[test]
    fn a_worktree_path_over_the_limit_is_refused_naming_the_limit() {
        let absolute_of_length = |total: usize| {
            let prefix = if cfg!(windows) { r"C:\" } else { "/" };
            PathBuf::from(format!("{prefix}{}", "x".repeat(total - prefix.len())))
        };
        let path = absolute_of_length(WINDOWS_MAX_WORKTREE_PATH + 1);
        let err = ensure_worktree_path_fits(&path, Some(WINDOWS_MAX_WORKTREE_PATH)).unwrap_err();
        match &err {
            Error::WorktreePathTooLong { length, max, .. } => {
                assert!(*length > *max && *max == WINDOWS_MAX_WORKTREE_PATH, "{err}");
            }
            other => panic!("{other:?}"),
        }
        assert!(err.to_string().contains("Nothing was created"));
        let at_limit = absolute_of_length(WINDOWS_MAX_WORKTREE_PATH);
        ensure_worktree_path_fits(&at_limit, Some(WINDOWS_MAX_WORKTREE_PATH)).unwrap();
        ensure_worktree_path_fits(&path, None).unwrap();
    }

    /// Where the limit applies, creation is refused before any lease or Git
    /// state exists. Windows only: elsewhere Git has no such limit.
    #[cfg(windows)]
    #[test]
    fn a_worktree_over_the_windows_limit_is_refused_before_any_git_state_exists() {
        let (tmp, wts, repo, treehouse) = fixture();
        let root = root_of_length(
            &wts,
            WINDOWS_MAX_WORKTREE_PATH + 1 - slot_overhead(&wts, repo.path()),
        );
        std::fs::create_dir_all(&root).unwrap();
        let err = create_task_worktree(
            &repo,
            "too-long",
            "HEAD",
            &root,
            &BranchPrefix::default(),
            &treehouse,
            "too-long-workspace-1",
        )
        .unwrap_err();
        assert!(matches!(err, Error::WorktreePathTooLong { .. }), "{err:?}");
        assert!(!root.join(".treehouse").exists());
        let repo_dir = tmp.path().join("repo");
        assert!(!git_ok(
            &repo_dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/mcao/too-long"
            ]
        ));
    }

    #[test]
    fn dirty_primary_checkout_preserved() {
        let (tmp, wts, repo, treehouse) = fixture();
        let repo_dir = tmp.path().join("repo");
        let head = git(&repo_dir, &["rev-parse", "HEAD"]);
        std::fs::write(repo_dir.join("uncommitted.txt"), "wip\n").unwrap();
        git(&repo_dir, &["add", "uncommitted.txt"]);
        let wt = create_task_worktree(
            &repo,
            "dirty-test",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "dirty-test-workspace-1",
        )
        .unwrap();
        // Worktree carries committed content only.
        assert!(
            !wt.path().join("uncommitted.txt").exists(),
            "uncommitted work must not leak into the task worktree"
        );
        // Primary dirty state is intact.
        let status = git(&repo_dir, &["status", "--porcelain"]);
        assert!(
            status.contains("uncommitted.txt"),
            "primary uncommitted work must survive: {status}"
        );
        assert_eq!(git(&repo_dir, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn resolve_base_commit_rejects_invalid_ref() {
        let (tmp, _wts, repo, _treehouse) = fixture();
        let head = git(tmp.path().join("repo").as_path(), &["rev-parse", "HEAD"]);
        match repo.resolve_base_ref("no-such-ref") {
            Err(Error::InvalidBaseRef { .. }) => {}
            other => panic!("expected InvalidBaseRef, got {other:?}"),
        }
        assert_eq!(repo.resolve_base_ref("HEAD").unwrap(), head);
    }

    #[test]
    fn resolve_base_ref_refuses_option_like_and_control_character_refs() {
        let (_tmp, _wts, repo, _treehouse) = fixture();
        for refspec in ["--all", "-h", "HEAD\nmain", "HEAD\0"] {
            match repo.resolve_base_ref(refspec) {
                Err(Error::InvalidBaseRef { .. }) => {}
                other => panic!("expected InvalidBaseRef for {refspec:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn empty_task_id_rejected() {
        let (_tmp, wts, repo, treehouse) = fixture();
        for id in ["", "---", "-"] {
            let r = create_task_worktree(
                &repo,
                id,
                "HEAD",
                &wts,
                &BranchPrefix::default(),
                &treehouse,
                "holder",
            );
            assert!(
                matches!(r, Err(Error::EmptyTaskId)),
                "task id {id:?} should be rejected, got {r:?}"
            );
        }
        // Rejected ids create nothing.
        assert!(!wts.exists());
        // Symbols are replaced, not rejected: "~~~" becomes a valid id.
        let ok = create_task_worktree(
            &repo,
            "~~~",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "holder",
        )
        .unwrap();
        assert_eq!(ok.branch(), "mcao/___");
    }

    #[test]
    fn worktrees_root_inside_repository_rejected() {
        let (tmp, _wts, repo, treehouse) = fixture();
        let inside = tmp.path().join("repo").join("worktrees");
        match create_task_worktree(
            &repo,
            "inside-test",
            "HEAD",
            &inside,
            &BranchPrefix::default(),
            &treehouse,
            "holder",
        ) {
            Err(Error::WorktreesRootInsideRepository { .. }) => {}
            other => panic!("expected WorktreesRootInsideRepository, got {other:?}"),
        }
        assert!(
            !inside.exists(),
            "no worktree may be created inside the checkout"
        );
    }

    #[test]
    fn collision_allocates_suffix() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let first = create_task_worktree(
            &repo,
            "task-a1b2c3",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "first-workspace-1",
        )
        .unwrap();
        let second = create_task_worktree(
            &repo,
            "task-a1b2c3",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "second-workspace-1",
        )
        .unwrap();
        assert_eq!(first.branch(), "mcao/task-a1b2c3");
        assert_eq!(second.branch(), "mcao/task-a1b2c3-1");
        assert!(
            first.path().exists() && second.path().exists(),
            "both worktrees must coexist"
        );
        assert_ne!(first.path(), second.path());
    }

    #[test]
    fn preexisting_branch_skipped() {
        let (tmp, wts, repo, treehouse) = fixture();
        git(tmp.path().join("repo").as_path(), &["branch", "mcao/x"]);
        let wt = create_task_worktree(
            &repo,
            "x",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "x-workspace-1",
        )
        .unwrap();
        assert_eq!(wt.branch(), "mcao/x-1");
        assert!(wt.path().exists());
    }

    #[test]
    fn worktree_is_pinned_to_the_resolved_sha_when_origin_is_ahead() {
        let (tmp, wts, repo, treehouse) = fixture();
        let repo_dir = tmp.path().join("repo");
        let origin = tmp.path().join("origin.git");
        let clone = tmp.path().join("clone");
        git(tmp.path(), &["init", "--bare", "-b", "main", "origin.git"]);
        git(
            &repo_dir,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&repo_dir, &["push", "origin", "main"]);
        // Origin moves on, and the fetch makes `origin/main` ahead of local `main`.
        git(
            tmp.path(),
            &["clone", origin.to_str().unwrap(), clone.to_str().unwrap()],
        );
        std::fs::write(clone.join("ahead.txt"), "ahead\n").unwrap();
        git(&clone, &["add", "ahead.txt"]);
        git(
            &clone,
            &[
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=T",
                "commit",
                "-m",
                "ahead",
            ],
        );
        git(&clone, &["push", "origin", "main"]);
        git(&repo_dir, &["fetch", "origin"]);
        let local_head = git(&repo_dir, &["rev-parse", "HEAD"]);

        let wt = create_task_worktree(
            &repo,
            "pin",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "pin-workspace-1",
        )
        .unwrap();
        assert_eq!(wt.base_sha(), local_head);
        assert!(
            !wt.path().join("ahead.txt").exists(),
            "the worktree must stay at the resolved commit, not origin/main"
        );
    }

    #[test]
    fn released_slot_is_reused_by_the_next_task() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let first = create_task_worktree(
            &repo,
            "first",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "first-workspace-1",
        )
        .unwrap();
        let released =
            release_task_worktree(&treehouse, &wts, first.path(), "first-workspace-1").unwrap();
        assert_eq!(released, Released::Returned);

        let second = create_task_worktree(
            &repo,
            "second",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "second-workspace-1",
        )
        .unwrap();
        assert_eq!(second.path(), first.path());
        assert_eq!(second.branch(), "mcao/second");
    }

    #[test]
    fn dirty_worktree_is_not_released() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let wt = create_task_worktree(
            &repo,
            "dirty",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "dirty-workspace-1",
        )
        .unwrap();
        std::fs::write(wt.path().join("wip.txt"), "wip\n").unwrap();

        let released =
            release_task_worktree(&treehouse, &wts, wt.path(), "dirty-workspace-1").unwrap();
        assert_eq!(released, Released::Dirty);
        assert!(
            wt.path().join("wip.txt").exists(),
            "uncommitted work must survive"
        );
        // Still leased, so the next task gets another slot.
        let next = create_task_worktree(
            &repo,
            "next",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "next-workspace-1",
        )
        .unwrap();
        assert_ne!(next.path(), wt.path());
    }

    /// Only the holder of a lease may return it: treehouse refuses another holder, so a stale
    /// caller cannot free a slot another task is using.
    #[test]
    fn release_by_another_holder_is_refused() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let wt = create_task_worktree(
            &repo,
            "held",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "held-workspace-1",
        )
        .unwrap();
        let err =
            release_task_worktree(&treehouse, &wts, wt.path(), "other-workspace-1").unwrap_err();
        assert!(
            matches!(err, Error::TreehouseFailed { step: "return", .. }),
            "{err:?}"
        );
        assert!(
            wt.path().join("base.txt").exists(),
            "the slot must stay leased"
        );
    }

    /// A treehouse that never finishes: `get` must be stopped and reported, not waited on.
    #[cfg(unix)]
    #[test]
    fn treehouse_timeout_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, wts, repo, _treehouse) = fixture();
        let program = tmp.path().join("hanging-treehouse");
        std::fs::write(&program, "#!/bin/sh\nexec sleep 600\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let hanging = Treehouse::new(&program, tmp.path()).unwrap();
        let err = create_task_worktree(
            &repo,
            "slow",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &hanging,
            "slow-workspace-1",
        )
        .unwrap_err();
        assert!(
            matches!(err, Error::TreehouseTimeout { step: "get" }),
            "{err:?}"
        );
    }

    #[test]
    fn missing_treehouse_is_reported() {
        let (tmp, wts, repo, _treehouse) = fixture();
        let missing = Treehouse::new(tmp.path().join("no-such-treehouse"), tmp.path()).unwrap();
        let err = create_task_worktree(
            &repo,
            "gone",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &missing,
            "gone-workspace-1",
        )
        .unwrap_err();
        assert!(matches!(err, Error::TreehouseUnavailable { .. }), "{err:?}");
        assert!(!git_ok(
            &tmp.path().join("repo"),
            &["rev-parse", "--verify", "--quiet", "refs/heads/mcao/gone"]
        ));
    }

    #[test]
    fn path_outside_worktrees_root_is_refused() {
        let (tmp, wts, _repo, treehouse) = fixture();
        std::fs::create_dir_all(&wts).unwrap();
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        let err = release_task_worktree(&treehouse, &wts, &outside, "holder").unwrap_err();
        assert!(matches!(err, Error::WorktreeOutsideRoot { .. }), "{err:?}");
    }

    #[test]
    fn name_collision_is_recognized_only_for_gits_own_conflict_lines() {
        let branch = "a branch named 'mcao/t' already exists";
        assert!(is_name_collision(&format!(
            "Preparing worktree; fatal: {branch}"
        )));
        assert!(is_name_collision("fatal: '/w/t' already exists"));
        assert!(!is_name_collision(
            "fatal: could not create leading directories of '/already exists/t/.git': denied"
        ));
        assert!(!is_name_collision("fatal: failed in /tmp/already exists"));
        assert!(!is_name_collision("already exists"));
    }

    #[test]
    fn configured_prefix_names_the_branch_and_collision_suffix() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let prefix = BranchPrefix::parse("team1/").unwrap();
        let first =
            create_task_worktree(&repo, "t", "HEAD", &wts, &prefix, &treehouse, "t-1").unwrap();
        let second =
            create_task_worktree(&repo, "t", "HEAD", &wts, &prefix, &treehouse, "t-2").unwrap();
        assert_eq!(first.branch(), "team1/t");
        assert_eq!(second.branch(), "team1/t-1");
    }

    #[test]
    fn long_stderr_keeps_the_final_cause_within_the_bound() {
        let noise = "é".repeat(2000);
        let stderr = format!("Preparing worktree\n{noise}\nfatal: the real cause\n");
        let shown = crate::error::stderr_detail(stderr.as_bytes());
        assert!(shown.chars().count() <= 1024, "{}", shown.chars().count());
        assert!(shown.ends_with("fatal: the real cause"), "{shown}");
        assert!(
            shown.starts_with("..."),
            "truncation must be visible: {shown}"
        );
    }

    #[test]
    fn stderr_detail_neutralizes_terminal_control_characters() {
        let shown = crate::error::stderr_detail(b"fatal: a\x1b[31mb\x07\x00c\r\nsecond\tline\n");
        assert!(!shown.chars().any(char::is_control), "{shown:?}");
        assert!(
            shown.contains("fatal: a") && shown.contains("second"),
            "{shown:?}"
        );
    }

    #[test]
    fn slot_git_dirs_grant_the_slot_objects_and_the_branch_refs_only() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let prefix = BranchPrefix::default();
        let wt = create_task_worktree(
            &repo,
            "grant",
            "HEAD",
            &wts,
            &prefix,
            &treehouse,
            "grant-workspace-1",
        )
        .unwrap();
        let dirs = slot_git_dirs(wt.path(), wt.branch()).unwrap();
        let common = PathBuf::from(git(
            wt.path(),
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ));
        let gitdir = PathBuf::from(git(
            wt.path(),
            &["rev-parse", "--path-format=absolute", "--git-dir"],
        ));
        // The prefix directory comes from the same `BranchPrefix` the worktree was created with,
        // so a changed default cannot leave the grant pointing at a directory no branch uses.
        let branch_dir = Path::new(prefix.as_str());
        assert_eq!(dirs.common, common);
        assert_eq!(
            dirs.grants,
            vec![
                gitdir,
                common.join("objects"),
                common.join("refs/heads").join(branch_dir),
                common.join("logs/refs/heads").join(branch_dir),
            ]
        );
        for dir in &dirs.grants {
            assert!(dir.is_dir(), "{dir:?}");
            for denied in ["config", "hooks", "info"] {
                assert!(!dir.ends_with(denied), "{dir:?}");
            }
        }
    }

    #[test]
    fn slot_git_dirs_grant_the_configured_prefix_the_branch_was_created_under() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let prefix = BranchPrefix::parse("team1/").unwrap();
        let wt =
            create_task_worktree(&repo, "t", "HEAD", &wts, &prefix, &treehouse, "t-1").unwrap();
        assert_eq!(wt.branch(), "team1/t");

        let dirs = slot_git_dirs(wt.path(), wt.branch()).unwrap();

        let ref_dir = dirs.common.join("refs/heads/team1");
        let log_dir = dirs.common.join("logs/refs/heads/team1");
        assert!(dirs.grants.contains(&ref_dir), "{:?}", dirs.grants);
        assert!(dirs.grants.contains(&log_dir), "{:?}", dirs.grants);
        assert!(!dirs.grants.contains(&dirs.common.join("refs/heads")));
    }

    #[cfg(unix)]
    #[test]
    fn slot_git_runs_no_hook_the_agent_pointed_git_at() {
        let (_tmp, wts, repo, treehouse) = fixture();
        let wt = create_task_worktree(
            &repo,
            "hooked",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "hooked-workspace-1",
        )
        .unwrap();
        let marker = wt.path().join("hook-ran");
        let hooks = wt.path().join("agent-hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(
            hooks.join("pre-commit"),
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(
            hooks.join("pre-commit"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        git(
            wt.path(),
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        );
        std::fs::write(wt.path().join("new.txt"), "new\n").unwrap();
        git(wt.path(), &["add", "new.txt"]);

        let commit = slot_git_command(wt.path())
            .args(["commit", "-m", "daemon"])
            .output()
            .unwrap();

        assert!(commit.status.success(), "{commit:?}");
        assert!(!marker.exists(), "the agent's pre-commit hook ran");
    }

    #[test]
    fn a_slot_whose_commondir_points_at_another_repository_is_not_attached() {
        let (tmp, wts, repo, treehouse) = fixture();
        let wt = create_task_worktree(
            &repo,
            "attached",
            "HEAD",
            &wts,
            &BranchPrefix::default(),
            &treehouse,
            "attached-workspace-1",
        )
        .unwrap();
        let common = PathBuf::from(git(
            wt.path(),
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ));
        assert!(slot_attached_to(wt.path(), &common));

        let other = tmp.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        init_repo(&other);
        let gitdir = PathBuf::from(git(
            wt.path(),
            &["rev-parse", "--path-format=absolute", "--git-dir"],
        ));
        std::fs::write(
            gitdir.join("commondir"),
            other.join(".git").display().to_string(),
        )
        .unwrap();

        assert!(!slot_attached_to(wt.path(), &common));
    }
}
