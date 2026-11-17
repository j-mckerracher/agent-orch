//! Orchestrator notes: the pure rules (DPES-1105 phase 5b).
//!
//! A note is one short line the user writes to remind the orchestrator of a durable fact. The
//! snapshot lists notes as data, never as instructions, and only the user writes them. Every
//! rule here refuses rather than strips, so the user sees what was not accepted. The note
//! text is kept out of every log and error: a refusal is a stable code.

/// Most notes one scope holds.
pub const MAX_NOTES_PER_SCOPE: usize = 10;
/// Longest note, in bytes. Migration 0050 enforces the same limit.
pub const MAX_NOTE_BYTES: usize = 200;

/// Shortest run of base64 or hex characters that reads as a token.
const TOKEN_RUN_CHARS: usize = 32;
/// Word starts that name a credential. `sk-` and the others only count at the start of a word.
const TOKEN_PREFIXES: [&str; 13] = [
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "sk-",
    "xoxb-",
    "xoxp-",
    "xox-",
    "glpat-",
    "AIza",
    "npm_",
    "eyJ",
];
/// Word starts that name a relative or home-relative path, or a UNC share.
const PATH_PREFIXES: [&str; 5] = ["./", "../", "$HOME", "%USERPROFILE%", "\\\\"];

/// Why a note or a note list was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteRefusal {
    /// More than [`MAX_NOTES_PER_SCOPE`] notes.
    TooMany,
    /// Blank after trimming.
    Empty,
    /// More than [`MAX_NOTE_BYTES`] bytes.
    TooLong,
    /// A newline, tab or other control character.
    ControlCharacter,
    /// Looks like a path, a URL with credentials or a token.
    LooksSensitive,
}

impl NoteRefusal {
    /// Stable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooMany => "too_many",
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::ControlCharacter => "control_character",
            Self::LooksSensitive => "looks_sensitive",
        }
    }
}

/// Validates one note. Refuses rather than strips: the user sees what was not accepted.
pub fn check_note(text: &str) -> Result<(), NoteRefusal> {
    if text.trim().is_empty() {
        return Err(NoteRefusal::Empty);
    }
    if text.len() > MAX_NOTE_BYTES {
        return Err(NoteRefusal::TooLong);
    }
    if text.chars().any(char::is_control) {
        return Err(NoteRefusal::ControlCharacter);
    }
    // A zero-width or bidi character inside a token must not hide it from the checks.
    if looks_sensitive(&strip_invisible(text)) {
        return Err(NoteRefusal::LooksSensitive);
    }
    Ok(())
}

/// Validates a whole scope's list: its length, then each note in order.
pub fn check_notes(notes: &[String]) -> Result<(), NoteRefusal> {
    if notes.len() > MAX_NOTES_PER_SCOPE {
        return Err(NoteRefusal::TooMany);
    }
    notes.iter().try_for_each(|note| check_note(note))
}

/// `text` without the characters that render as nothing (zero-width, bidi, soft hyphen,
/// line and paragraph separators, tag characters, the joiners).
fn strip_invisible(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(c,
                '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
                | '\u{00ad}'
                | '\u{061c}'
                | '\u{e0000}'..='\u{e007f}')
        })
        .collect()
}

/// True for an absolute path (`/Users/`, `/home/`, any word that starts with `/`, a drive
/// letter), `~/`, a relative or home path (`./`, `../`, `$HOME`, `%USERPROFILE%`, `\\host`), a
/// URL with credentials, or a token-shaped string (`ghp_`, `github_pat_`, `sk-`, `glpat-`,
/// `AIza`, `npm_`, `eyJ`, `xox` plus a letter and `-`, `AKIA`, a 32 or longer hex or base64
/// run).
fn looks_sensitive(text: &str) -> bool {
    if text.contains("~/") || text.contains("/Users/") || text.contains("/home/") {
        return true;
    }
    if has_url_credentials(text) || has_token_run(text) {
        return true;
    }
    text.split_whitespace().any(|word| {
        let word =
            word.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';'));
        let mut chars = word.chars();
        let drive = matches!(
            (chars.next(), chars.next(), chars.next()),
            (Some(letter), Some(':'), Some('/' | '\\')) if letter.is_ascii_alphabetic()
        );
        drive
            || (word.len() > 1 && word.starts_with('/'))
            || TOKEN_PREFIXES.iter().any(|prefix| word.starts_with(prefix))
            || PATH_PREFIXES.iter().any(|prefix| word.starts_with(prefix))
            || is_slack_token_start(word)
            || (word.starts_with("AKIA") && word.len() >= 16)
    })
}

/// `xox`, one letter, then `-`: the Slack token family (`xoxb-`, `xoxa-`, `xoxs-`).
fn is_slack_token_start(word: &str) -> bool {
    let mut chars = word.strip_prefix("xox").unwrap_or("").chars();
    matches!((chars.next(), chars.next()), (Some(c), Some('-')) if c.is_ascii_alphabetic())
}

/// A `scheme://user:pass@host` shape: an `@` between `://` and the next `/`.
fn has_url_credentials(text: &str) -> bool {
    text.match_indices("://").any(|(index, _)| {
        let rest = &text[index + 3..];
        let authority = rest.split(['/', ' ']).next().unwrap_or("");
        authority.contains('@')
    })
}

