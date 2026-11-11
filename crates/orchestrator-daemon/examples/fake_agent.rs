//! Deterministic stand-in for the Codex and Claude Code CLIs, used by the
//! conversation runtime's tests and the desktop e2e suite (M7-25).
//!
//! Point `AGENT_ORCH_CODEX_PROGRAM` or `AGENT_ORCH_CLAUDE_PROGRAM` at this
//! binary. It answers the probes the catalog runs (`--version`, login state,
//! models, `--help`) and the conversation launch (`exec --json ...` for Codex,
//! `-p ... --output-format stream-json` for Claude Code) with the same JSON
//! lines the real CLIs print, so it exercises the real adapters and parsers.
//!
//! The reply depends only on the prompt. Markers in the operator's current
//! message (the text after the last `Current message:` line) select a mode:
//!
//! - `[fake:fail]` exits non-zero without finishing the turn.
//! - `[fake:hang]` starts the turn and never finishes it.
//! - `[fake:slow]` replies in five pieces, 200 ms apart.
//! - `[fake:control]` replies with control, bidirectional and invisible characters.
//! - `[fake:flood]` replies with more text than a turn may store.
//! - `[fake:echo]` replies with the prompt's context and history lines.
//!
//! Environment: `AGENT_ORCH_FAKE_VERSION` (default `0.0.0-fake`),
//! `AGENT_ORCH_FAKE_VERSION_MODE` (`fail`, `garbage`, `hang` or `flood`: how
//! `--version` misbehaves), `AGENT_ORCH_FAKE_LOG` (appends one line per turn
//! with the working directory and arguments) and `AGENT_ORCH_FAKE_PID_FILE`
//! (written with this process's id when a turn starts).

use std::io::Write;
use std::time::Duration;

const SESSION_ID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const MARKER: &str = "Current message:\n";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let version = std::env::var("AGENT_ORCH_FAKE_VERSION").unwrap_or_else(|_| "0.0.0-fake".into());
    let first: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
    match first.as_slice() {
        ["--version", ..] | ["-V", ..] => version_probe(&version),
        ["login", "status"] => println!("Logged in using ChatGPT"),
        ["debug", "models"] => println!(
            r#"{{"models":[{{"slug":"fake-model","display_name":"Fake model","visibility":"list","default_reasoning_level":"low","supported_reasoning_levels":[{{"effort":"low"}},{{"effort":"high"}}]}}]}}"#
        ),
        ["auth", "status"] => println!(r#"{{"loggedIn":true}}"#),
        ["--help", ..] => println!(
            "Usage: claude [options]\n  --effort <level>   Effort level (low, medium, high)\n  --model <model>    Model"
        ),
        ["exec", ..] => {
            let prompt = args.last().cloned().unwrap_or_default();
            turn(&args, &prompt, Dialect::Codex);
        }
        _ if args.iter().any(|arg| arg == "-p") => {
            let at = args.iter().position(|arg| arg == "-p").unwrap_or(0);
            let prompt = args.get(at + 1).cloned().unwrap_or_default();
            turn(&args, &prompt, Dialect::Claude);
        }
        _ => {
            eprintln!("fake_agent: unsupported arguments");
            std::process::exit(2);
        }
    }
}

/// Answers `--version`, or misbehaves as `AGENT_ORCH_FAKE_VERSION_MODE` says.
fn version_probe(version: &str) {
    match std::env::var("AGENT_ORCH_FAKE_VERSION_MODE").as_deref() {
        Ok("fail") => {
            eprintln!("fake_agent: version probe failing on request");
            std::process::exit(1);
        }
        Ok("garbage") => println!("this is not a version banner"),
        Ok("hang") => std::thread::sleep(Duration::from_secs(600)),
        Ok("flood") => println!("codex-cli {version} {}", "x".repeat(100 * 1024)),
        _ => println!("codex-cli {version}"),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Dialect {
    Codex,
    Claude,
}

fn emit(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn started(dialect: Dialect) {
    match dialect {
        Dialect::Codex => {
            emit(&format!(
                r#"{{"type":"thread.started","thread_id":"{SESSION_ID}"}}"#
            ));
            emit(r#"{"type":"turn.started"}"#);
        }
        Dialect::Claude => emit(&format!(
            r#"{{"type":"system","subtype":"init","session_id":"{SESSION_ID}"}}"#
        )),
    }
}

/// One piece of reply text. Codex reports whole messages; Claude Code reports
/// fragments.
fn piece(dialect: Dialect, text: &str) {
    let text = serde_json::Value::String(text.to_owned());
    match dialect {
        Dialect::Codex => emit(&format!(
            r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":{text}}}}}"#
        )),
        Dialect::Claude => emit(&format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":{text}}}}}}}"#
        )),
    }
}

fn finished(dialect: Dialect, whole: &str) {
    match dialect {
        Dialect::Codex => emit(r#"{"type":"turn.completed","usage":{"input_tokens":1}}"#),
        Dialect::Claude => emit(&format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"result":{}}}"#,
            serde_json::Value::String(whole.to_owned())
        )),
    }
}

fn turn(args: &[String], prompt: &str, dialect: Dialect) {
    if let Some(path) = std::env::var_os("AGENT_ORCH_FAKE_LOG") {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "cwd={:?} args={args:?}", std::env::current_dir());
        }
    }
    if let Some(path) = std::env::var_os("AGENT_ORCH_FAKE_PID_FILE") {
        let _ = std::fs::write(path, std::process::id().to_string());
    }
    started(dialect);
    let message = prompt.rsplit_once(MARKER).map_or(prompt, |(_, tail)| tail);
    let mut pieces: Vec<String> = Vec::new();
    if message.contains("[fake:fail]") {
        eprintln!("fake_agent: failing on request");
        std::process::exit(3);
    } else if message.contains("[fake:hang]") {
        std::thread::sleep(Duration::from_secs(600));
        return;
    } else if message.contains("[fake:slow]") {
        pieces.extend((1..=5).map(|n| format!("part{n} ")));
    } else if message.contains("[fake:control]") {
        pieces.push(
            "a\u{1b}[31mred\u{7}\u{0}b\r\nc\u{202e}d\u{200b}e\u{feff}f\u{2028}g\u{2029}h\u{e0041}\u{2060}i"
                .to_owned(),
        );
    } else if message.contains("[fake:flood]") {
        // 40 pieces of 8 KiB is more than the 256 KiB a turn may store.
        pieces.extend((0..40).map(|_| "x".repeat(8 * 1024)));
    } else if message.contains("[fake:echo]") {
        let context = prompt
            .lines()
            .filter(|line| {
                line.starts_with("Scope:")
                    || line.starts_with("Workstream context:")
                    || line.starts_with("Operator:")
                    || line.starts_with("Assistant:")
            })
            .collect::<Vec<_>>()
            .join("\n");
        pieces.push(format!("echo\n{context}"));
    } else {
        pieces.push(format!("fake reply to: {}", message.trim()));
    }
    let delay = message.contains("[fake:slow]");
    let mut whole = String::new();
    for text in &pieces {
        piece(dialect, text);
        whole.push_str(text);
        if delay {
            std::thread::sleep(Duration::from_millis(200));
        }
        // A reply too large to store is cut off by the daemon, which kills
        // this process; keep going until it does.
    }
    finished(dialect, &whole);
}
