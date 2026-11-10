//! Credential redaction for text that leaves the process: daemon logs, panic
//! messages, error details shown in the desktop host and test failure output.
//!
//! Every pass is a single left-to-right scan with bounded look-ahead, so cost
//! is linear in the input even for multi-megabyte hostile text. That is
//! deliberate: an unanchored pattern rescanned from each position went
//! quadratic on word runs in earlier evidence redaction.
//!
//! Redaction is best effort by shape, not a guarantee. It recognises:
//! - credentials in URL userinfo (`https://user:pass@host`, `https://TOKEN@host`),
//! - values of sensitive keys (`token=...`, `"password": "..."`,
//!   `Authorization: Bearer ...`, `--api-key ...`),
//! - `Bearer` tokens anywhere,
//! - well-known token shapes (GitHub, GitLab, OpenAI-style, Slack, AWS access
//!   key ids, JWTs) and 64-character hex strings, the shape of the daemon's
//!   IPC token (this also masks SHA-256 digests, which is the safe direction).
//!
//! `redact` is idempotent. Never rely on it to make a secret safe to store;
//! it exists so a secret that reaches an error string does not reach a log.

pub const MASK: &str = "[REDACTED]";

const SENSITIVE_KEYS: [&str; 12] = [
    "token",
    "secret",
    "password",
    "passwd",
    "passphrase",
    "api_key",
    "api-key",
    "apikey",
    "authorization",
    "credential",
    "private_key",
    "cookie",
];
// Key names that merely contain a marker but carry no secret.
const NOT_SECRET_SUFFIXES: [&str; 3] = ["_count", "_limit", "_ms"];

/// True when a field or variable name says its value is a credential
/// (`GITHUB_TOKEN`, `x-api-key`, `Password`). Case-insensitive.
pub fn is_sensitive_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let hit = SENSITIVE_KEYS.iter().any(|marker| {
        let marker = marker.as_bytes();
        bytes.len() >= marker.len()
            && bytes
                .windows(marker.len())
                .any(|window| window.eq_ignore_ascii_case(marker))
    });
    hit && !NOT_SECRET_SUFFIXES
        .iter()
        .any(|suffix| key.len() >= suffix.len() && key.as_bytes()[key.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes()))
}

/// Replaces credential-shaped text with [`MASK`].
pub fn redact(input: &str) -> String {
    let urls = redact_url_userinfo(input);
    let pairs = redact_key_values(&urls);
    let bearer = redact_bearer(&pairs);
    redact_known_tokens(&bearer)
}

/// Keeps at most `max` characters, marking the cut. Call after [`redact`] so a
/// cut cannot split a credential into an unrecognisable prefix.
pub fn truncate(input: &str, max: usize) -> String {
    match input.char_indices().nth(max) {
        None => input.to_owned(),
        Some((cut, _)) => format!(
            "{}...[truncated {} chars]",
            &input[..cut],
            input[cut..].chars().count()
        ),
    }
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
}

fn skip_blanks(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t') {
        at += 1;
    }
    at
}

fn is_authority_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'/' | b'?' | b'#' | b'"' | b'\'' | b'<' | b'>' | b')')
}

fn redact_url_userinfo(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut position) = (0, 0);
    while let Some(found) = input[position..].find("://") {
        let scheme_end = position + found;
        let start = scheme_end + 3;
        let mut end = start;
        let mut at = None;
        while end < bytes.len() && !is_authority_end(bytes[end]) {
            if bytes[end] == b'@' {
                at = Some(end);
            }
            end += 1;
        }
        if let Some(at) = at.filter(|at| *at > start) {
            // `ssh://git@host` names a user, not a credential.
            let web = ["http", "https", "ftp", "ftps"]
                .iter()
                .any(|scheme| input[..scheme_end].to_ascii_lowercase().ends_with(scheme));
            if web || bytes[start..at].contains(&b':') {
                out.push_str(&input[copied..start]);
                out.push_str(MASK);
                copied = at;
            }
        }
        position = end;
    }
    out.push_str(&input[copied..]);
    out
}

fn is_value_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b',' | b';' | b'&' | b'"' | b'\'' | b')' | b']' | b'}')
}

fn value_end(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && !is_value_end(bytes[at]) {
        at += 1;
    }
    at
}