/// A run of [`TOKEN_RUN_CHARS`] or more base64 characters.
fn has_token_run(text: &str) -> bool {
    let mut run = 0;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=') {
            run += 1;
            if run >= TOKEN_RUN_CHARS {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_note_refuses_empty_long_control_and_sensitive() {
        assert_eq!(check_note(""), Err(NoteRefusal::Empty));
        assert_eq!(check_note("   "), Err(NoteRefusal::Empty));
        assert_eq!(check_note(&"a ".repeat(101)), Err(NoteRefusal::TooLong));
        assert_eq!(check_note("two\nlines"), Err(NoteRefusal::ControlCharacter));
        assert_eq!(check_note("tab\there"), Err(NoteRefusal::ControlCharacter));
        // Built at run time so no key-shaped literal sits in the source for a scanner to flag.
        let aws_shaped = format!("key AKIA{}", "ABCDEFGHIJKLMNOP");
        let url_with_credentials = format!("clone https://user:{}@example.com/repo", "secret");
        assert_eq!(check_note(&aws_shaped), Err(NoteRefusal::LooksSensitive));
        for sensitive in [
            "the repo is at /Users/someone/code",
            "see /home/build/out",
            "C:\\work\\repo",
            "d:/work/repo",
            "keep it in ~/code",
            "use /etc/hosts",
            &url_with_credentials,
            "token ghp_abcdefghijklmnop",
            "token github_pat_abcdefghijk",
            "key sk-abcdefghijkl",
            "hash 0123456789abcdef0123456789abcdef",
            "blob QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo0MTIz",
        ] {
            assert_eq!(
                check_note(sensitive),
                Err(NoteRefusal::LooksSensitive),
                "{sensitive}"
            );
        }
        for plain in [
            "The release branch is release/2026-12",
            "Ask before touching the task-queue",
            "Standup is at 9, use and/or wisely",
            "See https://example.com/docs for the plan",
        ] {
            assert_eq!(check_note(plain), Ok(()), "{plain}");
        }
    }

    #[test]
    fn a_note_with_a_provider_token_prefix_or_a_relative_path_is_refused() {
        for sensitive in [
            "token glpat-abcdefgh",
            "key AIzaSyABCDEF",
            "token npm_abcdefgh",
            "jwt eyJhbGciOi",
            "slack xoxa-12345",
            "slack xoxs-12345",
            "see ./config/local",
            "see ../secrets",
            "in $HOME/work",
            "in %USERPROFILE%\\work",
            "share \\\\host\\x",
            "quoted \"./here\"",
        ] {
            assert_eq!(
                check_note(sensitive),
                Err(NoteRefusal::LooksSensitive),
                "{sensitive}"
            );
        }
        for plain in [
            "Use and/or here",
            "Docs at https://example.com/docs",
            "A a-b c-d style note",
            "The foo_bar and npm install step",
            "Fox is a fox, not xox",
        ] {
            assert_eq!(check_note(plain), Ok(()), "{plain}");
        }
    }

    #[test]
    fn a_note_with_a_zero_width_character_inside_a_token_is_refused() {
        let token = format!("ghp_{}", "a".repeat(36));
        for invisible in ['\u{200b}', '\u{200d}', '\u{2060}', '\u{202e}', '\u{feff}'] {
            let note = format!("token {}{invisible}{}", &token[..10], &token[10..]);
            assert_eq!(
                check_note(&note),
                Err(NoteRefusal::LooksSensitive),
                "{invisible:?}"
            );
        }
        let split_run = format!("{}\u{200b}{}", "a".repeat(20), "b".repeat(20));
        assert_eq!(check_note(&split_run), Err(NoteRefusal::LooksSensitive));
    }

    #[test]
    fn check_note_counts_bytes_not_chars() {
        // Two bytes per character: 100 characters fit, 101 do not.
        assert_eq!(check_note(&"é".repeat(100)), Ok(()));
        assert_eq!(check_note(&"é".repeat(101)), Err(NoteRefusal::TooLong));
        // 195 bytes of short words plus one five-byte word is exactly the limit (a run of
        // 200 letters would read as a token and be refused for that reason instead).
        let exact = format!("{}words", "word ".repeat(39));
        assert_eq!(exact.len(), MAX_NOTE_BYTES);
        assert_eq!(check_note(&exact), Ok(()));
    }

    #[test]
    fn check_notes_refuses_an_eleventh_note_and_reports_the_first_bad_one() {
        let ten = vec!["fine".to_owned(); MAX_NOTES_PER_SCOPE];
        assert_eq!(check_notes(&ten), Ok(()));
        let mut eleven = ten.clone();
        eleven.push("fine".to_owned());
        assert_eq!(check_notes(&eleven), Err(NoteRefusal::TooMany));
        let mut bad = ten;
        bad[3] = String::new();
        assert_eq!(check_notes(&bad), Err(NoteRefusal::Empty));
        assert_eq!(NoteRefusal::LooksSensitive.code(), "looks_sensitive");
    }
}
