//! The worker status channel: the pure rules (DPES-1108 phase 4).
//!
//! A task worker is a headless CLI run with no input channel. It reports by appending one line
//! per event to a status file the brief names, in the daemon's data directory outside the
//! worktree. The daemon tails that file while the run lives and once more at exit. This module
//! parses the file; it reads no file and keeps no clock.
//!
//! Every line is untrusted worker output. A line is refused, never repaired: one that is too
//! long, is not UTF-8, holds a control, bidi or invisible character, names an unknown verb or a
//! malformed key is dropped and only its line number and refusal code are logged. A note is
//! data for the user and the orchestrator, never an instruction to either.
//!
//! Line form: `<verb> [key=<slug>] <note>`, where the key and the note are optional except that
//! `needs-decision` needs its question as the note.

use sha2::{Digest, Sha256};

use crate::question::{check_question, MAX_KEY_CHARS, MAX_QUESTION_BYTES};
use crate::task::WaitReason;

/// File name of the status file inside its per-attempt directory.
pub const STATUS_FILE_NAME: &str = "status";
/// Longest line, in bytes, without its line break: the verb, a key and a note of
/// [`MAX_QUESTION_BYTES`].
pub const MAX_LINE_BYTES: usize = MAX_QUESTION_BYTES + 96;
/// Most lines the daemon reads from one attempt's file. Later lines are ignored.
pub const MAX_LINES_PER_ATTEMPT: u32 = 200;
/// Most lines one read takes. The daemon reads about once a second while a run lives, so this
/// is the rate limit; lines over it wait for the next read.
pub const MAX_LINES_PER_READ: u32 = 20;
/// Most bytes the daemon reads from one status file.
pub const MAX_FILE_BYTES: u64 = (MAX_LINES_PER_ATTEMPT as u64) * (MAX_LINE_BYTES as u64 + 1);
/// Most questions one wait raises. Later `needs-decision` lines of the same run are recorded
/// as events but raise no question, so the answers of one resume stay within one command line.
pub const MAX_QUESTIONS_PER_WAIT: usize = 3;

/// What a status line reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusVerb {
    Working,
    NeedsDecision,
    Blocked,
    Done,
    Failed,
}

impl StatusVerb {
    pub const ALL: [Self; 5] = [
        Self::Working,
        Self::NeedsDecision,
        Self::Blocked,
        Self::Done,
        Self::Failed,
    ];

    /// The word a worker writes, which is also the stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NeedsDecision => "needs-decision",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|verb| verb.as_str() == word)
    }
}

/// One accepted line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusLine {
    pub verb: StatusVerb,
    pub key: Option<String>,
    pub note: Option<String>,
}

/// Why a line was refused. The code is logged; the text never is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineError {
    TooLong,
    NotText,
    UnknownVerb,
    BadKey,
    /// A `needs-decision` line without a question, or a note the question rules refuse.
    BadNote,
}

impl LineError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooLong => "too_long",
            Self::NotText => "not_text",
            Self::UnknownVerb => "unknown_verb",
            Self::BadKey => "bad_key",
            Self::BadNote => "bad_note",
        }
    }
}

/// Parses one line, without its line break. A trailing carriage return is allowed.
pub fn parse_line(line: &str) -> Result<StatusLine, LineError> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.len() > MAX_LINE_BYTES {
        return Err(LineError::TooLong);
    }
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    let verb = StatusVerb::parse(verb).ok_or(LineError::UnknownVerb)?;
    let mut rest = rest.trim_start_matches(' ');
    let mut key = None;
    if let Some(after) = rest.strip_prefix("[key=") {
        let (slug, tail) = after.split_once(']').ok_or(LineError::BadKey)?;
        let shaped = (1..=MAX_KEY_CHARS).contains(&slug.len())
            && slug
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        if !shaped {
            return Err(LineError::BadKey);
        }
        key = Some(slug.to_owned());
        rest = tail.trim_start_matches(' ');
    }
    let note = rest.trim_end();
    if note.contains('\r') {
        return Err(LineError::NotText);
    }
    let note = (!note.is_empty()).then(|| note.to_owned());
    // The question rules are the store's, so a parsed question can always be raised.
    match &note {
        Some(text) if check_question(text, &[], key.as_deref()).is_err() => {
            return Err(LineError::BadNote)
        }
        None if verb == StatusVerb::NeedsDecision => return Err(LineError::BadNote),
        _ => {}
    }
    Ok(StatusLine { verb, key, note })
}

