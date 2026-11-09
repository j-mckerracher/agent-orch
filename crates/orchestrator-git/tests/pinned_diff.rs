use orchestrator_git::{Error, Repository};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit(path: &Path, name: &str, contents: &str) -> String {
    fs::write(path.join(name), contents).unwrap();
    git(path, &["add", "--", name]);
    git(path, &["commit", "-m", name]);
    git(path, &["rev-parse", "HEAD"])
}

fn repository() -> (TempDir, PathBuf, Repository, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("repo");
    fs::create_dir_all(&path).unwrap();
    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "test@example.invalid"]);
    git(&path, &["config", "user.name", "Pinned Diff Test"]);
    let base = commit(&path, "base.txt", "base\n");
    (
        directory,
        path.clone(),
        Repository::new(&path).unwrap(),
        base,
    )
}

#[test]
fn pinned_diff_ignores_later_commits_and_work_tree_edits() {
    let (_directory, path, repository, base) = repository();
    let candidate = commit(&path, "a.txt", "candidate\n");
    commit(&path, "b.txt", "later\n");
    fs::write(path.join("a.txt"), "dirty edit\n").unwrap();

    let files = repository.changed_files_between(&base, &candidate).unwrap();
    assert_eq!(files, ["a.txt"]);
    let diff = repository.diff_between(&base, &candidate, None).unwrap();
    assert!(diff.contains("+candidate"));
    assert!(!diff.contains("later") && !diff.contains("dirty edit"));
    let scoped = repository
        .diff_between(&base, &candidate, Some(Path::new("a.txt")))
        .unwrap();
    assert_eq!(scoped, diff);
    assert_eq!(
        repository
            .diff_between(&base, &candidate, Some(Path::new("base.txt")))
            .unwrap(),
        ""
    );
}

#[test]
fn changed_files_are_literal_unquoted_and_not_pathspecs() {
    let (_directory, path, repository, base) = repository();
    commit(&path, "has space.txt", "1\n");
    commit(&path, "ünï.txt", "2\n");
    let candidate = commit(&path, "[x].txt", "3\n");
    let files = repository.changed_files_between(&base, &candidate).unwrap();
    assert_eq!(files, ["[x].txt", "has space.txt", "ünï.txt"]);
    assert_eq!(
        repository
            .diff_between(&base, &candidate, Some(Path::new("*.txt")))
            .unwrap(),
        "",
        "glob characters are literal"
    );
    assert!(repository
        .diff_between(&base, &candidate, Some(Path::new("[x].txt")))
        .unwrap()
        .contains("+3"));
}

#[test]
fn pinned_diff_rejects_unsafe_ids_and_paths() {
    let (directory, path, repository, base) = repository();
    let candidate = commit(&path, "a.txt", "candidate\n");
    assert!(matches!(
        repository.changed_files_between("main", &candidate),
        Err(Error::InvalidDiffBase)
    ));
    assert!(matches!(
        repository.changed_files_between(&base, "HEAD"),
        Err(Error::InvalidDiffCandidate)
    ));
    assert!(matches!(
        repository.diff_between(&base, "--output=x", None),
        Err(Error::InvalidDiffCandidate)
    ));
    assert!(matches!(
        repository.diff_between(&base, &candidate[..12], None),
        Err(Error::InvalidDiffCandidate)
    ));
    assert!(matches!(
        repository.diff_between(&base, &"b".repeat(40), None),
        Err(Error::InvalidDiffCandidate) | Err(Error::InvalidBaseRef { .. })
    ));
    assert!(matches!(
        repository.diff_between(&base, &candidate, Some(Path::new(""))),
        Err(Error::InvalidDiffPath)
    ));
    assert!(matches!(
        repository.diff_between(&base, &candidate, Some(Path::new("../x"))),
        Err(Error::DiffPathOutsideWorktree)
    ));
    assert!(matches!(
        repository.diff_between(
            &base,
            &candidate,
            Some(&directory.path().join("outside.txt"))
        ),
        Err(Error::DiffPathOutsideWorktree)
    ));
}

#[test]
fn diff_between_treats_the_path_as_a_path_not_an_option_or_revision() {
    let (_directory, path, repository, base) = repository();
    commit(&path, "--stat", "dash\n");
    let candidate = commit(&path, "main", "named like a branch\n");
    let dashed = repository
        .diff_between(&base, &candidate, Some(Path::new("--stat")))
        .unwrap();
    assert!(dashed.contains("diff --git") && dashed.contains("+dash"));
    let branch_named = repository
        .diff_between(&base, &candidate, Some(Path::new("main")))
        .unwrap();
    assert!(branch_named.contains("+named like a branch"));
    assert!(!branch_named.contains("+dash"));
}

#[cfg(unix)]
mod drivers {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(directory: &Path, marker: &Path) -> PathBuf {
        let script = directory.join("driver.sh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\necho ran >> '{}'\ncat \"$1\"\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    /// Both drivers are repository-local configuration, so a candidate that
    /// carries them must still never get a helper executed by the daemon.
    fn assert_helper_never_runs(configure: impl Fn(&Path, &Path)) {
        let (directory, path, repository, base) = repository();
        let marker = directory.path().join("marker");
        let script = script(directory.path(), &marker);
        configure(&path, &script);
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "--allow-empty", "-m", "drivers"]);
        let candidate = commit(&path, "base.txt", "changed\n");
        fs::write(path.join("base.txt"), "worktree edit\n").unwrap();

        let diff = repository.diff_between(&base, &candidate, None).unwrap();
        assert!(diff.contains("+changed"), "{diff}");
        repository.diff_from(&base, None).unwrap();
        repository.changed_files_between(&base, &candidate).unwrap();
        assert!(!marker.exists(), "a configured diff helper was executed");

        // Control: the same configuration does run a helper when asked to.
        git(&path, &["diff", &base, "HEAD"]);
        assert!(marker.exists(), "the helper is not wired up in this test");
    }

    #[test]
    fn configured_external_diff_is_never_executed() {
        assert_helper_never_runs(|path, script| {
            git(
                path,
                &["config", "diff.external", &script.display().to_string()],
            );
        });
    }

    #[test]
    fn configured_textconv_driver_is_never_executed() {
        assert_helper_never_runs(|path, script| {
            fs::write(path.join(".gitattributes"), "*.txt diff=probe\n").unwrap();
            git(
                path,
                &[
                    "config",
                    "diff.probe.textconv",
                    &script.display().to_string(),
                ],
            );
        });
    }
}
