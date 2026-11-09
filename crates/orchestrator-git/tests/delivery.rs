use orchestrator_git::delivery::{
    self, ApprovalCheck, CallbackError, DeliveryAction, DeliveryCallbacks, DeliveryError,
    DeliveryOutcome, DeliveryRequest, DeliveryStep, DestinationLockError, UnsafeToDeleteReason,
    VerificationCheck, WorktreeDeletionSafety,
};
use orchestrator_git::{create_task_worktree, Repository};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

const REVIEW_REF: &str = "refs/heads/orchestrator/review/task-1";
const CANDIDATE_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct Fixture {
    _directory: TempDir,
    repository_path: PathBuf,
    worktree_path: PathBuf,
    repository: Repository,
    base_sha: String,
    candidate_sha: String,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temporary Git directory");
    let repository_path = directory.path().join("repository");
    fs::create_dir_all(&repository_path).expect("create repository");
    git(&repository_path, &["init", "-b", "main"]);
    git(
        &repository_path,
        &["config", "user.email", "test@example.invalid"],
    );
    git(&repository_path, &["config", "user.name", "Delivery Test"]);
    // The task worktree is created by production Git, which reads the machine
    // config (Git for Windows defaults to autocrlf=true and checks out CRLF),
    // while `git()` pins that config away. Pin it for both so they agree.
    git(&repository_path, &["config", "core.autocrlf", "false"]);
    fs::write(repository_path.join("base.txt"), "base\n").expect("write base file");
    git(&repository_path, &["add", "base.txt"]);
    git(&repository_path, &["commit", "-m", "initial"]);
    let repository = Repository::new(&repository_path).expect("validated repository");
    let worktrees_path = directory.path().join("worktrees");
    let worktree = create_task_worktree(&repository, "task-1", "main", &worktrees_path)
        .expect("create task worktree");
    let worktree_path = worktree.path().to_path_buf();
    let base_sha = worktree.base_sha().to_owned();
    fs::write(worktree_path.join("candidate.txt"), "candidate\n").expect("write candidate file");
    git(&worktree_path, &["add", "candidate.txt"]);
    git(&worktree_path, &["commit", "-m", "candidate"]);
    let candidate_sha = git(&worktree_path, &["rev-parse", "HEAD"]);
    Fixture {
        _directory: directory,
        repository_path,
        worktree_path,
        repository,
        base_sha,
        candidate_sha,
    }
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
        .args(args)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn request<'a>(fixture: &'a Fixture, step_id: &'a str) -> DeliveryRequest<'a> {
    DeliveryRequest {
        step_id,
        task_id: "task-1",
        candidate_sha: &fixture.candidate_sha,
        base_sha: &fixture.base_sha,
        approval_id: "approval-1",
        verification_run_id: "verification-1",
        parameter_digest: [7; 32],
        worktree_path: &fixture.worktree_path,
    }
}

fn approval_ok(_: &ApprovalCheck<'_>) -> Result<(), CallbackError> {
    Ok(())
}

fn verification_ok(_: &VerificationCheck<'_>) -> Result<(), CallbackError> {
    Ok(())
}

fn persist_nothing(_: &DeliveryStep) -> Result<(), CallbackError> {
    Ok(())
}

fn callbacks<'a>(
    persist_step: &'a dyn Fn(&DeliveryStep) -> Result<(), CallbackError>,
) -> DeliveryCallbacks<'a> {
    DeliveryCallbacks {
        check_approval: &approval_ok,
        check_verification: &verification_ok,
        persist_step,
    }
}

#[test]
fn report_only_preserves_refs_worktrees_and_repository_files() {
    let fixture = fixture();
    let before_refs = refs(&fixture.repository_path);
    let before_repository_status = status(&fixture.repository_path);
    let before_worktree_status = status(&fixture.worktree_path);
    let before_files = file_hashes(&fixture.repository_path, &fixture.worktree_path);
    let persisted = RefCell::new(Vec::new());
    let persist = |step: &DeliveryStep| {
        persisted.borrow_mut().push(step.clone());
        Ok(())
    };
    let callbacks = callbacks(&persist);

    let report = delivery::report_only(
        &fixture.repository,
        request(&fixture, "delivery-report"),
        &callbacks,
    )
    .expect("report verified candidate");

    assert_eq!(report.outcome, DeliveryOutcome::Reported);
    assert_eq!(report.task_id, "task-1");
    assert_eq!(report.candidate_sha, fixture.candidate_sha);
    assert_eq!(report.approval_id, "approval-1");
    assert_eq!(report.verification_run_id, "verification-1");
    assert!(report
        .diff_summary
        .as_deref()
        .unwrap()
        .contains("candidate.txt"));
    assert_eq!(persisted.borrow().as_slice(), &[report]);
    assert_eq!(refs(&fixture.repository_path), before_refs);
    assert_eq!(status(&fixture.repository_path), before_repository_status);
    assert_eq!(status(&fixture.worktree_path), before_worktree_status);
    assert_eq!(
        file_hashes(&fixture.repository_path, &fixture.worktree_path),
        before_files
    );
}