/// The lines one read takes from a status file.
#[derive(Debug, PartialEq, Eq)]
pub struct Read {
    /// Each line with its 1-based number in the file. Blank lines are skipped, not listed.
    pub lines: Vec<(u32, Result<StatusLine, LineError>)>,
    /// The number of the last line this read consumed, blank or not. The next read starts
    /// after it.
    pub through: u32,
}

/// Reads the lines after line `after` of `content`. Only lines that end with a line break are
/// taken, unless `last` is set (the run has exited, so a final line without one is complete).
/// At most [`MAX_LINES_PER_READ`] lines are taken, and none past [`MAX_LINES_PER_ATTEMPT`].
pub fn read_lines(content: &[u8], after: u32, last: bool) -> Read {
    let mut lines = Vec::new();
    let mut through = after;
    let mut pieces = content.split_inclusive(|byte| *byte == b'\n');
    let mut number = 0_u32;
    for piece in pieces.by_ref() {
        number += 1;
        if number <= after {
            continue;
        }
        if number > MAX_LINES_PER_ATTEMPT || number - after > MAX_LINES_PER_READ {
            break;
        }
        let complete = piece.ends_with(b"\n");
        if !complete && !last {
            break;
        }
        through = number;
        let body = piece.strip_suffix(b"\n").unwrap_or(piece);
        if body.iter().all(|byte| matches!(byte, b' ' | b'\t' | b'\r')) {
            continue;
        }
        let parsed = std::str::from_utf8(body)
            .map_err(|_| LineError::NotText)
            .and_then(parse_line);
        lines.push((number, parsed));
    }
    Read { lines, through }
}

/// How a run that exited cleanly is settled, from the lines it reported. A wait wins over any
/// claim: a `needs-decision` line stays a decision even if a `done` follows it, and a decision
/// wins over a blocker. With no wait, a `failed` claim fails the attempt, since a worker that
/// says it failed is believed in the safe direction. A `done` claim decides nothing: the
/// candidate commit and verification do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settlement {
    Wait(WaitReason),
    ReportedFailure(Option<String>),
    Ordinary,
}

pub fn settle(lines: &[StatusLine]) -> Settlement {
    let has = |verb| lines.iter().any(|line| line.verb == verb);
    if has(StatusVerb::NeedsDecision) {
        return Settlement::Wait(WaitReason::Decision);
    }
    if has(StatusVerb::Blocked) {
        return Settlement::Wait(WaitReason::Blocker);
    }
    match lines
        .iter()
        .rev()
        .find(|line| matches!(line.verb, StatusVerb::Done | StatusVerb::Failed))
    {
        Some(line) if line.verb == StatusVerb::Failed => {
            Settlement::ReportedFailure(line.note.clone())
        }
        _ => Settlement::Ordinary,
    }
}

