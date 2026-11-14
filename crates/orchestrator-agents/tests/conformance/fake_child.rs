use std::{
    io::{self, BufRead, Write},
    process, thread,
    time::Duration,
};

const SESSION: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const LAUNCH_ARGUMENT: &str = "literal arg;$(echo must-not-run)";

/// Codex 0.167 emits this nonterminal error item after `thread.started` and
/// before `turn.started`, then continues with a successful turn.
const CODEX_HOOKS_WARNING: &str = r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"`[features].codex_hooks` is deprecated. Use `[features].hooks` instead."}}"#;
const CODEX_COMMAND_STARTED: &str = r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"printf ok","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#;
const CODEX_COMMAND_DONE: &str = r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"printf ok","aggregated_output":"ok","exit_code":0,"status":"completed"}}"#;
const CODEX_MESSAGE_DONE: &str =
    r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"done"}}"#;

/// Rewrites every `\n` as `\r\n`, the line ending a Windows producer may use.
struct CrLf<'a, W: Write>(&'a mut W);

impl<W: Write> Write for CrLf<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for part in buf.split_inclusive(|byte| *byte == b'\n') {
            match part.strip_suffix(b"\n") {
                Some(text) => {
                    self.0.write_all(text)?;
                    self.0.write_all(b"\r")?;
                    self.0.flush()?;
                    self.0.write_all(b"\n")?;
                }
                None => self.0.write_all(part)?,
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Records a long-lived descendant before any protocol output so the harness
/// can prove cancellation removes the whole process tree.
fn spawn_descendant() {
    let descendant = process::Command::new(std::env::current_exe().expect("fake child path"))
        .arg("--descendant")
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .spawn()
        .expect("spawn descendant");
    std::fs::write("descendant.pid", descendant.id().to_string()).expect("record descendant pid");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--descendant") {
        sleep_forever();
    }
    let start = if args.first().map(String::as_str) == Some("--mode") {
        2
    } else {
        0
    };
    let adapter = args.get(start).map(String::as_str).unwrap_or_default();
    let scenario = args.get(start + 1).map(String::as_str).unwrap_or_default();
    let variant = args.get(start + 2).map(String::as_str).unwrap_or_default();
    let argument = args.get(start + 3).map(String::as_str);
    if adapter == "pi" {
        pi(scenario, variant, argument);
    } else {
        stream(adapter, scenario, variant, argument);
    }
}

fn stream(adapter: &str, scenario: &str, variant: &str, argument: Option<&str>) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match scenario {
        "framing" if variant == "oversized" => oversized_stream(adapter, &mut out),
        "framing" if variant == "crlf" => complete_stream(adapter, &mut CrLf(&mut out)),
        "framing" => partial_start(adapter, &mut out),
        "launch" => {
            if argument != Some(LAUNCH_ARGUMENT) {
                process::exit(64);
            }
            eprintln!("diagnostic: stderr is not protocol");
            complete_stream(adapter, &mut out);
        }
        "approvals" => {
            start_stream(adapter, &mut out);
            let request = if adapter == "codex" {
                r#"{"type":"exec_approval_request","call_id":"call_1"}"#
            } else {
                r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool"}}"#
            };
            event(&mut out, request);
            complete_body(adapter, &mut out);
        }
        "errors" if variant == "warning" && adapter == "codex" => {
            start_stream(adapter, &mut out);
            event(&mut out, CODEX_HOOKS_WARNING);
            event(&mut out, r#"{"type":"turn.started"}"#);
            event(&mut out, CODEX_COMMAND_STARTED);
            event(&mut out, CODEX_COMMAND_DONE);
            event(&mut out, CODEX_MESSAGE_DONE);
            event(&mut out, r#"{"type":"turn.completed"}"#);
        }
        "errors" if variant == "preturn_message" && adapter == "codex" => {
            start_stream(adapter, &mut out);
            event(&mut out, CODEX_MESSAGE_DONE);
            complete_body(adapter, &mut out);
        }
        "errors" if variant == "postturn_error" && adapter == "codex" => {
            complete_stream(adapter, &mut out);
            event(&mut out, CODEX_HOOKS_WARNING);
        }
        "errors" if variant == "result_error" && adapter == "claude" => {
            start_stream(adapter, &mut out);
            event(
                &mut out,
                r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"boom"}"#,
            );
        }
        "completed" => complete_stream(adapter, &mut out),
        "cancellation" | "interruption" => {
            if variant == "tree" {
                spawn_descendant();
            }
            start_stream(adapter, &mut out);
            message_stream(adapter, &mut out);
            sleep_forever();
        }
        "errors" if variant == "nonzero" => {
            complete_stream(adapter, &mut out);
            process::exit(7);
        }
        "errors" => {
            start_stream(adapter, &mut out);
            writeln!(out, "not JSON").unwrap();
        }
        "unexpected_eof" => {
            start_stream(adapter, &mut out);
            message_stream(adapter, &mut out);
        }
        _ => process::exit(64),
    }
}

fn start_stream(adapter: &str, out: &mut impl Write) {
    match adapter {
        "codex" => writeln!(
            out,
            r#"{{"type":"thread.started","thread_id":"{SESSION}"}}"#
        ),
        "claude" => writeln!(
            out,
            r#"{{"type":"system","subtype":"init","session_id":"{SESSION}"}}"#
        ),
        _ => unreachable!(),
    }
    .unwrap();
    out.flush().unwrap();
}

fn message_stream(adapter: &str, out: &mut impl Write) {
    match adapter {
        "codex" => writeln!(out, r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"ready"}}}}"#),
        "claude" => writeln!(out, r#"{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"ready"}}}}}}"#),
        _ => unreachable!(),
    }
    .unwrap();
    out.flush().unwrap();
}