fn refs(repository_path: &Path) -> String {
    git(
        repository_path,
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )
}

fn status(path: &Path) -> String {
    git(path, &["status", "--porcelain", "--untracked-files=all"])
}

fn file_hashes(repository_path: &Path, worktree_path: &Path) -> BTreeMap<PathBuf, u64> {
    let mut hashes = BTreeMap::new();
    hash_tree(repository_path, &mut hashes);
    hash_tree(worktree_path, &mut hashes);
    hashes
}

fn hash_tree(path: &Path, hashes: &mut BTreeMap<PathBuf, u64>) {
    for entry in fs::read_dir(path).expect("list repository files") {
        let entry = entry.expect("read repository entry");
        let file_type = entry.file_type().expect("read entry type");
        if file_type.is_dir() {
            hash_tree(&entry.path(), hashes);
        } else {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            if file_type.is_symlink() {
                fs::read_link(entry.path())
                    .expect("read symlink target")
                    .hash(&mut hasher);
            } else {
                fs::read(entry.path())
                    .expect("read repository file")
                    .hash(&mut hasher);
            }
            hashes.insert(entry.path(), hasher.finish());
        }
    }
}

#[test]
fn local_review_creates_only_review_ref() {
    let fixture = fixture();
    let main_before = git(&fixture.repository_path, &["rev-parse", "refs/heads/main"]);
    let before_repository_status = status(&fixture.repository_path);
    let before_worktree_status = status(&fixture.worktree_path);
    let callbacks = callbacks(&persist_nothing);

    let step = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-review"),
        REVIEW_REF,
        &callbacks,
    )
    .expect("create local-review ref");

    assert_eq!(step.outcome, DeliveryOutcome::ReviewRefCreated);
    assert_eq!(
        git(&fixture.repository_path, &["rev-parse", REVIEW_REF]),
        fixture.candidate_sha
    );
    assert_eq!(
        git(&fixture.repository_path, &["rev-parse", "refs/heads/main"]),
        main_before
    );
    assert_eq!(
        git(&fixture.repository_path, &["symbolic-ref", "HEAD"]),
        "refs/heads/main"
    );
    assert_eq!(status(&fixture.repository_path), before_repository_status);
    assert_eq!(status(&fixture.worktree_path), before_worktree_status);
    assert_eq!(
        fs::read(fixture.repository_path.join("base.txt")).unwrap(),
        b"base\n"
    );
    assert!(!fixture.repository_path.join("candidate.txt").exists());
    assert_eq!(
        fs::read(fixture.worktree_path.join("candidate.txt")).unwrap(),
        b"candidate\n"
    );
}

#[test]
fn local_review_refuses_existing_ref_with_different_commit() {
    let fixture = fixture();
    git(
        &fixture.repository_path,
        &["update-ref", REVIEW_REF, &fixture.base_sha],
    );
    let before_refs = refs(&fixture.repository_path);
    let callbacks = callbacks(&persist_nothing);

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-conflict"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("ref with different content must be refused");

    assert!(matches!(error, DeliveryError::ReviewRefConflict { .. }));
    assert_eq!(refs(&fixture.repository_path), before_refs);
    assert_eq!(
        git(&fixture.repository_path, &["rev-parse", REVIEW_REF]),
        fixture.base_sha
    );
}

#[test]
fn local_review_accepts_existing_ref_with_same_candidate() {
    let fixture = fixture();
    git(
        &fixture.repository_path,
        &["update-ref", REVIEW_REF, &fixture.candidate_sha],
    );
    let before_refs = refs(&fixture.repository_path);
    let callbacks = callbacks(&persist_nothing);

    let step = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-idempotent"),
        REVIEW_REF,
        &callbacks,
    )
    .expect("same candidate ref is already delivered");

    assert_eq!(step.outcome, DeliveryOutcome::ReviewRefAlreadyMatches);
    assert_eq!(refs(&fixture.repository_path), before_refs);
}

