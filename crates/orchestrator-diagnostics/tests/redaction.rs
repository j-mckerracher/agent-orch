//! Redaction behaviour, including Windows-shaped text and linear-time checks.

use orchestrator_diagnostics::{redact, MASK};

const HEX64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn assert_hidden(input: &str, secret: &str) {
    let out = redact(input);
    assert!(!out.contains(secret), "secret survived: {out}");
    assert!(out.contains(MASK), "nothing masked: {out}");
    assert_eq!(redact(&out), out, "redact must be idempotent");
}

#[test]
fn url_userinfo_is_masked_but_host_and_path_stay() {
    let out = redact("fatal: unable to access 'https://bob:hunter2pass@example.com/org/repo.git/'");
    assert!(!out.contains("hunter2pass") && !out.contains("bob"), "{out}");
    assert!(out.contains("@example.com/org/repo.git"), "{out}");
    assert_hidden("clone https://ghp_abcdefghijklmnopqrstuvwxyz0123@github.com/o/r", "ghp_abcdefghijklmnopqrstuvwxyz0123");
    assert_hidden("remote ssh://deploy:s3cretpw@host/repo", "s3cretpw");
}

#[test]
fn plain_ssh_user_and_scp_style_remote_are_not_credentials() {
    let text = "ssh://git@github.com/o/r.git and git@github.com:o/r.git";
    assert_eq!(redact(text), text);
}

#[test]
fn sensitive_keys_hide_their_values() {
    assert_hidden("GITHUB_TOKEN=abc123xyz", "abc123xyz");
    assert_hidden("password: hunter2pass", "hunter2pass");
    assert_hidden(r#"{"api_key":"k-1234567890","other":"fine"}"#, "k-1234567890");
    assert_hidden("run --token sekrit-value --verbose", "sekrit-value");
    assert_hidden("Authorization: Bearer abcdef0123456789", "abcdef0123456789");
    assert_hidden("Authorization: Basic dXNlcjpwYXNz", "dXNlcjpwYXNz");
    assert_hidden("token='quoted secret'", "quoted secret");
    assert!(redact(r#"{"other":"fine"}"#).contains("fine"));
}

#[test]
fn keys_that_only_mention_a_marker_keep_their_values() {
    assert_eq!(redact("token_count=42 retry_limit=3"), "token_count=42 retry_limit=3");
}

#[test]
fn bearer_tokens_and_known_shapes_are_masked_anywhere() {
    assert_hidden("header was Bearer abcdefgh12345678.xyz", "abcdefgh12345678");
    assert_hidden("saw ghp_0123456789abcdefghijABCDEFGHIJ in output", "ghp_0123456789abcdefghijABCDEFGHIJ");
    assert_hidden("key sk-abcdefghijklmnopqrstuvwx used", "sk-abcdefghijklmnopqrstuvwx");
    assert_hidden("id AKIAABCDEFGHIJKLMNOP leaked", "AKIAABCDEFGHIJKLMNOP");
    assert_hidden(
        "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop end",
        "eyJhbGciOiJIUzI1NiJ9",
    );
}

#[test]
fn ipc_token_shaped_hex_is_masked_but_git_shas_are_not() {
    assert_hidden(&format!("token on wire {HEX64}"), HEX64);
    assert_hidden(&format!("connect {HEX64}."), HEX64);
    let sha1 = "efa5e8af3469082226c61cccfea167cc607d6a0f";
    assert_eq!(redact(&format!("HEAD {sha1}")), format!("HEAD {sha1}"));
    let longer = format!("{HEX64}0");
    assert_eq!(redact(&longer), longer, "only exactly 64 hex characters");
}

#[test]
fn windows_paths_and_unc_text_survive() {
    let text = r"C:\Users\ci\AppData\Local\orch\daemon.log \\server\share\repo.git C:\\tmp\\data";
    assert_eq!(redact(text), text);
    assert_hidden(r"set PASSWORD=Win-Secret1 && C:\bin\agent.exe", "Win-Secret1");
}

#[test]
fn ordinary_text_is_unchanged() {
    let text = "failed to create worktree: No such file or directory (os error 2)\nexit status: 1";
    assert_eq!(redact(text), text);
}

#[test]
fn hostile_multi_megabyte_input_stays_linear() {
    let started = std::time::Instant::now();
    let samples = [
        "a".repeat(4_000_000),
        "token ".repeat(500_000),
        "://".repeat(500_000),
        "a=".repeat(1_000_000),
        "bearer ".repeat(500_000),
        "https://".to_owned() + &"x@".repeat(1_000_000),
        format!("password: {}", "word ".repeat(600_000)),
    ];
    for sample in &samples {
        let _ = redact(sample);
    }
    assert!(
        started.elapsed().as_secs() < 20,
        "redaction took {:?}",
        started.elapsed()
    );
}

#[test]
fn multibyte_text_does_not_split_characters() {
    let out = redact("token=päss€word and ключ=значение");
    assert!(!out.contains("päss"), "{out}");
    assert!(out.contains("ключ=значение"), "{out}");
}
