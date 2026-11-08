use std::{
    io::{BufRead, Write},
    process, thread,
    time::Duration,
};

const SESSION: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const LAUNCH_ARGUMENT: &str = "literal arg;$(echo must-not-run)";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
        "framing" => partial_start(adapter, &mut out),
        "launch" => {
            if argument != Some(LAUNCH_ARGUMENT) {
                process::exit(64);
            }
            complete_stream(adapter, &mut out);
        }
        "cancellation" | "interruption" => {
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
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ready"}}"#,
            );
            event(&mut out, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "cancellation" | "interruption" => {
            response(&mut out, &id, &command, "{}");
            event(
                &mut out,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ready"}}"#,
            );
            out.flush().unwrap();
            sleep_forever();
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