#[test]
fn local_review_refuses_missing_approval() {
    let fixture = fixture();
    let before_refs = refs(&fixture.repository_path);
    let missing = |_: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
        Err(Box::new(io::Error::new(
            io::ErrorKind::NotFound,
            "approval was not found",
        )))
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &missing,
        check_verification: &verification_ok,
        persist_step: &persist_nothing,
    };

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-no-approval"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("missing approval must refuse before ref creation");

    assert!(matches!(error, DeliveryError::Approval(_)));
    assert_eq!(refs(&fixture.repository_path), before_refs);
}

#[test]
fn local_review_refuses_changed_parameter_digest() {
    let fixture = fixture();
    let before_refs = refs(&fixture.repository_path);
    let approved_digest = [8; 32];
    let changed = |check: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
        assert_eq!(check.action, DeliveryAction::LocalReview);
        assert_eq!(check.task_id, "task-1");
        assert_eq!(check.target_ref, Some(REVIEW_REF));
        if check.parameter_digest != &approved_digest {
            Err(Box::new(io::Error::other(
                "approved parameter digest differs",
            )))
        } else {
            Ok(())
        }
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &changed,
        check_verification: &verification_ok,
        persist_step: &persist_nothing,
    };

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-changed-parameters"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("changed parameter digest must refuse");

    assert!(matches!(error, DeliveryError::Approval(_)));
    assert_eq!(refs(&fixture.repository_path), before_refs);
}

#[test]
fn local_review_refuses_changed_approved_candidate_sha() {
    let fixture = fixture();
    let before_refs = refs(&fixture.repository_path);
    let changed = |check: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
        assert_eq!(check.candidate_sha, fixture.candidate_sha);
        assert_ne!(check.candidate_sha, CANDIDATE_SHA);
        Err(Box::new(io::Error::other(
            "approved candidate commit differs",
        )))
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &changed,
        check_verification: &verification_ok,
        persist_step: &persist_nothing,
    };

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-changed-approved-candidate"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("changed approved candidate must refuse");

    assert!(matches!(error, DeliveryError::Approval(_)));
    assert_eq!(refs(&fixture.repository_path), before_refs);
}

#[test]
fn local_review_refuses_when_worktree_candidate_sha_changed() {
    let fixture = fixture();
    let before_refs = refs(&fixture.repository_path);
    let mut request = request(&fixture, "delivery-changed-candidate");
    request.candidate_sha = CANDIDATE_SHA;
    let callbacks = callbacks(&persist_nothing);

    let error = delivery::local_review(&fixture.repository, request, REVIEW_REF, &callbacks)
        .expect_err("changed current candidate must refuse");

    assert!(matches!(error, DeliveryError::CandidateChanged { .. }));
    assert_eq!(refs(&fixture.repository_path), before_refs);
}

#[test]
fn local_review_refuses_stale_verification_run() {
    let fixture = fixture();
    let before_refs = refs(&fixture.repository_path);
    let mismatch = |check: &VerificationCheck<'_>| -> Result<(), CallbackError> {
        assert_eq!(check.task_id, "task-1");
        assert_eq!(check.run_id, "verification-1");
        assert_eq!(check.candidate_sha, fixture.candidate_sha);
        Err(Box::new(io::Error::other(
            "verification run no longer matches candidate",
        )))
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &approval_ok,
        check_verification: &mismatch,
        persist_step: &persist_nothing,
    };

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-stale-verification"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("stale verification run must refuse");

    assert!(matches!(error, DeliveryError::Verification(_)));
    assert_eq!(refs(&fixture.repository_path), before_refs);
}

