//! M3-04 acceptance contract for report-only and local-review delivery that the
//! existing `delivery.rs` tests do not pin: the exact approval binding, refusal
//! of a contender with the owner's lock untouched, destination state changing
//! while an approval check runs, and retention classification that follows the
//! current repository state. Everything runs against real temporary Git
//! repositories through the public API and uses no platform-specific code.

use orchestrator_git::delivery::{
    self, ApprovalCheck, CallbackError, DeliveryAction, DeliveryCallbacks, DeliveryError,
    DeliveryOutcome, DeliveryRequest, DeliveryStep, DestinationLockError, UnsafeToDeleteReason,
    VerificationCheck, WorktreeDeletionSafety,
};
use orchestrator_git::{create_task_worktree, Repository};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

const REVIEW_REF: &str = "refs/heads/orchestrator/review/task-1";
const REVIEW_PREFIX: &str = "refs/heads/orchestrator/review/";

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
    git(&repository_path, &["config", "core.autocrlf", "false"]);
    fs::write(repository_path.join("base.txt"), "base\n").expect("write base file");
    git(&repository_path, &["add", "base.txt"]);
    git(&repository_path, &["commit", "-m", "initial"]);
    let repository = Repository::new(&repository_path).expect("validated repository");
    let worktree = create_task_worktree(
        &repository,
        "task-1",
        "main",
        &directory.path().join("worktrees"),
    )
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

fn lock_path(fixture: &Fixture) -> PathBuf {
    fixture
        .repository
        .git_common_dir()
        .join(".orchestrator-delivery.lock")
}

fn review_refs(fixture: &Fixture) -> String {
    git(
        &fixture.repository_path,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            REVIEW_PREFIX,
        ],
    )
}

fn all_refs(fixture: &Fixture) -> String {
    git(
        &fixture.repository_path,
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )
}

#[derive(Debug, PartialEq, Eq)]
struct SeenApproval {
    action: DeliveryAction,
    task_id: String,
    approval_id: String,
    candidate_sha: String,
    parameter_digest: [u8; 32],
    destination: PathBuf,
    base_sha: String,
    target_ref: Option<String>,
}

#[test]
fn approval_and_verification_receive_exactly_the_requested_binding_for_each_action() {
    let fixture = fixture();
    let seen_approvals = RefCell::new(Vec::new());
    let seen_verifications = RefCell::new(Vec::new());
    let persisted = RefCell::new(Vec::new());
    let approval = |check: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
        seen_approvals.borrow_mut().push(SeenApproval {
            action: check.action,
            task_id: check.task_id.to_owned(),
            approval_id: check.approval_id.to_owned(),
            candidate_sha: check.candidate_sha.to_owned(),
            parameter_digest: *check.parameter_digest,
            destination: check.destination.to_path_buf(),
            base_sha: check.base_sha.to_owned(),
            target_ref: check.target_ref.map(str::to_owned),
        });
        Ok(())
    };
    let verification = |check: &VerificationCheck<'_>| -> Result<(), CallbackError> {
        seen_verifications.borrow_mut().push((
            check.task_id.to_owned(),
            check.run_id.to_owned(),
            check.candidate_sha.to_owned(),
        ));
        Ok(())
    };
    let persist = |step: &DeliveryStep| -> Result<(), CallbackError> {
        persisted.borrow_mut().push(step.clone());
        Ok(())
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &approval,
        check_verification: &verification,
        persist_step: &persist,
    };
    let mut bound = request(&fixture, "delivery-binding");
    bound.approval_id = "approval-exact";
    bound.verification_run_id = "verification-exact";
    bound.parameter_digest = [0x5a; 32];

    let report = delivery::report_only(&fixture.repository, bound, &callbacks)
        .expect("authorized report-only delivery");
    let review = delivery::local_review(&fixture.repository, bound, REVIEW_REF, &callbacks)
        .expect("authorized local-review delivery");

    let destination = fs::canonicalize(&fixture.repository_path).expect("canonical destination");
    let seen_approvals = seen_approvals.into_inner();
    assert_eq!(seen_approvals.len(), 2);
    for (seen, action, target_ref) in [
        (&seen_approvals[0], DeliveryAction::ReportOnly, None),
        (
            &seen_approvals[1],
            DeliveryAction::LocalReview,
            Some(REVIEW_REF),
        ),
    ] {
        assert_eq!(seen.action, action);
        assert_eq!(seen.task_id, "task-1");
        assert_eq!(seen.approval_id, "approval-exact");
        assert_eq!(seen.candidate_sha, fixture.candidate_sha);
        assert_eq!(seen.parameter_digest, [0x5a; 32]);
        assert_eq!(
            fs::canonicalize(&seen.destination).expect("canonical approval destination"),
            destination
        );
        assert_eq!(seen.base_sha, fixture.base_sha);
        assert_eq!(seen.target_ref.as_deref(), target_ref);
    }
    let expected_verification = (
        "task-1".to_owned(),
        "verification-exact".to_owned(),
        fixture.candidate_sha.clone(),
    );
    assert_eq!(
        seen_verifications.into_inner(),
        [expected_verification.clone(), expected_verification]
    );

    assert_eq!(report.outcome, DeliveryOutcome::Reported);
    assert_eq!(report.target_ref, None);
    assert_eq!(review.outcome, DeliveryOutcome::ReviewRefCreated);
    assert_eq!(review.target_ref.as_deref(), Some(REVIEW_REF));
    assert_eq!(persisted.into_inner(), [report.clone(), review.clone()]);
    for step in [&report, &review] {
        assert_eq!(step.approval_id, "approval-exact");
        assert_eq!(step.verification_run_id, "verification-exact");
        assert_eq!(step.parameter_digest, [0x5a; 32]);
        assert_eq!(step.candidate_sha, fixture.candidate_sha);
    }
    assert_eq!(
        review_refs(&fixture),
        format!("{REVIEW_REF} {}", fixture.candidate_sha)
    );
}