fn complete_stream(adapter: &str, out: &mut impl Write) {
    start_stream(adapter, out);
    complete_body(adapter, out);
}

fn complete_body(adapter: &str, out: &mut impl Write) {
    match adapter {
        "codex" => {
            writeln!(out, r#"{{"type":"turn.started"}}"#).unwrap();
            writeln!(
                out,
                r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"done"}}}}"#
            )
            .unwrap();
            writeln!(out, r#"{{"type":"turn.completed"}}"#).unwrap();
        }
        "claude" => {
            writeln!(
                out,
                r#"{{"type":"result","subtype":"success","is_error":false,"result":"done"}}"#
            )
            .unwrap();
        }
        _ => unreachable!(),
    }
    out.flush().unwrap();
}

fn partial_start(adapter: &str, out: &mut impl Write) {
    let line = match adapter {
        "codex" => format!(r#"{{"type":"thread.started","thread_id":"{SESSION}"}}"#),
        "claude" => format!(r#"{{"type":"system","subtype":"init","session_id":"{SESSION}"}}"#),
        _ => unreachable!(),
    };
    let split = line.len() / 2;
    out.write_all(&line.as_bytes()[..split]).unwrap();
    out.flush().unwrap();
    out.write_all(&line.as_bytes()[split..]).unwrap();
    writeln!(out).unwrap();
    complete_body(adapter, out);
}

fn oversized_stream(adapter: &str, out: &mut impl Write) {
    start_stream(adapter, out);
    let text = "a".repeat(70_000);
    match adapter {
        "codex" => writeln!(out, r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"{text}"}}}}"#),
        "claude" => writeln!(out, r#"{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"{text}"}}}}}}"#),
        _ => unreachable!(),
    }
    .unwrap();
}

fn sleep_forever() {
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn pi(scenario: &str, variant: &str, argument: Option<&str>) {
    let stdin = std::io::stdin();
    let mut command_line = String::new();
    stdin
        .lock()
        .read_line(&mut command_line)
        .expect("read RPC command");
    let id = field(&command_line, "id");
    let command = field(&command_line, "type");
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match scenario {
        "framing" if variant == "oversized" => {
            let padding = "a".repeat(70_000);
            writeln!(
                out,
                r#"{{"type":"response","id":"{id}","command":"{command}","success":true,"data":{{"padding":"{padding}"}}}}"#
            )
            .unwrap();
        }
        "framing" if variant == "crlf" => {
            let mut crlf = CrLf(&mut out);
            response(&mut crlf, &id, &command, r#"{"marker":"complete"}"#);
            event(&mut crlf, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "framing" => {
            let line = format!(
                r#"{{"type":"response","id":"{id}","command":"{command}","success":true,"data":{{"marker":"complete"}}}}"#
            );
            write_split(&mut out, &line);
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "launch" => {
            if argument != Some(LAUNCH_ARGUMENT) {
                process::exit(64);
            }
            eprintln!("diagnostic: stderr is not protocol");
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ready"}}"#,
            );
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "approvals" => {
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"extension_ui_request","id":"u1","method":"confirm","title":"Allow?"}"#,
            );
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "completed" => {
            response(&mut out, &id, &command, "{}");
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "cancellation" | "interruption" => {
            if variant == "tree" {
                spawn_descendant();
            }
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ready"}}"#,
            );
            out.flush().unwrap();
            sleep_forever();
        }
        "errors" if variant == "aborted" => {
            response(&mut out, &id, &command, "{}");
            event(&mut out, r#"{"type":"agent_settled","aborted":true}"#);
        }
        "errors" if variant == "nonzero" => {
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"done"}}"#,
            );
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
            process::exit(7);
        }
        "errors" => {
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"partial"}}"#,
            );
            writeln!(out, "not JSON").unwrap();
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "unexpected_eof" => {
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"partial"}}"#,
            );
        }
        _ => process::exit(64),
    }
    out.flush().unwrap();
}

fn response(out: &mut impl Write, id: &str, command: &str, data: &str) {
    writeln!(
        out,
        r#"{{"type":"response","id":"{id}","command":"{command}","success":true,"data":{data}}}"#
    )
    .unwrap();
}

fn event(out: &mut impl Write, record: &str) {
    writeln!(out, "{record}").unwrap();
    out.flush().unwrap();
}

fn write_split(out: &mut impl Write, record: &str) {
    let split = record.len() / 2;
    out.write_all(&record.as_bytes()[..split]).unwrap();
    out.flush().unwrap();
    out.write_all(&record.as_bytes()[split..]).unwrap();
    writeln!(out).unwrap();
    out.flush().unwrap();
}

fn field(line: &str, name: &str) -> String {
    let marker = format!("\"{name}\":\"");
    let start = line.find(&marker).expect("RPC field") + marker.len();
    let end = line[start..].find('"').expect("RPC string end") + start;
    line[start..end].to_owned()
}
