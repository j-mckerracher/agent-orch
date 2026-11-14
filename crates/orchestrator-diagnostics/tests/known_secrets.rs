//! Exact-value redaction of registered secrets. The registry is process-wide,
//! so every test here registers its own distinctive value.

use orchestrator_diagnostics::{redact, register_secret, watch_secret_file, MASK};
use std::fs;
use tempfile::TempDir;

#[test]
fn a_registered_secret_is_masked_wherever_it_appears() {
    let secret = "zq7Kp2mWx9Lr4TnV8bYc";
    register_secret(secret);
    for text in [
        format!("plain {secret} here"),
        format!("{secret}-suffix"),
        format!("prefix_{secret}"),
        format!("tok{secret}"),
        format!("0x{secret}"),
        format!("{secret}{secret}"),
        format!("in quotes \"{secret}\" and '{secret}'"),
    ] {
        let out = redact(&text);
        assert!(!out.contains(secret), "{text} -> {out}");
        assert!(out.contains(MASK), "{text} -> {out}");
        assert_eq!(redact(&out), out, "idempotent");
    }
    assert_eq!(redact("nothing secret"), "nothing secret");
}

#[test]
fn a_registered_secret_split_across_lines_or_escaped_breaks_is_masked() {
    let secret = "Hm3Qa8Vd5Fn1Ys6Gt0Wu";
    register_secret(secret);
    let (head, tail) = secret.split_at(secret.len() / 2);
    for sep in ["\n", "\r\n", " ", "\t", "\\n", "\n\n  "] {
        let text = format!("log {head}{sep}{tail} end");
        let out = redact(&text);
        assert!(!out.contains(head) && !out.contains(tail), "{sep:?}: {out}");
        assert!(out.ends_with(" end") && out.starts_with("log "), "{sep:?}: {out}");
    }
}

#[test]
fn matching_ignores_ascii_case_and_leaves_other_text() {
    let secret = "ab12cd34ef56ab78";
    register_secret(secret);
    let out = redact(&format!("a {} b", secret.to_ascii_uppercase()));
    assert_eq!(out, format!("a {MASK} b"));
    assert_eq!(redact("ab12cd34 ef99"), "ab12cd34 ef99");
}

#[test]
fn short_values_and_empty_values_are_ignored() {
    register_secret("");
    register_secret("   ");
    register_secret("short");
    register_secret("REDACTED");
    assert_eq!(redact("short and REDACTED stay"), "short and REDACTED stay");
}

#[test]
fn multibyte_secrets_are_masked_without_splitting_characters() {
    let secret = "пароль-ключ-Ж9";
    register_secret(secret);
    let out = redact(&format!("é{secret}é"));
    assert_eq!(out, format!("é{MASK}é"));
}

#[test]
fn a_watched_token_file_is_masked_now_and_after_it_changes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("daemon.token");
    // Not hex, so only the exact-value match can catch them.
    let first = "tokfile-first-Qm8Zr3Vw5Yb1Kd7Xn2Hs";
    let second = "tokfile-second-Lc4Pj9Gt6Fa0Ue5Wo3Ri";

    // Not created yet: watching must not fail, and the file is picked up later.
    watch_secret_file(&path);
    fs::write(&path, format!("{first}\n")).unwrap();
    let out = redact(&format!("x{first}-y"));
    assert!(!out.contains(first), "{out}");

    // Regenerated with a different length: a restart wrote a new token.
    fs::write(&path, format!("{second}-extra\n")).unwrap();
    let out = redact(&format!("tok{second}-extra!"));
    assert!(!out.contains(second), "{out}");
    // The earlier token stays masked after the file moves on.
    assert!(!redact(first).contains(first));

    // A removed file (the daemon deletes it on exit) keeps its contribution.
    fs::remove_file(&path).unwrap();
    assert!(!redact(&format!("{second}-extra")).contains(second));
    watch_secret_file(&path);
}

#[test]
fn registered_secrets_do_not_make_redaction_quadratic() {
    register_secret("aaaaaaaaaaaaaaaaaaaaaaab");
    let started = std::time::Instant::now();
    let _ = redact(&"a".repeat(2_000_000));
    assert!(started.elapsed().as_secs() < 20, "took {:?}", started.elapsed());
}
