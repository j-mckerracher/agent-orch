use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

fn str_field(line: &str, name: &str) -> Option<String> {
    let marker = format!("\"{name}\":\"");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_owned())
}

// The adapter writes `id` as the first key of every request, and request ids are numbers.
fn id_field(line: &str) -> String {
    let start = line.find("\"id\":").expect("request id") + "\"id\":".len();
    let rest = &line[start..];
    let end = rest.find([',', '}']).expect("request id end");
    rest[..end].to_owned()
}

fn frame(path: &Path) -> String {
    std::fs::read_to_string(path)
        .expect("frame file")
        .replace(&['\n', '\r'][..], "")
}

fn reply(out: &mut impl Write, request: &str, body: &str) {
    writeln!(out, "{{\"id\":{},\"result\":{body}}}", id_field(request)).expect("reply");
    out.flush().expect("flush reply");
}

fn main() {
    let dir = std::env::current_dir().expect("working dir");
    let scenario = std::fs::read_to_string(dir.join("scenario.txt")).unwrap_or_default();
    let mut bodies = PathBuf::new();
    let mut turns: Vec<Vec<PathBuf>> = vec![Vec::new()];
    let mut silent = false;
    for line in scenario.lines() {
        if let Some(path) = line.strip_prefix("bodies=") {
            bodies = PathBuf::from(path);
        } else if let Some(path) = line.strip_prefix("emit=") {
            turns
                .last_mut()
                .expect("turn group")
                .push(PathBuf::from(path));
        } else if line == "turn" {
            turns.push(Vec::new());
        } else if line == "silent" {
            silent = true;
        }
    }
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("stdin.log"))
        .expect("stdin log");
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut turns_started = 0;
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin line");
        writeln!(log, "{line}").expect("log line");
        log.flush().expect("flush log");
        if silent {
            std::process::exit(0);
        }
        let Some(method) = str_field(&line, "method") else {
            continue;
        };
        match method.as_str() {
            "initialize" => reply(
                &mut out,
                &line,
                &frame(&bodies.join("initialize_result.json")),
            ),
            "thread/start" | "thread/resume" => reply(
                &mut out,
                &line,
                &frame(&bodies.join("thread_start_result.json")),
            ),
            "turn/start" => {
                reply(
                    &mut out,
                    &line,
                    r#"{"turn":{"id":"turn-1","status":"inProgress"}}"#,
                );
                if let Some(group) = turns.get(turns_started) {
                    for path in group {
                        writeln!(out, "{}", frame(path)).expect("emit");
                    }
                }
                turns_started += 1;
                out.flush().expect("flush emit");
            }
            _ => {}
        }
    }
    std::process::exit(exit_code(&dir));
}

fn exit_code(dir: &Path) -> i32 {
    std::fs::read_to_string(dir.join("exit_code"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}
