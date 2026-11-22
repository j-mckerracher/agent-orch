//! Scripted stand-in for `claude` in stream-json mode, used by claude_code_stream_adapter tests.
//!
//! Runs with its working directory set to the test's temp directory. It records its argv to
//! `argv.txt`, prints each line of `script.jsonl` to stdout, then logs each stdin line to
//! `stdin.log` until stdin closes. Compiled with rustc by the test, like the conformance harness.

use std::io::{self, BufRead, Write};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::fs::write("argv.txt", argv.join("\n")).expect("record argv");
    let script = std::fs::read_to_string("script.jsonl").expect("read script");
    let mut stdout = io::stdout().lock();
    for line in script.lines() {
        writeln!(stdout, "{line}").expect("print script line");
    }
    stdout.flush().expect("flush script");
    drop(stdout);
    let mut log = std::fs::File::create("stdin.log").expect("create stdin log");
    for line in io::stdin().lock().lines() {
        let line = line.expect("read stdin");
        writeln!(log, "{line}").expect("log stdin line");
        log.flush().expect("flush stdin log");
    }
}
