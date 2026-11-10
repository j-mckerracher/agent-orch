use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{stderr_detail, Error};
use crate::git::{git, LONG_PATHS};
use crate::git_path;
use crate::repository::Repository;

/// A task-owned branch and worktree, created from a resolved base commit.
///
/// Invariants:
/// - The worktree directory and branch name are fresh at creation time;
///   never reused across tasks.
/// - Creation never modifies the primary checkout. On any failure the
///   partially created branch and directory are left in place (reported by
///   [`Error::CreationFailed`]) for inspection; nothing in this crate
///   deletes worktrees, branches, or files.
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

    /// The resolved commit the worktree was created from.
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

/// Create a task branch and worktree from the resolved commit of `base_ref`.
///
/// The worktree directory goes under `worktrees_root`, which must live
/// outside the repository's work trees (plan section 8: a `worktrees/`
/// directory outside the repository, not the primary checkout). The primary
/// checkout is never modified. On failure, the partially created branch and
/// worktree are left in place; the error reports where to inspect them.
pub fn create_task_worktree(
    repo: &Repository,
    task_id: &str,
    base_ref: &str,
    worktrees_root: &Path,
) -> Result<TaskWorktree, Error> {
    repo.worktrees_root_is_outside(worktrees_root)?;
    // The caller may pass a verbatim `\\?\C:\..` path (from `canonicalize`),
    // which Git for Windows cannot create directories under.
    let worktrees_root = &git_path::for_git(worktrees_root);
    let slug = sanitize_task_id(task_id)?;
    let sha = repo.resolve_base_ref(base_ref)?;

    let mut last_err: Option<Box<Error>> = None;
    for attempt in 0..MAX_NAME_ATTEMPTS {
        let (branch, dir) = if attempt == 0 {
            (format!("orchestrator/{slug}"), slug.clone())
        } else {
            (format!("orchestrator/{slug}-{attempt}"), format!("{slug}-{attempt}"))
        };
        let path = worktrees_root.join(&dir);
        let branch_taken = git(
            "check branch availability",
            repo.path(),
            ["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
        )
        .is_ok();
        if branch_taken || path.exists() {
            continue;
        }
        ensure_worktree_path_fits(&path, cfg!(windows).then_some(WINDOWS_MAX_WORKTREE_PATH))?;
        match add_worktree(repo, &branch, &path, &sha) {
            Ok(()) => {
                verify_head(&path, &sha)?;
                return Ok(TaskWorktree {
                    path,
                    branch,
                    base_sha: sha.to_string(),
                });
            }
            Err(err) => {
                // Name race (a concurrent creator won) vs. real failure:
                // git's "already exists" means retry with the next name;
                // anything else leaves the partial state for inspection.
                let race = matches!(&err, Error::Git { stderr, .. }
                    if is_name_collision(stderr));
                last_err = Some(Box::new(err));
                if !race {
                    return Err(Error::CreationFailed {
                        path,
                        branch,
                        source: last_err.take().unwrap(),
                    });
                }
            }
        }
    }
    match last_err {
        Some(err) if matches!(*err, Error::Git { .. }) => Err(Error::NameExhausted {
            base: format!("orchestrator/{slug}"),
            attempts: MAX_NAME_ATTEMPTS,
        }),
        Some(err) => Err(*err),
        None => Err(Error::NameExhausted {
            base: format!("orchestrator/{slug}"),
            attempts: MAX_NAME_ATTEMPTS,
        }),
    }
}

fn add_worktree(repo: &Repository, branch: &str, path: &Path, sha: &str) -> Result<(), Error> {
    if let Err(e) = std::fs::create_dir_all(path.parent().unwrap_or(path)) {
        return Err(Error::Io(e));
    }
    // `--` guards the (sanitized, never leading-dash) worktree path against
    // option injection, and keeps every argument a literal value. `LC_ALL=C`
    // (on this child only) fixes git's message wording so `is_name_collision`
    // and the reported cause do not depend on the user's locale.
    let output = Command::new("git")
        .current_dir(repo.path())
        .env("LC_ALL", "C")
        .args(LONG_PATHS)
        .args(["worktree", "add", "-b"])
        .arg(branch)
        .arg("--")
        .arg(path)
        .arg(sha)
        .output()?;
    if !output.status.success() {
        return Err(Error::Git {
            step: "add worktree",
            status: output.status,
            stderr: stderr_detail(&output.stderr),
        });
    }
    Ok(())
}

/// True when git reports that the branch or worktree path already exists,
/// i.e. a concurrent creator took the name. `stderr` is `stderr_detail`
/// output (lines joined by `; `). Matches git's own whole `fatal:` line, not
/// the phrase anywhere, so an unrelated failure whose path contains
/// "already exists" is not retried.
fn is_name_collision(stderr: &str) -> bool {
    stderr.split("; ").any(|line| {
        line.strip_prefix("fatal: ").is_some_and(|message| {
            (message.starts_with("a branch named '") || message.starts_with('\''))
                && message.ends_with("' already exists")
        })
    })
}

fn verify_head(path: &Path, sha: &str) -> Result<(), Error> {
    let head = git("verify worktree head", path, ["rev-parse", "HEAD"])?;
    if head != sha {
        return Err(Error::HeadMismatch {
            path: path.to_path_buf(),
            expected: sha.to_string(),
            found: head,
        });
    }
    Ok(())
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
    let trimmed = sanitized.trim_start_matches('-').trim_end_matches(['.', '-']);
    let trimmed = trimmed.strip_suffix(".lock").unwrap_or(trimmed);
    if trimmed.is_empty() {
        return Err(Error::EmptyTaskId);
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Fresh repository plus a worktrees root outside it (sibling in the
    /// same tempdir). Both are owned by the test and cleaned up by tempfile.
    fn fixture() -> (TempDir, PathBuf, Repository) {
        let base = TempDir::new().unwrap();
        let repo_dir = base.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        init_repo(&repo_dir);
        let repo = Repository::new(&repo_dir).expect("temp dir is a repository");
        let wts = base.path().join("worktrees");
        (base, wts, repo)
    }

    #[test]
    fn open_accepts_repo_and_isolates_primary() {
        let (tmp, wts, repo) = fixture();
        let head = git(tmp.path().join("repo").as_path(), &["rev-parse", "HEAD"]);
        let wt = create_task_worktree(&repo, "task-a1b2c3", "HEAD", &wts).unwrap();
        assert!(wt.path().exists());
        assert_eq!(wt.branch(), "orchestrator/task-a1b2c3");
        assert_eq!(wt.base_sha(), head);
        // Worktree content is the committed base (line endings per the
        // checkout on this platform).
        assert!(
            std::fs::read_to_string(wt.path().join("base.txt")).unwrap().starts_with("base"),
            "worktree must contain the committed base content"
        );
        assert_eq!(
            git(tmp.path().join("repo").as_path(), &["rev-parse", "HEAD"]),
            head,
            "primary HEAD must not move"
        );
        let status = git(tmp.path().join("repo").as_path(), &["status", "--porcelain"]);
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
        let (_tmp, wts, repo) = fixture();
        std::fs::create_dir_all(&wts).unwrap();
        let verbatim = std::fs::canonicalize(&wts).unwrap();
        assert!(
            verbatim.to_string_lossy().starts_with(r"\\?\"),
            "{verbatim:?}"
        );
        let wt = create_task_worktree(&repo, "accept-run", "HEAD", &verbatim).unwrap();
        assert!(wt.path().join("base.txt").exists());
        assert!(
            !wt.path().to_string_lossy().starts_with(r"\\?\"),
            "{:?}",
            wt.path()
        );
    }

    /// A checkout whose files go past MAX_PATH (260) must still be created:
    /// Git for Windows fails creating checkout files without `core.longpaths`.
    /// The worktree path itself stays short, because Git cannot create a
    /// worktree at a path over `WINDOWS_MAX_WORKTREE_PATH` at all.
    #[test]
    fn checkout_files_longer_than_max_path_are_created() {
        let (tmp, wts, repo) = fixture();
        let repo_dir = tmp.path().join("repo");
        let mut deep = PathBuf::new();
        for _ in 0..4 {
            deep.push("long-path-segment-long-path-segment-long-path-segment");
        }
        std::fs::create_dir_all(repo_dir.join(&deep)).unwrap();
        std::fs::write(repo_dir.join(&deep).join("deep.txt"), "deep\n").unwrap();
        git(&repo_dir, &["-c", "core.longpaths=true", "add", "."]);
        git(&repo_dir, &["-c", "core.longpaths=true", "commit", "-m", "deep file"]);

        let wt = create_task_worktree(&repo, "deep-checkout", "HEAD", &wts).unwrap();
        let file = wt.path().join(&deep).join("deep.txt");
        assert!(file.as_os_str().len() > 260, "{file:?}");
        assert!(file.exists(), "{file:?}");
    }

    /// Builds `wts\<a>\<b>\<id>` with exactly `total` characters.
    fn path_of_length(wts: &Path, id: &str, total: usize) -> PathBuf {
        let rest = total
            .checked_sub(wts.as_os_str().len() + 1 + id.len())
            .filter(|rest| *rest >= 4)
            .unwrap_or_else(|| panic!("{wts:?} is too long for a {total} character worktree path"));
        let first = rest / 2 - 1;
        let second = rest - first - 2;
        let mut root = wts.to_path_buf();
        root.push("a".repeat(first));
        root.push("b".repeat(second));
        root
    }

    /// The longest worktree path Git accepts must work: the Windows limit is
    /// derived from Git's source, and this is what proves it is not too low.
    #[test]
    fn worktree_at_the_longest_accepted_path_is_created() {
        let (_tmp, wts, repo) = fixture();
        let id = "longest";
        let root = path_of_length(&wts, id, WINDOWS_MAX_WORKTREE_PATH);
        std::fs::create_dir_all(&root).unwrap();
        let wt = create_task_worktree(&repo, id, "HEAD", &root).unwrap();
        assert_eq!(wt.path().as_os_str().len(), WINDOWS_MAX_WORKTREE_PATH, "{:?}", wt.path());
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

    /// Where the limit applies, creation is refused before Git runs, so no
    /// branch or directory is left behind. Windows only: elsewhere Git has no
    /// such limit.
    #[cfg(windows)]
    #[test]
    fn a_worktree_over_the_windows_limit_is_refused_before_any_git_state_exists() {
        let (tmp, wts, repo) = fixture();
        let id = "too-long";
        let root = path_of_length(&wts, id, WINDOWS_MAX_WORKTREE_PATH + 1);
        std::fs::create_dir_all(&root).unwrap();
        let err = create_task_worktree(&repo, id, "HEAD", &root).unwrap_err();
        assert!(matches!(err, Error::WorktreePathTooLong { .. }), "{err:?}");
        assert!(!root.join(id).exists());
        let repo_dir = tmp.path().join("repo");
        assert!(!git_ok(
            &repo_dir,
            &["rev-parse", "--verify", "--quiet", "refs/heads/orchestrator/too-long"]
        ));
    }

    #[test]
    fn dirty_primary_checkout_preserved() {
        let (tmp, wts, repo) = fixture();
        let repo_dir = tmp.path().join("repo");
        let head = git(&repo_dir, &["rev-parse", "HEAD"]);
        std::fs::write(repo_dir.join("uncommitted.txt"), "wip\n").unwrap();
        git(&repo_dir, &["add", "uncommitted.txt"]);
        let wt = create_task_worktree(&repo, "dirty-test", "HEAD", &wts).unwrap();
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
        let (tmp, _wts, repo) = fixture();
        let head = git(tmp.path().join("repo").as_path(), &["rev-parse", "HEAD"]);
        match repo.resolve_base_ref("no-such-ref") {
            Err(Error::InvalidBaseRef { .. }) => {}
            other => panic!("expected InvalidBaseRef, got {other:?}"),
        }
        assert_eq!(repo.resolve_base_ref("HEAD").unwrap(), head);
    }

    #[test]
    fn resolve_base_ref_refuses_option_like_and_control_character_refs() {
        let (_tmp, _wts, repo) = fixture();
        for refspec in ["--all", "-h", "HEAD\nmain", "HEAD\0"] {
            match repo.resolve_base_ref(refspec) {
                Err(Error::InvalidBaseRef { .. }) => {}
                other => panic!("expected InvalidBaseRef for {refspec:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn empty_task_id_rejected() {
        let (_tmp, wts, repo) = fixture();
        for id in ["", "---", "-"] {
            let r = create_task_worktree(&repo, id, "HEAD", &wts);
            assert!(
                matches!(r, Err(Error::EmptyTaskId)),
                "task id {id:?} should be rejected, got {r:?}"
            );
        }
        // Rejected ids create nothing.
        assert!(!wts.exists());
        // Symbols are replaced, not rejected: "~~~" becomes a valid id.
        let ok = create_task_worktree(&repo, "~~~", "HEAD", &wts).unwrap();
        assert_eq!(ok.branch(), "orchestrator/___");
    }

    #[test]
    fn worktrees_root_inside_repository_rejected() {
        let (tmp, _wts, repo) = fixture();
        let inside = tmp.path().join("repo").join("worktrees");
        match create_task_worktree(&repo, "inside-test", "HEAD", &inside) {
            Err(Error::WorktreesRootInsideRepository { .. }) => {}
            other => panic!("expected WorktreesRootInsideRepository, got {other:?}"),
        }
        assert!(!inside.exists(), "no worktree may be created inside the checkout");
    }

    #[test]
    fn collision_allocates_suffix() {
        let (_tmp, wts, repo) = fixture();
        let first = create_task_worktree(&repo, "task-a1b2c3", "HEAD", &wts).unwrap();
        let second = create_task_worktree(&repo, "task-a1b2c3", "HEAD", &wts).unwrap();
        assert_eq!(first.branch(), "orchestrator/task-a1b2c3");
        assert_eq!(second.branch(), "orchestrator/task-a1b2c3-1");
        assert!(
            first.path().exists() && second.path().exists(),
            "both worktrees must coexist"
        );
        assert_ne!(first.path(), second.path());
    }

    #[test]
    fn preexisting_branch_skipped() {
        let (tmp, wts, repo) = fixture();
        git(tmp.path().join("repo").as_path(), &["branch", "orchestrator/x"]);
        let wt = create_task_worktree(&repo, "x", "HEAD", &wts).unwrap();
        assert_eq!(wt.branch(), "orchestrator/x-1");
        assert!(wt.path().exists());
    }

    #[test]
    fn git_add_failure_preserves_repo_state_and_reports_partial_state() {
        let (tmp, _wts, repo) = fixture();
        let repo_dir = tmp.path().join("repo");
        let head = git(&repo_dir, &["rev-parse", "HEAD"]);
        let before = git(&repo_dir, &["worktree", "list"]);
        let wts = tmp.path().join("wts-blocked");
        // Block the worktrees root with an existing regular file so the
        // worktree directory cannot be created, cross-platform.
        std::fs::write(&wts, "blocker\n").unwrap();
        std::fs::write(tmp.path().join("precious.txt"), "keep\n").unwrap();
        let r = create_task_worktree(&repo, "fail-test", "HEAD", &wts);
        match &r {
            Err(Error::CreationFailed { path, branch, source }) => {
                assert_eq!(path, &wts.join("fail-test"));
                assert_eq!(branch, "orchestrator/fail-test");
                assert!(
                    matches!(**source, Error::Io(_)),
                    "expected an Io failure, got {source:?}"
                );
            }
            other => panic!("expected CreationFailed, got {other:?}"),
        }
        assert!(
            git(&repo_dir, &["worktree", "list"]) == before,
            "worktree list must be unchanged after a failed add"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("precious.txt")).unwrap(),
            "keep\n",
            "unrelated files must survive a failed add"
        );
        assert!(
            !git_ok(&repo_dir, &["rev-parse", "--verify", "--quiet", "refs/heads/orchestrator/fail-test"]),
            "no branch may remain after a failed add (git itself never created it)"
        );
        assert_eq!(git(&repo_dir, &["rev-parse", "HEAD"]), head);
    }

    /// A branch named `orchestrator` makes `refs/heads/orchestrator/<slug>`
    /// uncreatable (ref directory/file conflict). Git prints a progress line
    /// before its `fatal:` line; the reported error must carry the `fatal:`
    /// cause, not just the progress line, and must keep the original source.
    /// The full ref name appears only on the `fatal:` line, so asserting it
    /// proves the cause survived without depending on Git's message wording.
    const CAUSE_REF: &str = "refs/heads/orchestrator/cause-test";

    #[test]
    fn git_add_failure_reports_git_stderr_cause() {
        let (tmp, wts, repo) = fixture();
        let repo_dir = tmp.path().join("repo");
        git(&repo_dir, &["branch", "orchestrator"]);
        let err = create_task_worktree(&repo, "cause-test", "HEAD", &wts).unwrap_err();
        let shown = err.to_string();
        assert!(shown.contains("worktree creation failed at"), "{shown}");
        assert!(shown.contains(CAUSE_REF), "git's cause is lost: {shown}");
        match &err {
            Error::CreationFailed { source, .. } => match &**source {
                Error::Git { step, status, stderr } => {
                    assert_eq!(*step, "add worktree");
                    assert!(!status.success());
                    assert!(stderr.contains(CAUSE_REF), "{stderr}");
                }
                other => panic!("expected Git source, got {other:?}"),
            },
            other => panic!("expected CreationFailed, got {other:?}"),
        }
    }

    /// A non-collision Git failure whose message merely contains a path with
    /// the phrase "already exists" must fail at once with Git's cause, not be
    /// retried as a name race until the names run out.
    #[cfg(unix)]
    #[test]
    fn non_collision_failure_with_already_exists_in_path_is_not_retried() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, _wts, repo) = fixture();
        let wts = tmp.path().join("already exists");
        std::fs::create_dir(&wts).unwrap();
        std::fs::set_permissions(&wts, std::fs::Permissions::from_mode(0o555)).unwrap();
        let writable = std::fs::File::create(wts.join("probe")).is_ok();
        let started = std::time::Instant::now();
        let r = create_task_worktree(&repo, "ro-root", "HEAD", &wts);
        std::fs::set_permissions(&wts, std::fs::Permissions::from_mode(0o755)).unwrap();
        if writable {
            return; // permission bits are not enforced (e.g. running as root)
        }
        match r {
            Err(Error::CreationFailed { path, source, .. }) => {
                assert_eq!(path, wts.join("ro-root"));
                match *source {
                    Error::Git { stderr, .. } => {
                        assert!(stderr.contains(&*wts.to_string_lossy()), "{stderr}")
                    }
                    other => panic!("expected Git source, got {other:?}"),
                }
            }
            other => panic!("expected CreationFailed, got {other:?}"),
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "retried");
    }

    /// A real name collision (git's own "already exists") still retries.
    #[cfg(unix)]
    #[test]
    fn path_name_collision_retries_the_next_name() {
        let (_tmp, wts, repo) = fixture();
        std::fs::create_dir_all(&wts).unwrap();
        // A dangling symlink: `path.exists()` is false, git says "already exists".
        std::os::unix::fs::symlink(wts.join("missing-target"), wts.join("race")).unwrap();
        let wt = create_task_worktree(&repo, "race", "HEAD", &wts).unwrap();
        assert_eq!(wt.path(), wts.join("race-1"));
        assert_eq!(wt.branch(), "orchestrator/race-1");
    }

    #[test]
    fn name_collision_is_recognized_only_for_gits_own_conflict_lines() {
        let branch = "a branch named 'orchestrator/t' already exists";
        assert!(is_name_collision(&format!("Preparing worktree; fatal: {branch}")));
        assert!(is_name_collision("fatal: '/w/t' already exists"));
        assert!(!is_name_collision(
            "fatal: could not create leading directories of '/already exists/t/.git': denied"
        ));
        assert!(!is_name_collision("fatal: failed in /tmp/already exists"));
        assert!(!is_name_collision("already exists"));
    }

    #[test]
    fn long_stderr_keeps_the_final_cause_within_the_bound() {
        let noise = "é".repeat(2000);
        let stderr = format!("Preparing worktree\n{noise}\nfatal: the real cause\n");
        let shown = crate::error::stderr_detail(stderr.as_bytes());
        assert!(shown.chars().count() <= 1024, "{}", shown.chars().count());
        assert!(shown.ends_with("fatal: the real cause"), "{shown}");
        assert!(shown.starts_with("..."), "truncation must be visible: {shown}");
    }

    #[test]
    fn stderr_detail_neutralizes_terminal_control_characters() {
        let shown = crate::error::stderr_detail(b"fatal: a\x1b[31mb\x07\x00c\r\nsecond\tline\n");
        assert!(!shown.chars().any(char::is_control), "{shown:?}");
        assert!(shown.contains("fatal: a") && shown.contains("second"), "{shown:?}");
    }
}
