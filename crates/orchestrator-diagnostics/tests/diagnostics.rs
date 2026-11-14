//! Line format, field masking, panic hook shape and log tails.

use orchestrator_diagnostics::{
    format_event, rfc3339_utc, tail_redacted, truncate, Diagnostics, Level, MASK,
};

fn parse(line: &str) -> serde_json::Value {
    serde_json::from_str(line).expect("one JSON object per line")
}

#[test]
fn event_lines_are_single_line_json_with_the_fixed_fields() {
    let diag = Diagnostics::memory();
    diag.warn("daemon", "launch_failed", &[("task_id", "t-1"), ("error", "line one\nline two")]);
    let lines = diag.lines();
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].contains('\n'));
    let value = parse(&lines[0]);
    assert_eq!(value["level"], "warn");
    assert_eq!(value["component"], "daemon");
    assert_eq!(value["event"], "launch_failed");
    assert_eq!(value["task_id"], "t-1");
    assert_eq!(value["error"], "line one\nline two");
    let ts = value["ts"].as_str().unwrap();
    assert_eq!(ts.len(), 20);
    assert!(ts.ends_with('Z'));
}

#[test]
fn field_values_are_redacted_and_credential_named_fields_are_masked() {
    let line = format_event(
        Level::Error,
        "daemon",
        "x",
        &[
            ("error", "git failed: https://u:pw12345@host/r.git"),
            ("auth_token", "plain-looking-value"),
            ("event", "spoof"),
        ],
    );
    assert!(!line.contains("pw12345") && !line.contains("plain-looking-value"), "{line}");
    let value = parse(&line);
    assert_eq!(value["auth_token"], MASK);
    assert_eq!(value["event"], "x");
    assert_eq!(value["field_event"], "spoof");
}

#[test]
fn long_values_are_truncated() {
    let line = format_event(Level::Info, "c", "e", &[("blob", &"x ".repeat(5_000))]);
    let blob = parse(&line)["blob"].as_str().unwrap().to_owned();
    assert!(blob.contains("[truncated"), "{blob}");
    assert!(blob.len() < 2_100);
    assert_eq!(truncate("short", 10), "short");
}

#[test]
fn timestamps_follow_the_civil_calendar() {
    assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(rfc3339_utc(1_792_000_000), "2026-10-14T17:46:40Z");
}

#[test]
fn clones_share_one_memory_sink() {
    let diag = Diagnostics::memory();
    let clone = diag.clone();
    clone.info("c", "e", &[]);
    assert_eq!(diag.lines().len(), 1);
}

#[test]
fn log_tail_is_redacted_bounded_and_starts_on_a_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.log");
    let mut text = String::new();
    for n in 0..200 {
        text.push_str(&format!("line {n} password=hunter2-{n}\n"));
    }
    std::fs::write(&path, &text).unwrap();
    let tail = tail_redacted(&path, 300).unwrap();
    assert!(tail.len() <= 300);
    assert!(tail.starts_with("line "), "{tail}");
    assert!(tail.contains("line 199"));
    assert!(!tail.contains("hunter2"), "{tail}");
    assert!(tail_redacted(&dir.path().join("missing.log"), 10).is_err());
}
