//! Orphaned conversation agents (M7-25 remediation of QA finding F3).
//!
//! A conversation turn runs an agent CLI in its own process group. The turn
//! thread kills that group when the turn ends, and the process supervisor
//! kills it when the daemon exits normally, but nothing runs when the daemon
//! is killed with SIGKILL, so the agent and its descendants outlive it.
//!
//! While a turn runs, its agent's launch identity (the same
//! [`ProcessIdentity`] the task supervisor records: PID, process group and
//! start time) is written to one small file in a registry directory. The next
//! daemon reads the registry at startup, after it holds the data-directory
//! lock, so no other daemon can own a listed agent. A listed process group is
//! signalled only when the leader still exists and its observed identity
//! equals the recorded one. A PID alone is never trusted: a reused PID has a
//! different start time and is left alone.
//!
//! A record is left in place, and nothing is signalled, when the identity
//! cannot be proven. That happens when the leader has exited but a
//! descendant keeps the group alive. Windows needs none of this: the
//! supervisor puts the agent in a Job Object created with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so the kernel ends the tree when the
//! daemon dies.

use std::path::{Path, PathBuf};

use orchestrator_core::identity::plan_process_reconciliation;
use orchestrator_runtime::reconcile::{
    capture_process_identity, observe_process, ProcessClassification, ProcessIdentity,
};

use crate::executor::log_safe;

const RECORD_EXTENSION: &str = "json";
const MAX_RECORD_BYTES: u64 = 1024;

/// The identity of a running agent, kept on disk until the turn ends. Dropping
/// it removes the file; a daemon killed outright never drops it, which is what
/// lets the next daemon find the agent.
pub(crate) struct AgentRecord {
    path: PathBuf,
}

impl AgentRecord {
    /// `None` when the identity cannot be captured (the process is already
    /// gone, or the platform has nothing to reap) or the file cannot be
    /// written. The turn runs either way; only the safety net is missing.
    pub(crate) fn register(dir: &Path, pid: u32) -> Option<Self> {
        if !cfg!(unix) {
            return None;
        }
        let identity = capture_process_identity(pid).ok()?;
        std::fs::create_dir_all(dir).ok()?;
        let path = dir.join(format!("{pid}.{RECORD_EXTENSION}"));
        std::fs::write(&path, serde_json::to_vec(&identity).ok()?).ok()?;
        Some(Self { path })
    }
}

impl Drop for AgentRecord {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Kills the agents a previous daemon left running and clears the registry.
/// Returns how many process groups were signalled.
pub(crate) fn reap(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut reaped = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(RECORD_EXTENSION) {
            continue;
        }
        let keep = match read_record(&path) {
            None => false,
            Some(identity) => {
                match plan_process_reconciliation(identity, observe_process(identity))
                    .classification
                {
                    ProcessClassification::Alive => {
                        reaped += usize::from(kill_group(identity));
                        false
                    }
                    ProcessClassification::Exited | ProcessClassification::IdentityMismatch => {
                        false
                    }
                    ProcessClassification::Unknown => {
                        eprintln!(
                            "orchestrator-daemon: conversation agent {} could not be identified; \
                         left running",
                            identity.pid
                        );
                        true
                    }
                }
            }
        };
        if !keep {
            let _ = std::fs::remove_file(&path);
        }
    }
    reaped
}

