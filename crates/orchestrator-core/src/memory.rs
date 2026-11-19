//! Preferences and learnings: the pure rules (DPES-1108 phase 6).
//!
//! The orchestrator's memory is three kinds of document, each a list of keyed one-line entries
//! with a byte budget and a last-confirmed time:
//!
//! - **Shared preferences** (`preference` in the `global` scope): the user's standing defaults.
//!   Every conversation reads them, wake turns included. Only the user and the global
//!   conversation write them; a write from any other conversation is refused
//!   ([`MemoryDenial::SharedReadOnly`]) and the agent is told to report it up instead.
//! - **Local preferences** (`preference` in a `workstream:` or `project:` scope): the same for one
//!   workstream conversation or one project coordinator. The user and that conversation write
//!   them.
//! - **Learnings** (`learning` in any scope): operational facts the orchestrator of that scope
//!   recorded. Data, not the user's wishes. They go stale when unconfirmed for
//!   [`LEARNING_STALE_AFTER_DAYS`] days; a stale entry is listed for the user, never deleted.
//!
//! A write names a key, so writing the same key again updates the entry in place. Every rule
//! here refuses rather than strips, so the user sees what was not accepted, and a refusal is a
//! stable code that never holds the entry text.

use serde::{Deserialize, Serialize};

/// Longest entry key, in bytes.
pub const MAX_KEY_BYTES: usize = 40;
/// Longest entry text, in bytes.
pub const MAX_TEXT_BYTES: usize = 280;
/// Budget of the shared preferences, in bytes of key plus text over all entries.
pub const SHARED_PREFERENCES_BUDGET: usize = 2048;
/// Budget of one workstream's or one project's preferences.
pub const LOCAL_PREFERENCES_BUDGET: usize = 2048;
/// Budget of one scope's learnings.
pub const LEARNINGS_BUDGET: usize = 1024;
/// Days after which an unconfirmed learning is listed as stale. Preferences never go stale:
/// the user wrote or approved them, and only the user retires them.
pub const LEARNING_STALE_AFTER_DAYS: u64 = 180;
/// Longest workstream or project id a scope may name, in bytes.
const MAX_SCOPE_ID_BYTES: usize = 4096;
const DAY_MS: u64 = 86_400_000;

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

/// What a document holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryKind {
    /// The user's standing defaults.
    Preference,
    /// Operational facts the orchestrator recorded.
    Learning,
}

impl MemoryKind {
    /// Stored and wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Learning => "learning",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "preference" => Some(Self::Preference),
            "learning" => Some(Self::Learning),
            _ => None,
        }
    }
}

/// Whose document: the global scope, one workstream conversation or one project coordinator.
/// The text form is the conversation's scope: `global`, `workstream:<id>` or `project:<id>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MemoryScope {
    Global,
    Workstream(String),
    Project(String),
}

impl MemoryScope {
    /// Parses `global`, `workstream:<id>` or `project:<id>`. The id is not empty, has no control
    /// character and is at most 4,096 bytes.
    pub fn parse(value: &str) -> Option<Self> {
        if value == "global" {
            return Some(Self::Global);
        }
        let valid = |id: &str| {
            !id.is_empty() && id.len() <= MAX_SCOPE_ID_BYTES && !id.chars().any(char::is_control)
        };
        if let Some(id) = value.strip_prefix("workstream:") {
            return valid(id).then(|| Self::Workstream(id.to_owned()));
        }
        if let Some(id) = value.strip_prefix("project:") {
            return valid(id).then(|| Self::Project(id.to_owned()));
        }
        None
    }

    /// The text form [`Self::parse`] reads.
    pub fn as_string(&self) -> String {
        match self {
            Self::Global => "global".to_owned(),
            Self::Workstream(id) => format!("workstream:{id}"),
            Self::Project(id) => format!("project:{id}"),
        }
    }
}

/// The byte budget of one document.
pub const fn budget(kind: MemoryKind, scope: &MemoryScope) -> usize {
    match (kind, scope) {
        (MemoryKind::Preference, MemoryScope::Global) => SHARED_PREFERENCES_BUDGET,
        (MemoryKind::Preference, _) => LOCAL_PREFERENCES_BUDGET,
        (MemoryKind::Learning, _) => LEARNINGS_BUDGET,
    }
}

/// The document a `memory.set` action names, relative to the conversation that wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDocument {
    /// The shared preferences.
    Shared,
    /// This conversation's own preferences.
    Local,
    /// This conversation's own learnings.
    Learnings,
}