#[test]
fn destination_lock_refuses_second_concurrent_holder() {
    let fixture = fixture();
    let lock_path = fixture
        .repository
        .git_common_dir()
        .join(".orchestrator-delivery.lock");
    let repository_path = fixture.repository_path.clone();
    let worktree_path = fixture.worktree_path.clone();
    let base_sha = fixture.base_sha.clone();
    let candidate_sha = fixture.candidate_sha.clone();
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (resume_sender, resume_receiver) = mpsc::channel();
    let owner = thread::spawn(move || {
        let repository = Repository::new(&repository_path).expect("open owner repository");
        let step_id = "delivery-lock-owner";
        let worktree_path = worktree_path.as_path();
        let request = DeliveryRequest {
            step_id,
            task_id: "task-1",
            candidate_sha: &candidate_sha,
            base_sha: &base_sha,
            approval_id: "approval-1",
            verification_run_id: "verification-1",
            parameter_digest: [7; 32],
            worktree_path,
        };
        let wait_for_release = move |_: &VerificationCheck<'_>| -> Result<(), CallbackError> {
            entered_sender
                .send(())
                .map_err(|error| -> CallbackError { Box::new(error) })?;
            resume_receiver
                .recv_timeout(Duration::from_secs(5))
                .map_err(|error| -> CallbackError { Box::new(error) })?;
            Ok(())
        };
        let callbacks = DeliveryCallbacks {
            check_approval: &approval_ok,
            check_verification: &wait_for_release,
            persist_step: &persist_nothing,
        };
        delivery::report_only(&repository, request, &callbacks)
    });
    entered_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("owner holds destination lock");
    assert!(
        lock_path.exists(),
        "contender must leave owner lock untouched"
    );

    let callbacks = callbacks(&persist_nothing);
    let error = delivery::report_only(
        &fixture.repository,
        request(&fixture, "delivery-lock-contender"),
        &callbacks,
    )
    .expect_err("second lock holder must be refused");
    assert!(matches!(
        error,
        DeliveryError::DestinationLock(DestinationLockError::AlreadyHeld { .. })
    ));

    resume_sender.send(()).expect("release destination lock");
    let owner_step = owner
        .join()
        .expect("join lock owner")
        .expect("owner delivery");
    assert!(!lock_path.exists(), "owner removes only its own lock");
    assert_eq!(owner_step.outcome, DeliveryOutcome::Reported);
}

#[test]
fn stale_destination_lock_is_reported_and_preserved() {
    let fixture = fixture();
    let lock_path = fixture
        .repository
        .git_common_dir()
        .join(".orchestrator-delivery.lock");
    let stale_contents = b"pid=2000000000\nowner=stale-fixture\n";
    fs::write(&lock_path, stale_contents).expect("create stale lock");
    let callbacks = callbacks(&persist_nothing);

    let error = delivery::report_only(
        &fixture.repository,
        request(&fixture, "delivery-stale-lock"),
        &callbacks,
    )
    .expect_err("stale lock must refuse delivery");

    assert!(matches!(
        error,
        DeliveryError::DestinationLock(DestinationLockError::StaleLock { .. })
    ));
    assert_eq!(
        fs::read(&lock_path).expect("read retained lock"),
        stale_contents
    );
}

#[test]
fn dirty_worktree_is_unsafe_and_survives_safety_check() {
    let fixture = fixture();
    let dirty_file = fixture.worktree_path.join("uncommitted.txt");
    fs::write(&dirty_file, "keep\n").expect("create uncommitted change");
    let before_status = status(&fixture.worktree_path);
    let before_head = git(&fixture.worktree_path, &["rev-parse", "HEAD"]);
    let before_worktrees = git(
        &fixture.repository_path,
        &["worktree", "list", "--porcelain"],
    );

    let result = delivery::check_worktree_safe_to_delete(
        &fixture.repository,
        &fixture.worktree_path,
        "main",
    )
    .expect("classify dirty worktree");

    assert!(matches!(
        result,
        WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::Dirty(_))
    ));
    assert!(fixture.worktree_path.exists());
    assert_eq!(fs::read(&dirty_file).unwrap(), b"keep\n");
    assert_eq!(status(&fixture.worktree_path), before_status);
    assert_eq!(
        git(&fixture.worktree_path, &["rev-parse", "HEAD"]),
        before_head
    );
    assert_eq!(
        git(
            &fixture.repository_path,
            &["worktree", "list", "--porcelain"]
        ),
        before_worktrees
    );
}

#[test]
fn unlanded_worktree_is_unsafe_and_survives_safety_check() {
    let fixture = fixture();
    let before_status = status(&fixture.worktree_path);
    let before_head = git(&fixture.worktree_path, &["rev-parse", "HEAD"]);
    let before_refs = refs(&fixture.repository_path);
    let before_worktrees = git(
        &fixture.repository_path,
        &["worktree", "list", "--porcelain"],
    );

    let result = delivery::check_worktree_safe_to_delete(
        &fixture.repository,
        &fixture.worktree_path,
        "main",
    )
    .expect("classify unlanded worktree");

    assert!(matches!(
        result,
        WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::NotReachable { .. })
    ));
    assert!(fixture.worktree_path.exists());
    assert_eq!(status(&fixture.worktree_path), before_status);
    assert_eq!(
        git(&fixture.worktree_path, &["rev-parse", "HEAD"]),
        before_head
    );
    assert_eq!(refs(&fixture.repository_path), before_refs);
    assert_eq!(
        git(
            &fixture.repository_path,
            &["worktree", "list", "--porcelain"]
        ),
        before_worktrees
    );
}
