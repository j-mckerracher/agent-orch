//! Scripted stand-in for `copilot --acp`, used by copilot_acp_adapter and interactive contract tests.
//!
//! Runs in the test's working directory: prints each line of `script.txt` to stdout, records every
//! stdin line to `stdin.log`, and exits when stdin closes with the status in `exit_code` (default 0).
//! An empty script exits at once, which models a child that dies before answering the handshake.

use std::io::{BufRead, Write};

fn main() {
    let script = std::fs::read_to_string("script.txt").expect("script.txt");
    if script.is_empty() {
        return;
    }
    let mut out = std::io::stdout().lock();
    for line in script.lines() {
        writeln!(out, "{line}").expect("stdout line");
    }
    out.flush().expect("flush");
    drop(out);
    let mut log = std::fs::File::create("stdin.log").expect("stdin.log");
    for line in std::io::stdin().lock().lines() {
        writeln!(log, "{}", line.expect("stdin line")).expect("log line");
    }
    std::process::exit(exit_code());
}

fn exit_code() -> i32 {
    std::fs::read_to_string("exit_code")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}