impl MemoryDocument {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Local => "local",
            Self::Learnings => "learnings",
        }
    }
}

/// Why a conversation may not write the document it named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryDenial {
    /// The shared preferences are read-only outside the global conversation. The agent reports
    /// the preference up (to the user, or to the global orchestrator) instead.
    SharedReadOnly,
    /// The global conversation has no local preferences; its own are the shared ones.
    NoLocalScope,
}

impl MemoryDenial {
    pub const ALL: [Self; 2] = [Self::SharedReadOnly, Self::NoLocalScope];

    /// Stable key, at most 64 bytes, stored as a proposal denial.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SharedReadOnly => "shared_preferences_read_only",
            Self::NoLocalScope => "no_local_preferences_here",
        }
    }
}

/// The document a conversation in `writer` reaches when it names `document`. This is the
/// read-only guard for the shared preferences: the daemon runs it when the proposal is stored and
/// again when it executes.
pub fn resolve(
    document: MemoryDocument,
    writer: &MemoryScope,
) -> Result<(MemoryKind, MemoryScope), MemoryDenial> {
    match (document, writer) {
        (MemoryDocument::Shared, MemoryScope::Global) => {
            Ok((MemoryKind::Preference, MemoryScope::Global))
        }
        (MemoryDocument::Shared, _) => Err(MemoryDenial::SharedReadOnly),
        (MemoryDocument::Local, MemoryScope::Global) => Err(MemoryDenial::NoLocalScope),
        (MemoryDocument::Local, scope) => Ok((MemoryKind::Preference, scope.clone())),
        (MemoryDocument::Learnings, scope) => Ok((MemoryKind::Learning, scope.clone())),
    }
}

/// Why an entry was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryRefusal {
    /// The key is empty, too long or has a character other than `a-z`, `0-9` and `-`.
    BadKey,
    /// Blank after trimming.
    Empty,
    /// More than [`MAX_TEXT_BYTES`] bytes.
    TooLong,
    /// A newline, tab or other control character, or a bidi or zero-width character.
    ControlCharacter,
    /// Looks like a path, a URL with credentials or a token.
    LooksSensitive,
}

impl EntryRefusal {
    /// Stable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::BadKey => "bad_key",
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::ControlCharacter => "control_character",
            Self::LooksSensitive => "looks_sensitive",
        }
    }
}

/// A key is 1 to [`MAX_KEY_BYTES`] bytes of `a-z`, `0-9` and `-`, starting with a letter or digit.
pub fn check_key(key: &str) -> Result<(), EntryRefusal> {
    let shaped = key
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if key.is_empty() || key.len() > MAX_KEY_BYTES || !shaped || key.starts_with('-') {
        return Err(EntryRefusal::BadKey);
    }
    Ok(())
}

/// Validates one entry's text: one line, not blank, at most [`MAX_TEXT_BYTES`] bytes, and
/// nothing that reads as a path or a secret.
pub fn check_text(text: &str) -> Result<(), EntryRefusal> {
    if text.trim().is_empty() {
        return Err(EntryRefusal::Empty);
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(EntryRefusal::TooLong);
    }
    // A bidi or zero-width character can make a prompt line read differently than the
    // Settings view shows it, so it is refused with the controls.
    if text.chars().any(char::is_control) || strip_invisible(text).len() != text.len() {
        return Err(EntryRefusal::ControlCharacter);
    }
    if looks_sensitive(text) {
        return Err(EntryRefusal::LooksSensitive);
    }
    Ok(())
}

/// Validates a whole entry: its key, then its text.
pub fn check_entry(key: &str, text: &str) -> Result<(), EntryRefusal> {
    check_key(key)?;
    check_text(text)
}

/// The bytes one entry takes from its document's budget: its key and its text, both of which
/// the prompt carries.
pub const fn entry_bytes(key: &str, text: &str) -> usize {
    key.len() + text.len()
}

/// A write that would leave its document over the budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverBudget {
    /// The document's size after the write.
    pub bytes: usize,
    pub budget: usize,
}

/// The document's size after a write, or the size it would have had when that is over the
/// budget. `current` is the document's size now, `replaced` the bytes of the entry the write
/// replaces (0 for a new key) and `added` the bytes of the written entry. A write that does not
/// grow the document is always allowed, so a document that is over budget (from the notes it
/// was migrated from, or a lower budget) can still be edited down.
pub fn check_budget(
    current: usize,
    replaced: usize,
    added: usize,
    budget: usize,
) -> Result<usize, OverBudget> {
    let after = current.saturating_sub(replaced) + added;
    if after <= budget || after <= current {
        Ok(after)
    } else {
        Err(OverBudget {
            bytes: after,
            budget,
        })
    }
}