#[test]
fn contender_is_refused_and_recorded_while_owner_lock_and_destination_stay_intact() {
    let fixture = fixture();
    let lock = lock_path(&fixture);
    let repository_path = fixture.repository_path.clone();
    let worktree_path = fixture.worktree_path.clone();
    let base_sha = fixture.base_sha.clone();
    let candidate_sha = fixture.candidate_sha.clone();
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (resume_sender, resume_receiver) = mpsc::channel();
    let owner = thread::spawn(move || {
        let repository = Repository::new(&repository_path).expect("open owner repository");
        let request = DeliveryRequest {
            step_id: "delivery-owner",
            task_id: "task-1",
            candidate_sha: &candidate_sha,
            base_sha: &base_sha,
            approval_id: "approval-1",
            verification_run_id: "verification-1",
            parameter_digest: [7; 32],
            worktree_path: &worktree_path,
        };
        let hold = move |_: &VerificationCheck<'_>| -> Result<(), CallbackError> {
            entered_sender
                .send(())
                .map_err(|error| -> CallbackError { Box::new(error) })?;
            resume_receiver
                .recv_timeout(Duration::from_secs(10))
                .map_err(|error| -> CallbackError { Box::new(error) })?;
            Ok(())
        };
        let callbacks = DeliveryCallbacks {
            check_approval: &approval_ok,
            check_verification: &hold,
            persist_step: &persist_nothing,
        };
        delivery::local_review(&repository, request, REVIEW_REF, &callbacks)
    });
    entered_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("owner holds the destination lock");
    let owner_lock = fs::read(&lock).expect("read owner lock");
    assert!(
        String::from_utf8_lossy(&owner_lock).starts_with(&format!("pid={}", std::process::id())),
        "lock names its owner"
    );
    let before_refs = all_refs(&fixture);

    let persisted = RefCell::new(Vec::new());
    let persist = |step: &DeliveryStep| -> Result<(), CallbackError> {
        persisted.borrow_mut().push(step.clone());
        Ok(())
    };
    let contender_callbacks = DeliveryCallbacks {
        check_approval: &approval_ok,
        check_verification: &verification_ok,
        persist_step: &persist,
    };
    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-contender"),
        REVIEW_REF,
        &contender_callbacks,
    )
    .expect_err("a second delivery to the same destination must be refused");

    assert!(matches!(
        error,
        DeliveryError::DestinationLock(DestinationLockError::AlreadyHeld { .. })
    ));
    {
        let persisted = persisted.borrow();
        assert_eq!(persisted.len(), 1, "the refusal is recorded");
        assert_eq!(persisted[0].id, "delivery-contender");
        assert_eq!(persisted[0].outcome, DeliveryOutcome::Refused);
        assert!(persisted[0].detail.is_some());
    }
    assert_eq!(
        fs::read(&lock).expect("owner lock survives contention"),
        owner_lock
    );
    assert_eq!(all_refs(&fixture), before_refs, "contender wrote nothing");

    resume_sender.send(()).expect("release the owner");
    let owner_step = owner
        .join()
        .expect("join lock owner")
        .expect("owner delivery");
    assert_eq!(owner_step.outcome, DeliveryOutcome::ReviewRefCreated);
    assert!(!lock.exists(), "owner removes its own lock");

    let retry = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-contender-retry"),
        REVIEW_REF,
        &contender_callbacks,
    )
    .expect("the destination is available once the owner finishes");
    assert_eq!(retry.outcome, DeliveryOutcome::ReviewRefAlreadyMatches);
    assert_eq!(
        review_refs(&fixture),
        format!("{REVIEW_REF} {}", fixture.candidate_sha)
    );
    assert!(!lock.exists());
}

