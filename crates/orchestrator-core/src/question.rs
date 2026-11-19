//! User decisions: the pure text rules (DPES-1108 phase 3).
//!
//! A question is a decision held for the user, bound to one task. The orchestrator raises one
//! with `decision.ask`; a later phase lets a worker raise one. Only the user closes it, with
//! their own words (an answer) or with evidence that it no longer matters (a reconcile). Every
//! rule here refuses rather than strips: an answer is stored exactly as the user wrote it.
//! Migration 0063 enforces the same limits.

use crate::orchestrator_action::is_disallowed_objective_char;

/// Longest question, in bytes.
pub const MAX_QUESTION_BYTES: usize = 1024;
/// Most options one question offers.
pub const MAX_OPTIONS: usize = 6;
/// Longest option, in bytes.
pub const MAX_OPTION_BYTES: usize = 200;
/// Longest key, in characters: lowercase ASCII letters, digits and `-`.
pub const MAX_KEY_CHARS: usize = 64;
/// Longest answer or reconcile evidence, in bytes.
pub const MAX_REPLY_BYTES: usize = 4096;

/// The field a question or a reply was refused for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionField {
    Question,
    Options,
    Key,
    /// The answer or the reconcile evidence.
    Reply,
}

impl QuestionField {
    /// The wire name of the field.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Options => "options",
            Self::Key => "key",
            Self::Reply => "reply",
        }
    }
}

/// Checks what a question is raised with: the text, up to [`MAX_OPTIONS`] options and an
/// optional key. Text is non-blank, bounded and free of control, bidi and invisible characters
/// other than tab and line breaks; an option is one line.
pub fn check_question(
    question: &str,
    options: &[String],
    key: Option<&str>,
) -> Result<(), QuestionField> {
    if !is_text(question, MAX_QUESTION_BYTES) {
        return Err(QuestionField::Question);
    }
    let one_line =
        |option: &String| is_text(option, MAX_OPTION_BYTES) && !option.contains(['\n', '\r']);
    if options.len() > MAX_OPTIONS || !options.iter().all(one_line) {
        return Err(QuestionField::Options);
    }
    if let Some(key) = key {
        let shaped = (1..=MAX_KEY_CHARS).contains(&key.len())
            && key
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        if !shaped {
            return Err(QuestionField::Key);
        }
    }
    Ok(())
}

/// Checks an answer or reconcile evidence. It is kept verbatim, so nothing is trimmed: it must
/// hold a visible character and pass the same character rule as a question.
pub fn check_reply(text: &str) -> Result<(), QuestionField> {
    if is_text(text, MAX_REPLY_BYTES) {
        Ok(())
    } else {
        Err(QuestionField::Reply)
    }
}

fn is_text(text: &str, max_bytes: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= max_bytes
        && !text.chars().any(is_disallowed_objective_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_needs_bounded_visible_text() {
        assert_eq!(check_question("Use A or B?", &[], None), Ok(()));
        assert_eq!(
            check_question(" \n", &[], None),
            Err(QuestionField::Question)
        );
        let long = "x".repeat(MAX_QUESTION_BYTES + 1);
        assert_eq!(
            check_question(&long, &[], None),
            Err(QuestionField::Question)
        );
        assert_eq!(
            check_question("a\u{202E}b", &[], None),
            Err(QuestionField::Question)
        );
        assert_eq!(
            check_question("a\u{200B}b", &[], None),
            Err(QuestionField::Question)
        );
    }

    #[test]
    fn options_are_few_and_one_line_each() {
        let two = vec!["A".to_owned(), "B".to_owned()];
        assert_eq!(check_question("Which?", &two, None), Ok(()));
        let many = vec!["A".to_owned(); MAX_OPTIONS + 1];
        assert_eq!(
            check_question("Which?", &many, None),
            Err(QuestionField::Options)
        );
        let lines = vec!["A\nB".to_owned()];
        assert_eq!(
            check_question("Which?", &lines, None),
            Err(QuestionField::Options)
        );
        assert_eq!(
            check_question("Which?", &[String::new()], None),
            Err(QuestionField::Options)
        );
    }

    #[test]
    fn a_key_is_a_short_lowercase_slug() {
        assert_eq!(check_question("Q?", &[], Some("db-choice-2")), Ok(()));
        let long = "k".repeat(MAX_KEY_CHARS + 1);
        for bad in ["", "Upper", "has space", "under_score", long.as_str()] {
            assert_eq!(
                check_question("Q?", &[], Some(bad)),
                Err(QuestionField::Key),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_reply_is_kept_verbatim_but_must_be_visible() {
        assert_eq!(check_reply("  use B, not A\n"), Ok(()));
        assert_eq!(check_reply("\t\n "), Err(QuestionField::Reply));
        assert_eq!(
            check_reply(&"x".repeat(MAX_REPLY_BYTES + 1)),
            Err(QuestionField::Reply)
        );
        assert_eq!(check_reply("ok\u{0007}"), Err(QuestionField::Reply));
    }
}