/// Whether an entry is listed as stale: a learning unconfirmed for more than
/// [`LEARNING_STALE_AFTER_DAYS`]. A preference never is.
pub const fn is_stale(kind: MemoryKind, confirmed_at_ms: u64, now_ms: u64) -> bool {
    match kind {
        MemoryKind::Preference => false,
        MemoryKind::Learning => {
            now_ms.saturating_sub(confirmed_at_ms) > LEARNING_STALE_AFTER_DAYS * DAY_MS
        }
    }
}

/// True when `text` reads as a path, a URL with credentials or a token, after removing the
/// characters that render as nothing (so a zero-width or bidi character inside a token cannot
/// hide it).
pub fn looks_sensitive(text: &str) -> bool {
    looks_sensitive_visible(&strip_invisible(text))
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
fn looks_sensitive_visible(text: &str) -> bool {
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
    fn check_text_refuses_empty_long_control_and_sensitive() {
        assert_eq!(check_text(""), Err(EntryRefusal::Empty));
        assert_eq!(check_text("   "), Err(EntryRefusal::Empty));
        assert_eq!(check_text(&"a ".repeat(141)), Err(EntryRefusal::TooLong));
        assert_eq!(
            check_text("two\nlines"),
            Err(EntryRefusal::ControlCharacter)
        );
        assert_eq!(check_text("tab\there"), Err(EntryRefusal::ControlCharacter));
        // Built at run time so no key-shaped literal sits in the source for a scanner to flag.
        let aws_shaped = format!("key AKIA{}", "ABCDEFGHIJKLMNOP");
        let url_with_credentials = format!("clone https://user:{}@example.com/repo", "secret");
        assert_eq!(check_text(&aws_shaped), Err(EntryRefusal::LooksSensitive));
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
                check_text(sensitive),
                Err(EntryRefusal::LooksSensitive),
                "{sensitive}"
            );
        }
        for plain in [
            "The release branch is release/2026-12",
            "Ask before touching the task-queue",
            "Standup is at 9, use and/or wisely",
            "See https://example.com/docs for the plan",
        ] {
            assert_eq!(check_text(plain), Ok(()), "{plain}");
        }
    }

    #[test]
    fn a_text_with_a_provider_token_prefix_or_a_relative_path_is_refused() {
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
                check_text(sensitive),
                Err(EntryRefusal::LooksSensitive),
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
            assert_eq!(check_text(plain), Ok(()), "{plain}");
        }
    }

    #[test]
    fn a_text_with_a_zero_width_or_bidi_character_is_refused() {
        let token = format!("ghp_{}", "a".repeat(36));
        for invisible in ['\u{200b}', '\u{200d}', '\u{2060}', '\u{202e}', '\u{feff}'] {
            let text = format!("token {}{invisible}{}", &token[..10], &token[10..]);
            assert!(looks_sensitive(&text), "{invisible:?}");
            assert_eq!(
                check_text(&text),
                Err(EntryRefusal::ControlCharacter),
                "{invisible:?}"
            );
        }
        let split_run = format!("{}\u{200b}{}", "a".repeat(20), "b".repeat(20));
        assert!(looks_sensitive(&split_run));
        assert_eq!(
            check_text("bidi \u{202e}txet"),
            Err(EntryRefusal::ControlCharacter)
        );
    }

    #[test]
    fn check_text_counts_bytes_not_chars() {
        // Two bytes per character: 140 characters fit, 141 do not.
        assert_eq!(check_text(&"é".repeat(140)), Ok(()));
        assert_eq!(check_text(&"é".repeat(141)), Err(EntryRefusal::TooLong));
        // Short words, so no run reads as a token, exactly at the limit.
        let exact = format!("{}words", "word ".repeat(55));
        assert_eq!(exact.len(), MAX_TEXT_BYTES);
        assert_eq!(check_text(&exact), Ok(()));
    }

    #[test]
    fn a_key_is_a_short_lowercase_slug() {
        for good in [
            "release-branch",
            "a",
            "pr-size-2",
            &"k".repeat(MAX_KEY_BYTES),
        ] {
            assert_eq!(check_key(good), Ok(()), "{good}");
        }
        for bad in [
            "",
            "-lead",
            "Upper",
            "with space",
            "under_score",
            "dot.ted",
            "é",
            &"k".repeat(MAX_KEY_BYTES + 1),
        ] {
            assert_eq!(check_key(bad), Err(EntryRefusal::BadKey), "{bad}");
        }
        assert_eq!(check_entry("Bad", "fine"), Err(EntryRefusal::BadKey));
        assert_eq!(check_entry("ok", ""), Err(EntryRefusal::Empty));
        assert_eq!(EntryRefusal::LooksSensitive.code(), "looks_sensitive");
    }

    #[test]
    fn scopes_parse_and_print_the_conversation_form() {
        for text in ["global", "workstream:ws-1", "project:/srv/repo"] {
            assert_eq!(MemoryScope::parse(text).unwrap().as_string(), text);
        }
        for bad in [
            "",
            "elsewhere",
            "workstream:",
            "project:",
            "project:a\nb",
            "Global",
        ] {
            assert_eq!(MemoryScope::parse(bad), None, "{bad:?}");
        }
        let long = format!("project:{}", "p".repeat(MAX_SCOPE_ID_BYTES + 1));
        assert_eq!(MemoryScope::parse(&long), None);
        assert_eq!(
            MemoryKind::parse("preference"),
            Some(MemoryKind::Preference)
        );
        assert_eq!(MemoryKind::parse("learning"), Some(MemoryKind::Learning));
        assert_eq!(MemoryKind::parse("note"), None);
    }

    #[test]
    fn only_the_global_conversation_writes_shared_preferences() {
        let global = MemoryScope::Global;
        let workstream = MemoryScope::Workstream("ws-1".to_owned());
        let project = MemoryScope::Project("/srv/repo".to_owned());
        assert_eq!(
            resolve(MemoryDocument::Shared, &global),
            Ok((MemoryKind::Preference, MemoryScope::Global))
        );
        // A coordinator or a workstream conversation is refused, with the denial that tells
        // it to report the preference up.
        for writer in [&workstream, &project] {
            assert_eq!(
                resolve(MemoryDocument::Shared, writer),
                Err(MemoryDenial::SharedReadOnly)
            );
            assert_eq!(
                resolve(MemoryDocument::Local, writer),
                Ok((MemoryKind::Preference, writer.clone()))
            );
            assert_eq!(
                resolve(MemoryDocument::Learnings, writer),
                Ok((MemoryKind::Learning, writer.clone()))
            );
        }
        assert_eq!(
            resolve(MemoryDocument::Local, &global),
            Err(MemoryDenial::NoLocalScope)
        );
        assert_eq!(
            resolve(MemoryDocument::Learnings, &global),
            Ok((MemoryKind::Learning, MemoryScope::Global))
        );
        for denial in MemoryDenial::ALL {
            assert!((1..=64).contains(&denial.as_str().len()));
        }
    }

    #[test]
    fn a_write_may_not_grow_a_document_past_its_budget() {
        assert_eq!(budget(MemoryKind::Preference, &MemoryScope::Global), 2048);
        let project = MemoryScope::Project("p".to_owned());
        assert_eq!(budget(MemoryKind::Preference, &project), 2048);
        assert_eq!(budget(MemoryKind::Learning, &MemoryScope::Global), 1024);
        assert_eq!(check_budget(1000, 0, 24, 1024), Ok(1024));
        assert_eq!(
            check_budget(1000, 0, 25, 1024),
            Err(OverBudget {
                bytes: 1025,
                budget: 1024
            })
        );
        // An in-place update counts only the difference.
        assert_eq!(check_budget(1024, 100, 100, 1024), Ok(1024));
        assert_eq!(
            check_budget(1024, 100, 101, 1024),
            Err(OverBudget {
                bytes: 1025,
                budget: 1024
            })
        );
        // A document already over budget may still shrink or stay the same, never grow.
        assert_eq!(check_budget(2100, 200, 150, 2048), Ok(2050));
        assert_eq!(check_budget(2100, 200, 200, 2048), Ok(2100));
        assert!(check_budget(2100, 200, 201, 2048).is_err());
        assert_eq!(entry_bytes("key", "text"), 7);
    }

    #[test]
    fn only_an_unconfirmed_learning_goes_stale() {
        let day = DAY_MS;
        let now = 1_000 * day;
        let limit = now - LEARNING_STALE_AFTER_DAYS * day;
        assert!(!is_stale(MemoryKind::Learning, limit, now));
        assert!(is_stale(MemoryKind::Learning, limit - 1, now));
        assert!(!is_stale(MemoryKind::Preference, 0, now));
        // A clock that went backwards never marks an entry stale.
        assert!(!is_stale(MemoryKind::Learning, now + day, now));
    }
}
