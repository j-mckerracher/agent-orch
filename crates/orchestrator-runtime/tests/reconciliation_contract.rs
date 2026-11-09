//! M2-04 contract tests: real process observations flow through
//! `reconcile_processes` into the `scheduling_may_resume` gate.
//!
//! Existing `tests/reconcile.rs` covers pure classification, exact-owner
//! observation, and the gate over synthetic records. These tests close the
//! remaining seams: a live process resolves the gate, a forged start time
//! blocks it, and (Unix process groups, including macOS) a group whose leader
//! died but whose descendant lives is never reported as safely exited.
//!
//! Windows Job Object cleanup is supervisor behaviour, covered by the
//! agents/CLI cancel tests; the Windows-capable tests here use only
//! cross-platform identity APIs.

use orchestrator_core::identity::{ProcessAction, ProcessClassification};
use orchestrator_runtime::reconcile::{
    capture_process_identity, reconcile_processes, scheduling_may_resume, ProcessSubject,
    RecordedProcess,
};

fn subject() -> ProcessSubject {
    ProcessSubject::Session {
        task_id: "task".into(),
        session_id: "session".into(),
    }
}

#[test]
fn live_process_is_left_running_and_does_not_block_the_gate() {
    let identity = capture_process_identity(std::process::id()).expect("own identity");
    let plan = reconcile_processes(&[RecordedProcess {
        subject: subject(),
        identity,
    }]);

    assert_eq!(plan[0].classification, ProcessClassification::Alive);
    assert_eq!(plan[0].action, ProcessAction::LeaveRunning);
    assert!(scheduling_may_resume(&plan, &[]));
}

#[test]
fn forged_start_time_needs_review_and_blocks_the_gate() {
    let mut identity = capture_process_identity(std::process::id()).expect("own identity");
    let start = identity.start_time_micros.expect("start time captured");
    // Same PID, different start time: a reused PID must never be adopted.
    identity.start_time_micros = Some(start.saturating_sub(5_000_000).max(1));
    assert_ne!(identity.start_time_micros, Some(start));

    let plan = reconcile_processes(&[RecordedProcess {
        subject: subject(),
        identity,
    }]);

    assert_eq!(
        plan[0].classification,
        ProcessClassification::IdentityMismatch
    );
    assert_eq!(plan[0].action, ProcessAction::MarkNeedsReview);
    assert!(!scheduling_may_resume(&plan, &[]));
}

#[cfg(unix)]
mod process_group {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use orchestrator_runtime::reconcile::capture_exact_process_identity;
    use orchestrator_runtime::supervisor::process_group_alive;

    /// Kills the test-owned group (a group whose leader this test spawned
    /// with `process_group(0)`) if an assertion fails before cleanup.
    struct OwnedGroup {
        pgid: i32,
        reaped: bool,
    }

    impl Drop for OwnedGroup {
        fn drop(&mut self) {
            if !self.reaped {
                unsafe { libc::kill(-self.pgid, libc::SIGKILL) };
            }
        }
    }

