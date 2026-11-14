use orchestrator_git::{Error, Repository};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) {
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
}

fn repository() -> (TempDir, PathBuf, Repository, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("repo");
    fs::create_dir_all(&path).unwrap();
    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "test@example.invalid"]);
    git(&path, &["config", "user.name", "Scoped Diff Test"]);
    fs::write(path.join("file.txt"), "before\n").unwrap();
    git(&path, &["add", "file.txt"]);
    git(&path, &["commit", "-m", "base"]);
    let repository = Repository::new(&path).unwrap();
    let base = repository.resolve_base_ref("HEAD").unwrap();
    fs::write(path.join("file.txt"), "after\n").unwrap();
    (directory, path, repository, base)
}

#[test]
fn diff_reads_scoped_changes_and_rejects_external_paths() {
    let (directory, _path, repository, base) = repository();
    let diff = repository
        .diff_from(&base, Some(Path::new("file.txt")))
        .unwrap();
    assert!(diff.contains("-before"));
    assert!(diff.contains("+after"));

    let outside = directory.path().join("outside.txt");
    assert!(matches!(
        repository.diff_from(&base, Some(&outside)),
        Err(Error::DiffPathOutsideWorktree)
    ));
    assert!(matches!(
        repository.diff_from(&base, Some(Path::new("../outside.txt"))),
        Err(Error::DiffPathOutsideWorktree)
    ));
}
