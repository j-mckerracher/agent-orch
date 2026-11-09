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
