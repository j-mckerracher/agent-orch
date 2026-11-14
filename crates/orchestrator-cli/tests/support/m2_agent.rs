//! Std-only stand-in for `codex exec --json`, compiled with `rustc` by
//! `crash_concurrency.rs` so the same two-task fixture runs on macOS and Windows.
//!
//! The agent writes one uncommitted file into its working directory, publishes
//! its PID by atomically creating `M2_READY_MARKER`, then holds until
//! `M2_BARRIER_FILE` exists. It never sleeps for synchronisation: the test
//! releases it through the barrier. The wait expires on its own so a failed test
//! cannot leave a permanent orphan.
//!
//! Environment: `M2_TASK_FILE`, `M2_TASK_CONTENT`, `M2_READY_MARKER`,
//! `M2_BARRIER_FILE`, and optional `M2_EXIT_CODE` (exit with that code after the
//! barrier instead of completing the turn, simulating a runtime failure).

use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const THREAD_ID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const SELF_EXPIRY: Duration = Duration::from_secs(120);

fn path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is not set")))
}

fn main() {
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        r#"{{"type":"thread.started","thread_id":"{THREAD_ID}"}}"#
    )
    .unwrap();
    writeln!(out, r#"{{"type":"turn.started"}}"#).unwrap();
    out.flush().unwrap();

    let content = std::env::var("M2_TASK_CONTENT").expect("M2_TASK_CONTENT is not set");
    std::fs::write(path("M2_TASK_FILE"), content).unwrap();

    let ready = path("M2_READY_MARKER");
    let partial = ready.with_extension("partial");
    std::fs::write(&partial, format!("{}\n", std::process::id())).unwrap();
    std::fs::rename(&partial, &ready).unwrap();

    let barrier = path("M2_BARRIER_FILE");
    let deadline = Instant::now() + SELF_EXPIRY;
    while !barrier.exists() {
        if Instant::now() >= deadline {
            eprintln!("m2 agent: barrier never released");
            std::process::exit(70);
        }
        thread::sleep(Duration::from_millis(50));
    }

    if let Ok(code) = std::env::var("M2_EXIT_CODE") {
        eprintln!("m2 agent: injected runtime failure");
        std::process::exit(code.parse().expect("M2_EXIT_CODE must be an integer"));
    }
    writeln!(
        out,
        r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":"done"}}}}"#
    )
    .unwrap();
    writeln!(out, r#"{{"type":"turn.completed"}}"#).unwrap();
}
