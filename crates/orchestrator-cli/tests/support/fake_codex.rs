//! Std-only stand-in for `codex exec --json`, compiled with `rustc` by
//! `portable_acceptance.rs` so the same fixture runs on macOS and Windows.
//! `FAKE_AGENT_SCENARIO` selects the behavior:
//! - `edit`: complete one turn after writing files into the working directory.
//! - `tree`: start a turn, spawn one descendant, publish both PIDs to
//!   `FAKE_AGENT_PIDFILE`, then wait. Every wait expires on its own so a
//!   failed run cannot leave a permanent orphan.

use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const THREAD_ID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const SELF_EXPIRY: Duration = Duration::from_secs(120);

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--descendant") {
        thread::sleep(SELF_EXPIRY);
        return;
    }
    let scenario = std::env::var("FAKE_AGENT_SCENARIO").unwrap_or_default();
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        r#"{{"type":"thread.started","thread_id":"{THREAD_ID}"}}"#
    )
    .unwrap();
    writeln!(out, r#"{{"type":"turn.started"}}"#).unwrap();
    out.flush().unwrap();
    match scenario.as_str() {
        "edit" => {
            std::fs::write("sample.txt", "candidate\n").unwrap();
            std::fs::write("new-file.txt", "untracked candidate\n").unwrap();
            writeln!(
                out,
                r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":"edited sample"}}}}"#
            )
            .unwrap();
            writeln!(out, r#"{{"type":"turn.completed"}}"#).unwrap();
        }
        "tree" => {
            let descendant = Command::new(std::env::current_exe().unwrap())
                .arg("--descendant")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn descendant");
            let pidfile = std::path::PathBuf::from(std::env::var_os("FAKE_AGENT_PIDFILE").unwrap());
            let partial = pidfile.with_extension("partial");
            std::fs::write(
                &partial,
                format!("{}\n{}\n", std::process::id(), descendant.id()),
            )
            .unwrap();
            std::fs::rename(&partial, &pidfile).unwrap();
            thread::sleep(SELF_EXPIRY);
        }
        other => {
            eprintln!("unknown FAKE_AGENT_SCENARIO {other:?}");
            std::process::exit(64);
        }
    }
}