/// The id of the question a `needs-decision` line raises: stable for the attempt and line, so
/// reading the file again raises nothing twice. 39 characters, under the store's 128.
pub fn question_id(attempt_id: &str, line: u32) -> String {
    let digest = Sha256::digest(format!("{attempt_id}\n{line}").as_bytes());
    let hex: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("worker-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(verb: StatusVerb, note: Option<&str>) -> StatusLine {
        StatusLine {
            verb,
            key: None,
            note: note.map(str::to_owned),
        }
    }

    #[test]
    fn a_line_has_a_verb_an_optional_key_and_an_optional_note() {
        assert_eq!(
            parse_line("needs-decision [key=db-choice] Use SQLite or Postgres?"),
            Ok(StatusLine {
                verb: StatusVerb::NeedsDecision,
                key: Some("db-choice".into()),
                note: Some("Use SQLite or Postgres?".into()),
            })
        );
        assert_eq!(parse_line("done"), Ok(line(StatusVerb::Done, None)));
        assert_eq!(
            parse_line("blocked  the test database is down \r"),
            Ok(line(StatusVerb::Blocked, Some("the test database is down")))
        );
        for verb in StatusVerb::ALL {
            assert_eq!(StatusVerb::parse(verb.as_str()), Some(verb));
        }
    }

    #[test]
    fn a_line_is_refused_rather_than_repaired() {
        for (text, error) in [
            ("finished all of it", LineError::UnknownVerb),
            ("Done", LineError::UnknownVerb),
            ("", LineError::UnknownVerb),
            ("blocked [key=Not A Slug] x", LineError::BadKey),
            ("blocked [key=unclosed x", LineError::BadKey),
            ("blocked [key=] x", LineError::BadKey),
            ("needs-decision", LineError::BadNote),
            ("needs-decision [key=a]", LineError::BadNote),
            ("working \u{202E}evil", LineError::BadNote),
            ("working a\u{0007}bell", LineError::BadNote),
            ("working a\rb", LineError::NotText),
        ] {
            assert_eq!(parse_line(text), Err(error), "{text:?}");
        }
        let long = format!("working {}", "x".repeat(MAX_LINE_BYTES));
        assert_eq!(parse_line(&long), Err(LineError::TooLong));
        let long_question = format!("needs-decision {}", "q".repeat(MAX_QUESTION_BYTES + 1));
        assert_eq!(parse_line(&long_question), Err(LineError::BadNote));
    }

    #[test]
    fn a_read_takes_whole_lines_after_the_last_one_and_keeps_numbering() {
        let file = b"working reading the parser\n\nbogus line\nblocked no network\npartial";
        let read = read_lines(file, 0, false);
        assert_eq!(read.through, 4);
        assert_eq!(
            read.lines,
            vec![
                (1, Ok(line(StatusVerb::Working, Some("reading the parser")))),
                (3, Err(LineError::UnknownVerb)),
                (4, Ok(line(StatusVerb::Blocked, Some("no network")))),
            ]
        );
        // The unfinished last line waits until the run has exited.
        assert_eq!(read_lines(file, 4, false).lines, vec![]);
        let last = read_lines(file, 4, true);
        assert_eq!(last.through, 5);
        assert_eq!(last.lines, vec![(5, Err(LineError::UnknownVerb))]);
        // Invalid UTF-8 is refused as one line, not the file.
        let bytes = b"working ok\n\xff\xfe\n";
        assert_eq!(
            read_lines(bytes, 0, true).lines[1],
            (2, Err(LineError::NotText))
        );
    }

    #[test]
    fn a_read_is_bounded_per_read_and_per_attempt() {
        let file = "working step\n".repeat(MAX_LINES_PER_ATTEMPT as usize + 50);
        let first = read_lines(file.as_bytes(), 0, true);
        assert_eq!(first.lines.len(), MAX_LINES_PER_READ as usize);
        assert_eq!(first.through, MAX_LINES_PER_READ);
        let mut after = 0;
        let mut taken = 0;
        loop {
            let read = read_lines(file.as_bytes(), after, true);
            if read.through == after {
                break;
            }
            taken += read.lines.len();
            after = read.through;
        }
        assert_eq!(taken, MAX_LINES_PER_ATTEMPT as usize);
        assert_eq!(after, MAX_LINES_PER_ATTEMPT);
    }

    #[test]
    fn a_wait_wins_over_any_claim_and_only_failed_is_believed() {
        use StatusVerb::*;
        let settle_of = |verbs: &[StatusVerb]| {
            settle(
                &verbs
                    .iter()
                    .map(|verb| line(*verb, Some("note")))
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(settle_of(&[]), Settlement::Ordinary);
        assert_eq!(settle_of(&[Working, Done]), Settlement::Ordinary);
        assert_eq!(
            settle_of(&[NeedsDecision, Done]),
            Settlement::Wait(WaitReason::Decision)
        );
        assert_eq!(
            settle_of(&[Blocked, NeedsDecision]),
            Settlement::Wait(WaitReason::Decision)
        );
        assert_eq!(
            settle_of(&[Working, Blocked, Done]),
            Settlement::Wait(WaitReason::Blocker)
        );
        assert_eq!(
            settle_of(&[Failed, Done]),
            Settlement::Ordinary,
            "the last claim counts"
        );
        assert_eq!(
            settle_of(&[Done, Failed]),
            Settlement::ReportedFailure(Some("note".into()))
        );
    }

    #[test]
    fn a_question_id_is_stable_per_attempt_and_line() {
        let id = question_id("t-1-attempt-1", 3);
        assert_eq!(id, question_id("t-1-attempt-1", 3));
        assert_ne!(id, question_id("t-1-attempt-1", 4));
        assert_ne!(id, question_id("t-1-attempt-2", 3));
        assert_eq!(id.len(), 39);
        assert!(id.starts_with("worker-"));
    }
}