#[test]
fn foreign_lock_without_an_owner_pid_blocks_delivery_and_is_preserved() {
    let fixture = fixture();
    let lock = lock_path(&fixture);
    let foreign = b"written by another tool\n";
    fs::write(&lock, foreign).expect("create foreign lock");
    let before_refs = all_refs(&fixture);
    let calls = RefCell::new(0);
    let count = |_: &VerificationCheck<'_>| -> Result<(), CallbackError> {
        *calls.borrow_mut() += 1;
        Ok(())
    };
    let persisted = RefCell::new(Vec::new());
    let persist = |step: &DeliveryStep| -> Result<(), CallbackError> {
        persisted.borrow_mut().push(step.clone());
        Ok(())
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &approval_ok,
        check_verification: &count,
        persist_step: &persist,
    };

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-foreign-lock"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("a lock of unknown ownership must refuse delivery");

    assert!(matches!(
        error,
        DeliveryError::DestinationLock(DestinationLockError::HolderUnknown)
    ));
    assert_eq!(fs::read(&lock).expect("read retained lock"), foreign);
    assert_eq!(*calls.borrow(), 0, "no check runs without the lock");
    assert_eq!(persisted.borrow().len(), 1);
    assert_eq!(persisted.borrow()[0].outcome, DeliveryOutcome::Refused);
    assert_eq!(all_refs(&fixture), before_refs);
}

fn local_review_with_ref_created_during_approval(
    ref_commit_is_candidate: bool,
) -> (
    Fixture,
    Result<DeliveryStep, DeliveryError>,
    Vec<DeliveryStep>,
) {
    let fixture = fixture();
    let racing_commit = if ref_commit_is_candidate {
        fixture.candidate_sha.clone()
    } else {
        fixture.base_sha.clone()
    };
    let persisted = RefCell::new(Vec::new());
    let racing_writer = |_: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
        git(
            &fixture.repository_path,
            &["update-ref", REVIEW_REF, &racing_commit],
        );
        Ok(())
    };
    let persist = |step: &DeliveryStep| -> Result<(), CallbackError> {
        persisted.borrow_mut().push(step.clone());
        Ok(())
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &racing_writer,
        check_verification: &verification_ok,
        persist_step: &persist,
    };
    let result = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-racing-ref"),
        REVIEW_REF,
        &callbacks,
    );
    let persisted = persisted.into_inner();
    (fixture, result, persisted)
}

#[test]
fn review_ref_created_by_another_writer_during_approval_is_revalidated_not_overwritten() {
    let (fixture, result, persisted) = local_review_with_ref_created_during_approval(false);
    let error = result.expect_err("a foreign review ref must not be overwritten");
    assert!(matches!(error, DeliveryError::ReviewRefConflict { .. }));
    assert_eq!(
        git(&fixture.repository_path, &["rev-parse", REVIEW_REF]),
        fixture.base_sha
    );
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].outcome, DeliveryOutcome::Refused);
    assert!(!lock_path(&fixture).exists());

    let (fixture, result, persisted) = local_review_with_ref_created_during_approval(true);
    let step = result.expect("a ref already at the approved candidate is accepted");
    assert_eq!(step.outcome, DeliveryOutcome::ReviewRefAlreadyMatches);
    assert_eq!(
        review_refs(&fixture),
        format!("{REVIEW_REF} {}", fixture.candidate_sha)
    );
    assert_eq!(persisted, [step]);
    assert!(!lock_path(&fixture).exists());
}