    #[test]
    fn dead_group_leader_with_live_descendant_blocks_gate_until_group_is_gone() {
        let mut leader = Command::new("/bin/sh")
            .args(["-c", "sleep 60 & echo $!; wait"])
            .process_group(0)
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn group leader");
        let pgid = leader.id() as i32;
        let mut guard = OwnedGroup {
            pgid,
            reaped: false,
        };
        let mut descendant_line = String::new();
        BufReader::new(leader.stdout.take().expect("leader stdout"))
            .read_line(&mut descendant_line)
            .expect("descendant pid line");
        let descendant: i32 = descendant_line.trim().parse().expect("descendant pid");
        assert_ne!(descendant, pgid);

        let identity = capture_exact_process_identity(leader.id())
            .expect("leader identity")
            .identity;
        assert_eq!(identity.process_group, Some(leader.id()));

        // Kill only the leader; the descendant keeps the group alive.
        assert_eq!(unsafe { libc::kill(pgid, libc::SIGKILL) }, 0);
        leader.wait().expect("reap leader");
        assert!(process_group_alive(pgid as u32), "descendant keeps group");

        let record = [RecordedProcess {
            subject: subject(),
            identity,
        }];
        let blocked = reconcile_processes(&record);
        assert_eq!(blocked[0].classification, ProcessClassification::Unknown);
        assert_eq!(blocked[0].action, ProcessAction::MarkNeedsReview);
        assert!(
            !scheduling_may_resume(&blocked, &[]),
            "a surviving descendant must block scheduling"
        );

        // Remove the surviving tree; only then may the record be interrupted.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
        let deadline = Instant::now() + Duration::from_secs(10);
        while process_group_alive(pgid as u32) {
            assert!(
                Instant::now() < deadline,
                "group did not exit after SIGKILL"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        guard.reaped = true;

        let resolved = reconcile_processes(&record);
        assert_eq!(resolved[0].classification, ProcessClassification::Exited);
        assert_eq!(resolved[0].action, ProcessAction::MarkInterrupted);
        assert!(scheduling_may_resume(&resolved, &[]));
    }
}

/// Real Git worktrees and delivery refs flow through the library converters
/// and classifier into the scheduling gate.
///
/// RED on base 46497ac (no `workspace_evidence`, `delivery_evidence`,
/// `classify_interrupted_task`, or runtime->git dependency); GREEN once the
/// M2-04 owner candidate provides them. Platform-neutral: real `git` only.
mod git_state {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use orchestrator_core::identity::ProcessClassification;
    use orchestrator_core::task::{AttemptState, TaskState};
    use orchestrator_git::delivery::{check_worktree_safe_to_delete, DeliveryAction};
    use orchestrator_git::delivery_reconcile::{reconcile_delivery_steps, RecordedDeliveryStep};
    use orchestrator_git::reconcile::{reconcile_workspaces, RecordedWorkspace};
    use orchestrator_git::{create_task_worktree, Repository, TaskWorktree};
    use orchestrator_runtime::reconcile::{
        classify_interrupted_task, delivery_evidence, scheduling_may_resume, workspace_evidence,
        DeliveryEvidence, TaskEvidence, TaskRecovery, WorkspaceEvidence,
    };

    struct Fixture {
        _directory: tempfile::TempDir,
        path: PathBuf,
        worktrees: PathBuf,
        repository: Repository,
        candidate_sha: String,
    }

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(directory)
            .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
            .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().expect("temporary git fixture");
        let path = directory.path().join("repository");
        fs::create_dir_all(&path).expect("create repository");
        git(&path, &["init", "-b", "main"]);
        git(&path, &["config", "user.email", "test@example.invalid"]);
        git(&path, &["config", "user.name", "Test"]);
        fs::write(path.join("base.txt"), "base\n").expect("write base");
        git(&path, &["add", "base.txt"]);
        git(&path, &["commit", "-m", "base"]);
        let candidate_sha = git(&path, &["rev-parse", "HEAD"]);
        let repository = Repository::new(&path).expect("validated repository");
        Fixture {
            worktrees: directory.path().join("worktrees"),
            _directory: directory,
            path,
            repository,
            candidate_sha,
        }
    }

    fn state_snapshot(fixture: &Fixture) -> String {
        format!(
            "{}\n{}",
            git(&fixture.path, &["for-each-ref"]),
            git(&fixture.path, &["worktree", "list", "--porcelain"])
        )
    }

    /// Mirrors what a recovery caller does for one task's worktree.
    fn observe_workspace(
        fixture: &Fixture,
        worktree: &TaskWorktree,
        inspect_safety: bool,
    ) -> WorkspaceEvidence {
        let recorded = [RecordedWorkspace {
            id: "workspace-1".into(),
            path: worktree.path().to_path_buf(),
            branch: worktree.branch().to_owned(),
            base_commit: worktree.base_sha().to_owned(),
        }];
        let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("workspace plan");
        let safety = (inspect_safety && worktree.path().exists()).then(|| {
            check_worktree_safe_to_delete(&fixture.repository, worktree.path(), "main")
                .expect("worktree safety")
        });
        workspace_evidence(plan.workspaces.first(), safety.as_ref())
    }

    fn evidence(
        workspace: WorkspaceEvidence,
        delivery: DeliveryEvidence,
        task: TaskState,
        attempt: AttemptState,
    ) -> TaskEvidence {
        TaskEvidence {
            task,
            attempt,
            process: ProcessClassification::Exited,
            workspace,
            delivery,
        }
    }

    fn step(fixture: &Fixture, id: &str, target_ref: &str, sha: &str) -> RecordedDeliveryStep {
        RecordedDeliveryStep {
            id: id.into(),
            task_id: Some("task-1".into()),
            action: Some(DeliveryAction::LocalReview),
            candidate_sha: Some(sha.into()),
            destination: Some(fixture.repository.path().to_path_buf()),
            target_ref: Some(target_ref.into()),
        }
    }

    fn gate(recovery: TaskRecovery) -> bool {
        scheduling_may_resume(&[], &[recovery.resolution()])
    }

    #[test]
    fn clean_matching_worktree_is_recoverable_and_dirty_work_is_preserved() {
        let fixture = fixture();
        let worktree =
            create_task_worktree(&fixture.repository, "task-1", "HEAD", &fixture.worktrees)
                .expect("create worktree");

        let clean = observe_workspace(&fixture, &worktree, true);
        assert_eq!(clean, WorkspaceEvidence::Matching);
        let recovery = classify_interrupted_task(evidence(
            clean,
            DeliveryEvidence::None,
            TaskState::Running,
            AttemptState::Running,
        ));
        assert_eq!(recovery, TaskRecovery::Recoverable);
        assert!(gate(recovery));

        // Uncommitted work: never discardable, a human must inspect.
        let dirty_file = worktree.path().join("unsaved.txt");
        fs::write(&dirty_file, "uncommitted\n").expect("write dirty file");
        let dirty = observe_workspace(&fixture, &worktree, true);
        assert_eq!(dirty, WorkspaceEvidence::Dirty);
        let recovery = classify_interrupted_task(evidence(
            dirty,
            DeliveryEvidence::None,
            TaskState::Running,
            AttemptState::Running,
        ));
        assert_eq!(recovery, TaskRecovery::RequiresInspection);
        assert!(!gate(recovery), "dirty work must block scheduling");
        assert_eq!(fs::read_to_string(&dirty_file).unwrap(), "uncommitted\n");
    }

    #[test]
    fn committed_unlanded_work_is_never_treated_as_matching_or_discardable() {
        let fixture = fixture();
        let worktree =
            create_task_worktree(&fixture.repository, "task-1", "HEAD", &fixture.worktrees)
                .expect("create worktree");
        fs::write(worktree.path().join("work.txt"), "work\n").expect("write work");
        git(worktree.path(), &["add", "work.txt"]);
        git(worktree.path(), &["commit", "-m", "unlanded work"]);
        let head = git(worktree.path(), &["rev-parse", "HEAD"]);

        let observed = observe_workspace(&fixture, &worktree, true);
        assert_ne!(observed, WorkspaceEvidence::Matching);

        // Mid-run: unlanded commits without a verified candidate need a human.
        let running = classify_interrupted_task(evidence(
            observed,
            DeliveryEvidence::None,
            TaskState::Running,
            AttemptState::Running,
        ));
        assert_eq!(running, TaskRecovery::RequiresInspection);
        assert!(!gate(running));

        // A recorded candidate may be re-verified from the preserved branch.
        let candidate = classify_interrupted_task(evidence(
            observed,
            DeliveryEvidence::None,
            TaskState::Verifying,
            AttemptState::CandidateReady,
        ));
        assert_eq!(candidate, TaskRecovery::Recoverable);
        assert_eq!(git(worktree.path(), &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn removed_worktree_blocks_the_task_without_blocking_other_scheduling() {
        let fixture = fixture();
        let worktree =
            create_task_worktree(&fixture.repository, "task-1", "HEAD", &fixture.worktrees)
                .expect("create worktree");
        fs::remove_dir_all(worktree.path()).expect("simulate out-of-band removal");

        let observed = observe_workspace(&fixture, &worktree, false);
        assert_eq!(observed, WorkspaceEvidence::Missing);
        let recovery = classify_interrupted_task(evidence(
            observed,
            DeliveryEvidence::None,
            TaskState::Running,
            AttemptState::Running,
        ));
        assert_eq!(recovery, TaskRecovery::Blocked);
        assert!(gate(recovery));
    }

    #[test]
    fn uninspected_or_unrecorded_workspace_requires_inspection() {
        let fixture = fixture();
        let worktree =
            create_task_worktree(&fixture.repository, "task-1", "HEAD", &fixture.worktrees)
                .expect("create worktree");

        // Present worktree whose safety was never established.
        let uninspected = observe_workspace(&fixture, &worktree, false);
        assert_eq!(uninspected, WorkspaceEvidence::Unknown);
        // No workspace decision at all.
        assert_eq!(workspace_evidence(None, None), WorkspaceEvidence::Unknown);

        for workspace in [uninspected, WorkspaceEvidence::Unknown] {
            let recovery = classify_interrupted_task(evidence(
                workspace,
                DeliveryEvidence::None,
                TaskState::Running,
                AttemptState::Running,
            ));
            assert_eq!(recovery, TaskRecovery::RequiresInspection);
            assert!(!gate(recovery));
        }
    }

    #[test]
    fn partially_landed_delivery_requires_inspection_and_leaves_git_untouched() {
        let fixture = fixture();
        let landed_ref = "refs/heads/orchestrator/review/task-1-a";
        let pending_ref = "refs/heads/orchestrator/review/task-1-b";
        git(
            &fixture.path,
            &["update-ref", landed_ref, fixture.candidate_sha.as_str()],
        );
        let before = state_snapshot(&fixture);
        let steps = [
            step(&fixture, "delivery-a", landed_ref, &fixture.candidate_sha),
            step(&fixture, "delivery-b", pending_ref, &fixture.candidate_sha),
        ];

        let plan = reconcile_delivery_steps(&fixture.repository, &steps).expect("delivery plan");
        let observed = delivery_evidence(&plan.steps);
        assert_eq!(observed, DeliveryEvidence::Partial);

        // Even a clean matching worktree and a "succeeded" record must not
        // hide a half-landed delivery.
        for task in [TaskState::Running, TaskState::Succeeded] {
            let recovery = classify_interrupted_task(evidence(
                WorkspaceEvidence::Matching,
                observed,
                task,
                AttemptState::CandidateReady,
            ));
            assert_eq!(recovery, TaskRecovery::RequiresInspection, "{task:?}");
            assert!(!gate(recovery));
        }
        assert_eq!(
            state_snapshot(&fixture),
            before,
            "reconciliation must not write"
        );
    }

    #[test]
    fn ambiguous_delivery_step_is_never_summarised_as_landed() {
        let fixture = fixture();
        let moved_ref = "refs/heads/orchestrator/review/task-1";
        let base_sha = fixture.candidate_sha.clone();
        fs::write(fixture.path.join("other.txt"), "other\n").expect("write other");
        git(&fixture.path, &["add", "other.txt"]);
        git(&fixture.path, &["commit", "-m", "other"]);
        // Ref now points somewhere other than the recorded candidate.
        git(&fixture.path, &["update-ref", moved_ref, "HEAD"]);
        let landed_ref = "refs/heads/orchestrator/review/task-1-ok";
        git(
            &fixture.path,
            &["update-ref", landed_ref, base_sha.as_str()],
        );
        let steps = [
            step(&fixture, "delivery-a", landed_ref, &base_sha),
            step(&fixture, "delivery-b", moved_ref, &base_sha),
        ];

        let plan = reconcile_delivery_steps(&fixture.repository, &steps).expect("delivery plan");
        assert!(plan.has_ambiguous_steps());
        let observed = delivery_evidence(&plan.steps);
        assert_eq!(observed, DeliveryEvidence::Ambiguous);
        let recovery = classify_interrupted_task(evidence(
            WorkspaceEvidence::Matching,
            observed,
            TaskState::Running,
            AttemptState::CandidateReady,
        ));
        assert_eq!(recovery, TaskRecovery::RequiresInspection);
        assert!(!gate(recovery));
    }

    #[test]
    fn only_git_confirmed_delivery_is_completed_and_unlanded_delivery_is_not() {
        let fixture = fixture();
        let review_ref = "refs/heads/orchestrator/review/task-1";
        let steps = [step(
            &fixture,
            "delivery-a",
            review_ref,
            &fixture.candidate_sha,
        )];

        // Nothing landed: an exited agent is not completion.
        let plan = reconcile_delivery_steps(&fixture.repository, &steps).expect("delivery plan");
        let not_applied = delivery_evidence(&plan.steps);
        assert_eq!(not_applied, DeliveryEvidence::NotApplied);
        let recovery = classify_interrupted_task(evidence(
            WorkspaceEvidence::Matching,
            not_applied,
            TaskState::Running,
            AttemptState::Running,
        ));
        assert_ne!(recovery, TaskRecovery::Completed);

        // Git now proves the delivery: completed, whatever the task record says.
        git(
            &fixture.path,
            &["update-ref", review_ref, fixture.candidate_sha.as_str()],
        );
        let plan = reconcile_delivery_steps(&fixture.repository, &steps).expect("delivery plan");
        let confirmed = delivery_evidence(&plan.steps);
        assert_eq!(confirmed, DeliveryEvidence::Confirmed);
        let recovery = classify_interrupted_task(evidence(
            WorkspaceEvidence::Matching,
            confirmed,
            TaskState::Running,
            AttemptState::Running,
        ));
        assert_eq!(recovery, TaskRecovery::Completed);
        assert!(gate(recovery));
    }

    // ---- F1: unsafe workspace evidence must outrank completion shortcuts ----
    //
    // RED provenance (base 4fc9ff3b): `classify_interrupted_task` returns
    // `Completed` for `Succeeded` tasks and `Confirmed` delivery before it
    // looks at the workspace, so dirty, uninspected, or foreign workspaces are
    // reported as completed. The legitimate clean crash-recovery case
    // (clean `Matching` workspace + `Confirmed` delivery) is deliberately NOT
    // asserted here; its expected class awaits the owner's decision matrix.

    /// Real workspace evidence for each unsafe class, built from real Git state.
    /// Returns (label, evidence, path whose contents must survive).
    fn unsafe_workspaces(fixture: &Fixture) -> Vec<(&'static str, WorkspaceEvidence, PathBuf)> {
        let dirty_tree =
            create_task_worktree(&fixture.repository, "dirty", "HEAD", &fixture.worktrees)
                .expect("create dirty worktree");
        let dirty_file = dirty_tree.path().join("unsaved.txt");
        fs::write(&dirty_file, "uncommitted\n").expect("write uncommitted work");
        let dirty = observe_workspace(fixture, &dirty_tree, true);

        let unknown_tree =
            create_task_worktree(&fixture.repository, "unknown", "HEAD", &fixture.worktrees)
                .expect("create uninspected worktree");
        let unknown = observe_workspace(fixture, &unknown_tree, false);

        let foreign_path = fixture.worktrees.join("foreign");
        fs::create_dir_all(&foreign_path).expect("create foreign directory");
        let foreign_file = foreign_path.join("keep.txt");
        fs::write(&foreign_file, "keep\n").expect("write foreign data");
        let recorded = [RecordedWorkspace {
            id: "workspace-foreign".into(),
            path: foreign_path,
            branch: "orchestrator/foreign".into(),
            base_commit: fixture.candidate_sha.clone(),
        }];
        let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("workspace plan");
        let foreign = workspace_evidence(plan.workspaces.first(), None);

        assert_eq!(dirty, WorkspaceEvidence::Dirty);
        assert_eq!(unknown, WorkspaceEvidence::Unknown);
        assert_eq!(foreign, WorkspaceEvidence::Foreign);
        vec![
            ("dirty", dirty, dirty_file),
            ("unknown", unknown, unknown_tree.path().to_path_buf()),
            ("foreign", foreign, foreign_file),
        ]
    }

    fn real_delivery(fixture: &Fixture, landed: bool) -> DeliveryEvidence {
        let review_ref = "refs/heads/orchestrator/review/task-1";
        if landed {
            git(
                &fixture.path,
                &["update-ref", review_ref, fixture.candidate_sha.as_str()],
            );
        }
        let steps = [step(
            fixture,
            "delivery-a",
            review_ref,
            &fixture.candidate_sha,
        )];
        let plan = reconcile_delivery_steps(&fixture.repository, &steps).expect("delivery plan");
        delivery_evidence(&plan.steps)
    }

    fn assert_all_require_inspection(cases: Vec<(String, TaskRecovery)>) {
        let failures: Vec<_> = cases
            .iter()
            .filter(|(_, recovery)| {
                *recovery != TaskRecovery::RequiresInspection || gate(*recovery)
            })
            .map(|(label, recovery)| format!("{label} => {recovery:?}"))
            .collect();
        assert!(
            failures.is_empty(),
            "unsafe workspace must require inspection and block scheduling, got: {failures:#?}"
        );
    }

    #[test]
    fn unsafe_workspace_requires_inspection_even_when_task_succeeded() {
        let fixture = fixture();
        let workspaces = unsafe_workspaces(&fixture);
        let not_applied = real_delivery(&fixture, false);
        assert_eq!(not_applied, DeliveryEvidence::NotApplied);

        let mut cases = Vec::new();
        for (name, workspace, path) in &workspaces {
            for (delivery_name, delivery) in [
                ("no-delivery", DeliveryEvidence::None),
                ("not-applied", not_applied),
            ] {
                let recovery = classify_interrupted_task(evidence(
                    *workspace,
                    delivery,
                    TaskState::Succeeded,
                    AttemptState::Succeeded,
                ));
                cases.push((format!("Succeeded+{name}+{delivery_name}"), recovery));
            }
            assert!(path.exists(), "{name} work must be preserved");
        }
        assert_all_require_inspection(cases);
    }

    #[test]
    fn unsafe_workspace_requires_inspection_even_when_delivery_is_confirmed() {
        let fixture = fixture();
        let workspaces = unsafe_workspaces(&fixture);
        let confirmed = real_delivery(&fixture, true);
        assert_eq!(confirmed, DeliveryEvidence::Confirmed);

        let mut cases = Vec::new();
        for (name, workspace, path) in &workspaces {
            for (task, attempt) in [
                (TaskState::Running, AttemptState::Running),
                (TaskState::Verifying, AttemptState::CandidateReady),
                (TaskState::Succeeded, AttemptState::Succeeded),
            ] {
                let recovery =
                    classify_interrupted_task(evidence(*workspace, confirmed, task, attempt));
                cases.push((format!("{task:?}+{name}+Confirmed"), recovery));
            }
            assert!(path.exists(), "{name} work must be preserved");
        }
        assert_all_require_inspection(cases);
    }
}
