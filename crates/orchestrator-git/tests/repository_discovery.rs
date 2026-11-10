//! `Repository::new` resolves the work-tree root and the shared Git directory
//! the same way from the root, from a subdirectory and from a linked worktree.

use orchestrator_git::{Error, Repository};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

#[test]
fn discovery_agrees_from_root_subdirectory_and_linked_worktree() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("repo");
    let linked = directory.path().join("linked");
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("sub/deeper/file.txt"), "x\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().unwrap(),
        ],
    );
    fs::create_dir_all(linked.join("sub")).unwrap();

    let common = canonical(&root.join(".git"));
    for (start, toplevel) in [
        (root.clone(), &root),
        (root.join("sub/deeper"), &root),
        (linked.clone(), &linked),
        (linked.join("sub"), &linked),
    ] {
        let repository = Repository::new(&start).unwrap();
        assert_eq!(
            canonical(repository.path()),
            canonical(toplevel),
            "{start:?}"
        );
        assert_eq!(canonical(repository.git_common_dir()), common, "{start:?}");
    }
}

#[test]
fn a_directory_outside_any_repository_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    assert!(matches!(
        Repository::new(directory.path()),
        Err(Error::NotARepository { .. })
    ));
}