#[test]
fn deletion_safety_follows_current_repository_state_and_never_deletes() {
    let fixture = fixture();
    let classify = |path: &Path, base_ref: &str| {
        delivery::check_worktree_safe_to_delete(&fixture.repository, path, base_ref)
    };

    match classify(&fixture.worktree_path, "main").expect("classify unlanded worktree") {
        WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::NotReachable { head, base_ref }) => {
            assert_eq!(head, fixture.candidate_sha);
            assert_eq!(base_ref, "main");
        }
        other => panic!("unlanded candidate must be retained: {other:?}"),
    }

    git(
        &fixture.repository_path,
        &["merge", "--ff-only", &fixture.candidate_sha],
    );
    assert_eq!(
        classify(&fixture.worktree_path, "main").expect("classify landed worktree"),
        WorktreeDeletionSafety::SafeToDelete
    );

    let tracked = fixture.worktree_path.join("candidate.txt");
    fs::write(&tracked, "edited after landing\n").expect("edit tracked file");
    match classify(&fixture.worktree_path, "main").expect("classify edited worktree") {
        WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::Dirty(status)) => {
            assert!(status.contains("candidate.txt"), "{status}");
        }
        other => panic!("tracked edit must retain the worktree: {other:?}"),
    }

    let nested = fixture.repository_path.join("nested");
    fs::create_dir(&nested).expect("create directory inside primary checkout");
    assert_eq!(
        classify(&nested, "main").expect("classify path inside primary checkout"),
        WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::PrimaryCheckout)
    );
    for base_ref in ["-main", "main\nother"] {
        assert!(
            matches!(
                classify(&fixture.worktree_path, base_ref),
                Err(DeliveryError::InvalidBaseRef(_))
            ),
            "{base_ref:?} must be rejected"
        );
    }

    assert_eq!(
        fs::read(&tracked).expect("read retained edit"),
        b"edited after landing\n"
    );
    assert!(fixture.worktree_path.join(".git").exists());
    assert_eq!(
        git(&fixture.worktree_path, &["rev-parse", "HEAD"]),
        fixture.candidate_sha
    );
    assert!(nested.exists() && fixture.repository_path.join("base.txt").exists());
}

#[test]
fn every_callback_runs_under_the_destination_lock_and_a_persistence_failure_keeps_the_written_ref()
{
    let fixture = fixture();
    let lock = lock_path(&fixture);
    let held = RefCell::new(Vec::new());
    let approval = |_: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
        held.borrow_mut().push(("approval", lock.exists()));
        Ok(())
    };
    let verification = |_: &VerificationCheck<'_>| -> Result<(), CallbackError> {
        held.borrow_mut().push(("verification", lock.exists()));
        Ok(())
    };
    let failing = |_: &DeliveryStep| -> Result<(), CallbackError> {
        held.borrow_mut().push(("persist", lock.exists()));
        Err("store unavailable".into())
    };
    let callbacks = DeliveryCallbacks {
        check_approval: &approval,
        check_verification: &verification,
        persist_step: &failing,
    };

    let error = delivery::local_review(
        &fixture.repository,
        request(&fixture, "delivery-persist-fails"),
        REVIEW_REF,
        &callbacks,
    )
    .expect_err("an unrecorded delivery must be surfaced");

    assert_eq!(
        review_refs(&fixture),
        format!("{REVIEW_REF} {}", fixture.candidate_sha),
        "the already written review ref is retained, never rolled back"
    );
    assert!(!lock.exists(), "the lock is released after the failure");
    assert_eq!(
        *held.borrow(),
        [
            ("verification", true),
            ("approval", true),
            ("persist", true)
        ]
    );
    assert!(matches!(error, DeliveryError::Persistence(_)));

    // Report-only has the same contract and still writes no ref.
    let refs_before = all_refs(&fixture);
    held.borrow_mut().clear();
    let error = delivery::report_only(
        &fixture.repository,
        request(&fixture, "delivery-report-persist-fails"),
        &callbacks,
    )
    .expect_err("an unrecorded report must be surfaced");
    assert_eq!(all_refs(&fixture), refs_before);
    assert!(!lock.exists());
    assert_eq!(
        *held.borrow(),
        [
            ("verification", true),
            ("approval", true),
            ("persist", true)
        ]
    );
    assert!(matches!(error, DeliveryError::Persistence(_)));
}