fn redact_key_values(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut i) = (0, 0);
    while i < bytes.len() {
        if !is_key_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_key_byte(bytes[i]) {
            i += 1;
        }
        let word = &input[start..i];
        if !is_sensitive_key(word) {
            continue;
        }
        let mut at = i;
        if at < bytes.len() && matches!(bytes[at], b'"' | b'\'') {
            at += 1;
        }
        at = skip_blanks(bytes, at);
        let separated = at < bytes.len() && matches!(bytes[at], b'=' | b':');
        if separated {
            at = skip_blanks(bytes, at + 1);
        } else if !(word.starts_with('-') && at > i && bytes[i] != b'"' && bytes[i] != b'\'') {
            // Only a flag (`--token VALUE`) takes its value without a separator.
            continue;
        }
        if at >= bytes.len() {
            continue;
        }
        let (value_start, mut value_stop, resume) = if matches!(bytes[at], b'"' | b'\'') {
            let quote = bytes[at];
            let inner = at + 1;
            let mut stop = inner;
            while stop < bytes.len() && bytes[stop] != quote && bytes[stop] != b'\n' {
                stop += 1;
            }
            let closed = stop < bytes.len() && bytes[stop] == quote;
            (inner, stop, stop + usize::from(closed))
        } else {
            let stop = value_end(bytes, at);
            (at, stop, stop)
        };
        if value_stop == value_start || bytes[value_start] == b'[' && input[value_start..].starts_with("[REDACTED") {
            continue;
        }
        let first = &input[value_start..value_stop];
        if ["bearer", "basic", "token", "digest"]
            .iter()
            .any(|scheme| first.eq_ignore_ascii_case(scheme))
        {
            let next = skip_blanks(bytes, value_stop);
            if next > value_stop {
                value_stop = value_end(bytes, next).max(value_stop);
            }
        }
        out.push_str(&input[copied..value_start]);
        out.push_str(MASK);
        copied = value_stop;
        i = resume.max(value_stop);
    }
    out.push_str(&input[copied..]);
    out
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'+' | b'/' | b'=' | b'-')
}

fn redact_bearer(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut i) = (0, 0);
    while i + 6 < bytes.len() {
        let boundary = i == 0 || !is_key_byte(bytes[i - 1]);
        if boundary && bytes[i..i + 6].eq_ignore_ascii_case(b"bearer") {
            let start = skip_blanks(bytes, i + 6);
            let mut end = start;
            while end < bytes.len() && is_token_byte(bytes[end]) {
                end += 1;
            }
            if start > i + 6 && end - start >= 8 && !input[start..].starts_with("[REDACTED") {
                out.push_str(&input[copied..start]);
                out.push_str(MASK);
                copied = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&input[copied..]);
    out
}

// (prefix, minimum characters after the prefix)
const TOKEN_PREFIXES: [(&str, usize); 17] = [
    ("ghp_", 20),
    ("gho_", 20),
    ("ghu_", 20),
    ("ghs_", 20),
    ("ghr_", 20),
    ("github_pat_", 20),
    ("glpat-", 20),
    ("sk-", 20),
    ("sk_live_", 16),
    ("sk_test_", 16),
    ("rk_live_", 16),
    ("xoxb-", 10),
    ("xoxp-", 10),
    ("xoxa-", 10),
    ("xoxs-", 10),
    ("npm_", 30),
    ("hf_", 30),
];

fn looks_like_credential(word: &str) -> bool {
    if TOKEN_PREFIXES
        .iter()
        .any(|(prefix, rest)| word.starts_with(prefix) && word.len() - prefix.len() >= *rest)
    {
        return true;
    }
    let bytes = word.as_bytes();
    let aws = bytes.len() == 20
        && (word.starts_with("AKIA") || word.starts_with("ASIA"))
        && bytes.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    let google = word.starts_with("AIza") && bytes.len() >= 35;
    let jwt = word.starts_with("eyJ")
        && bytes.len() >= 30
        && word.split('.').count() == 3
        && word.split('.').all(|part| !part.is_empty());
    let hex64 = bytes.len() == 64 && bytes.iter().all(u8::is_ascii_hexdigit);
    aws || google || jwt || hex64
}

fn redact_known_tokens(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut i) = (0, 0);
    while i < bytes.len() {
        if !is_key_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_key_byte(bytes[i]) {
            i += 1;
        }
        // Sentence punctuation is part of the key alphabet but not of a token.
        let stop = start + input[start..i].trim_end_matches('.').len();
        if looks_like_credential(&input[start..stop]) {
            out.push_str(&input[copied..start]);
            out.push_str(MASK);
            copied = stop;
        }
    }
    out.push_str(&input[copied..]);
    out
}