fn read_record(path: &Path) -> Option<ProcessIdentity> {
    let too_large = std::fs::metadata(path).ok()?.len() > MAX_RECORD_BYTES;
    if too_large {
        eprintln!(
            "orchestrator-daemon: ignoring oversized agent record {}",
            log_safe(&path.display().to_string())
        );
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Signals the process group the recorded leader started. Only called for an
/// identity that has just been observed alive and equal to the record.
#[cfg(unix)]
fn kill_group(identity: ProcessIdentity) -> bool {
    let (Some(group), Ok(pid)) = (identity.process_group, i32::try_from(identity.pid)) else {
        return false;
    };
    // The agent leads its own group (`process_group(0)`), and the daemon's
    // own group, init and group 0 are never a target.
    let own_group = unsafe { libc::getpgrp() };
    let Ok(group) = i32::try_from(group) else {
        return false;
    };
    if group != pid || pid <= 1 || group == own_group {
        return false;
    }
    // SAFETY: signals only the group whose leader matched its recorded identity.
    let signalled = unsafe { libc::kill(-group, libc::SIGKILL) } == 0;
    if signalled {
        eprintln!("orchestrator-daemon: killed orphaned conversation agent group {group}");
    }
    signalled
}

#[cfg(not(unix))]
fn kill_group(_identity: ProcessIdentity) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use orchestrator_runtime::supervisor::process_group_alive;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    /// A shell that leads its own process group and runs `script`.
    fn leader(script: &str) -> Child {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        command.spawn().expect("spawn leader")
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out: {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn records(dir: &Path) -> usize {
        std::fs::read_dir(dir).map_or(0, |entries| entries.count())
    }

    #[test]
    fn a_live_group_with_the_recorded_identity_is_killed_with_its_descendants() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = leader("sleep 60 & wait");
        std::mem::forget(AgentRecord::register(dir.path(), child.id()).expect("registered"));
        assert_eq!(records(dir.path()), 1);

        assert_eq!(reap(dir.path()), 1);

        let status = child.wait().unwrap();
        assert!(!status.success(), "{status:?}");
        wait_until("the group to disappear", || {
            !process_group_alive(child.id())
        });
        assert_eq!(records(dir.path()), 0);
    }

    #[test]
    fn a_record_whose_start_time_differs_is_a_reused_pid_and_is_never_signalled() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = leader("sleep 60");
        let record = AgentRecord::register(dir.path(), child.id()).expect("registered");
        let path = record.path.clone();
        std::mem::forget(record);
        let mut identity: ProcessIdentity =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        identity.start_time_micros = identity.start_time_micros.map(|micros| micros + 60_000_000);
        std::fs::write(&path, serde_json::to_vec(&identity).unwrap()).unwrap();

        assert_eq!(reap(dir.path()), 0);

        assert!(
            child.try_wait().unwrap().is_none(),
            "the process was killed"
        );
        assert_eq!(records(dir.path()), 0);
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn a_record_for_a_process_that_is_gone_is_cleared_without_a_signal() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = leader("sleep 60");
        std::mem::forget(AgentRecord::register(dir.path(), child.id()).expect("registered"));
        child.kill().unwrap();
        child.wait().unwrap();

        assert_eq!(reap(dir.path()), 0);
        assert_eq!(records(dir.path()), 0);
    }

    #[test]
    fn a_group_whose_leader_is_gone_cannot_be_identified_so_it_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = leader("sleep 60 & sleep 0.3");
        std::mem::forget(AgentRecord::register(dir.path(), child.id()).expect("registered"));
        child.wait().unwrap();
        assert!(
            process_group_alive(child.id()),
            "the descendant keeps the group"
        );

        assert_eq!(reap(dir.path()), 0);

        assert!(
            process_group_alive(child.id()),
            "an unidentified group was signalled"
        );
        assert_eq!(
            records(dir.path()),
            1,
            "the record stays for the next start"
        );
        // Test cleanup only: the group is this test's own.
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        wait_until("the group to disappear", || {
            !process_group_alive(child.id())
        });
        assert_eq!(reap(dir.path()), 0);
        assert_eq!(records(dir.path()), 0);
    }

    #[test]
    fn unreadable_or_oversized_records_are_removed_and_signal_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1.json"), b"not json").unwrap();
        std::fs::write(dir.path().join("2.json"), vec![b' '; 4096]).unwrap();
        std::fs::write(dir.path().join("note.txt"), b"unrelated").unwrap();

        assert_eq!(reap(dir.path()), 0);

        assert!(dir.path().join("note.txt").exists());
        assert!(!dir.path().join("1.json").exists());
        assert!(!dir.path().join("2.json").exists());
        assert_eq!(reap(&dir.path().join("missing")), 0);
    }

    #[test]
    fn a_record_is_removed_when_its_turn_ends() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = leader("sleep 60");
        let record = AgentRecord::register(dir.path(), child.id()).expect("registered");
        assert_eq!(records(dir.path()), 1);
        drop(record);
        assert_eq!(records(dir.path()), 0);
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
