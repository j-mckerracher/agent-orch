//! Work-item intake (DPES-673): turns a pasted GitHub issue reference, Jira key
//! or block of text into a [`WorkItem`] — title, ask, numbered acceptance
//! criteria and a source — that a person confirms before a task is admitted.
//!
//! [`normalize`] is pure. It never fetches anything: a bare key or reference is
//! recorded as the source and its title, ask and criteria come only from the
//! text supplied with it. When no criteria are found one is *drafted* from the
//! ask and marked `drafted: true`, so a reviewer can tell it from a stated one.
//! Every criterion starts `confirmed: false`;
//! [`crate::contract::validate_work_item`] refuses a work item until each one is
//! confirmed.
//!
//! All text is untrusted. Nothing is rendered or interpreted (no HTML, no
//! markdown beyond recognising list markers); control and bidirectional
//! formatting characters are stripped. Limits are enforced with a typed
//! [`IntakeError`]: over-limit input is refused, never silently truncated.
//! The one exception is a *drafted* criterion, which is generated text and is
//! shortened with an ellipsis when its source line is long.
//!
//! # What is recognised
//! * Source: the first non-empty line is scanned, token by token, for a Jira
//!   key (`ABC-123`), a GitHub issue reference (`owner/repo#123`) or a GitHub
//!   issues URL. The first match is the source; otherwise the source is text.
//! * Title: the first non-empty line.
//! * Criteria: `1.` / `1)` numbered items, `AC1:` items and `- [ ]` checklist
//!   items anywhere; plain `-`/`*` bullets and unmarked lines under an
//!   "Acceptance criteria" heading until the next heading or `Label:` line.
//!   An indented line directly after an item continues it.
//! * Ask: every other line after the title (the title when that leaves nothing).

use crate::contract::is_bidi_control;

/// Whole-input limit, in bytes, checked before any processing.
pub const MAX_INPUT_BYTES: usize = 20_000;
/// Title limit, in characters.
pub const MAX_TITLE_CHARS: usize = 200;
/// Criteria per work item.
pub const MAX_CRITERIA: usize = 20;
/// One criterion, in characters.
pub const MAX_CRITERION_CHARS: usize = 500;

/// Where a work item came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    GithubIssue,
    Jira,
    Text,
}

/// The source of a work item. `reference` is a Jira key or `owner/repo#N`; it is
/// only ever the recorded identity, never fetched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSource {
    pub kind: SourceKind,
    pub reference: Option<String>,
}

/// One acceptance criterion.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub text: String,
    /// True when the intake step wrote this criterion because none was given.
    pub drafted: bool,
    /// True once a person has accepted it.
    pub confirmed: bool,
}

/// A normalized unit of requested work.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItem {
    pub source: WorkSource,
    pub title: String,
    pub ask: String,
    pub acceptance_criteria: Vec<Criterion>,
}

/// Why an input was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeError {
    /// The input has no text once control characters are stripped.
    EmptyInput,
    /// The input exceeds [`MAX_INPUT_BYTES`].
    InputTooLarge,
    /// The title line exceeds [`MAX_TITLE_CHARS`].
    TitleTooLong,
    /// More than [`MAX_CRITERIA`] criteria.
    TooManyCriteria,
    /// A criterion exceeds [`MAX_CRITERION_CHARS`].
    CriterionTooLong,
}

impl std::fmt::Display for IntakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "input is empty"),
            Self::InputTooLarge => write!(f, "input exceeds {MAX_INPUT_BYTES} bytes"),
            Self::TitleTooLong => write!(f, "title exceeds {MAX_TITLE_CHARS} characters"),
            Self::TooManyCriteria => write!(f, "more than {MAX_CRITERIA} acceptance criteria"),
            Self::CriterionTooLong => write!(
                f,
                "an acceptance criterion exceeds {MAX_CRITERION_CHARS} characters"
            ),
        }
    }
}

impl std::error::Error for IntakeError {}

/// Normalize pasted text into a [`WorkItem`] with every criterion unconfirmed.
pub fn normalize(input: &str) -> Result<WorkItem, IntakeError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(IntakeError::InputTooLarge);
    }
    let text = clean(input);
    let lines: Vec<&str> = text.lines().collect();
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .ok_or(IntakeError::EmptyInput)?;
    let first_line = lines[first].trim();

    let source = detect_source(first_line);
    // A first line that is itself a list item is both the title and criterion 1.
    let first_item = strong_item(first_line);
    let title = first_item.unwrap_or(first_line).trim().to_owned();
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(IntakeError::TitleTooLong);
    }
    let start = if first_item.is_some() {
        first
    } else {
        first + 1
    };

    let mut criteria: Vec<String> = Vec::new();
    let mut ask_lines: Vec<&str> = Vec::new();
    let mut in_section = false;
    let mut continuable = false;
    for line in &lines[start..] {
        let t = line.trim();
        if t.is_empty() {
            continuable = false;
            if !in_section {
                ask_lines.push("");
            }
            continue;
        }
        if is_criteria_heading(t) {
            in_section = true;
            continuable = false;
            continue;
        }
        let item = strong_item(t).or_else(|| in_section.then(|| bullet(t)).flatten());
        if let Some(text) = item {
            criteria.push(text.trim().to_owned());
            continuable = true;
            continue;
        }
        if continuable && line.starts_with(char::is_whitespace) {
            if let Some(last) = criteria.last_mut() {
                last.push(' ');
                last.push_str(t);
                continue;
            }
        }
        if in_section && !ends_section(t) {
            criteria.push(t.to_owned());
            continuable = true;
            continue;
        }
        in_section = false;
        continuable = false;
        ask_lines.push(t);
    }
    criteria.retain(|c| !c.is_empty());
    if criteria.len() > MAX_CRITERIA {
        return Err(IntakeError::TooManyCriteria);
    }
    if criteria
        .iter()
        .any(|c| c.chars().count() > MAX_CRITERION_CHARS)
    {
        return Err(IntakeError::CriterionTooLong);
    }

    let ask = ask_lines.join("\n").trim().to_owned();
    let ask = if ask.is_empty() { title.clone() } else { ask };
    let acceptance_criteria = if criteria.is_empty() {
        vec![Criterion {
            text: draft_criterion(&ask),
            drafted: true,
            confirmed: false,
        }]
    } else {
        criteria
            .into_iter()
            .map(|text| Criterion {
                text,
                drafted: false,
                confirmed: false,
            })
            .collect()
    };
    Ok(WorkItem {
        source,
        title,
        ask,
        acceptance_criteria,
    })
}

/// Strips control and bidirectional formatting characters. Tabs become spaces
/// and every line ending becomes `\n`.
fn clean(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\n' => out.push('\n'),
            '\t' => out.push(' '),
            c if c.is_control() || is_bidi_control(c) => {}
            c => out.push(c),
        }
    }
    out
}

fn draft_criterion(ask: &str) -> String {
    let line = ask.lines().next().unwrap_or("").trim();
    let mut summary: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        summary.push('…');
    }
    format!("The requested work is complete and verified: {summary}")
}

fn detect_source(first_line: &str) -> WorkSource {
    for token in first_line.split_whitespace() {
        let token = token.trim_matches(|c: char| {
            matches!(
                c,
                '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';' | ':' | '.' | '\'' | '"'
            )
        });
        if let Some(reference) = github_url(token).or_else(|| github_ref(token)) {
            return WorkSource {
                kind: SourceKind::GithubIssue,
                reference: Some(reference),
            };
        }
        if is_jira_key(token) {
            return WorkSource {
                kind: SourceKind::Jira,
                reference: Some(token.to_owned()),
            };
        }
    }
    WorkSource {
        kind: SourceKind::Text,
        reference: None,
    }
}

fn is_jira_key(token: &str) -> bool {
    let Some((project, number)) = token.split_once('-') else {
        return false;
    };
    let mut chars = project.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && project.len() >= 2
        && project.len() <= 10
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && is_number(number)
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.len() <= 9 && text.bytes().all(|b| b.is_ascii_digit())
}

fn is_owner(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 39
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_repo(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 100
        && text != "."
        && text != ".."
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `owner/repo#123`.
fn github_ref(token: &str) -> Option<String> {
    let (repository, number) = token.split_once('#')?;
    let (owner, repo) = repository.split_once('/')?;
    (is_owner(owner) && is_repo(repo) && is_number(number)).then(|| token.to_owned())
}

/// `https://github.com/owner/repo/issues/123`, recorded as `owner/repo#123`.
fn github_url(token: &str) -> Option<String> {
    let rest = token
        .strip_prefix("https://github.com/")
        .or_else(|| token.strip_prefix("http://github.com/"))?;
    let rest = rest.split(['?', '#']).next()?.trim_end_matches('/');
    let mut parts = rest.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    (kind == "issues"
        && parts.all(|p| p.is_empty())
        && is_owner(owner)
        && is_repo(repo)
        && is_number(number))
    .then(|| format!("{owner}/{repo}#{number}"))
}

fn is_criteria_heading(trimmed: &str) -> bool {
    if strong_item(trimmed).is_some() {
        return false;
    }
    let text = trimmed
        .trim_start_matches('#')
        .trim()
        .trim_matches('*')
        .trim_end_matches(':')
        .trim_end_matches('*')
        .trim();
    text.eq_ignore_ascii_case("acceptance criteria")
        || text.eq_ignore_ascii_case("acceptance criterion")
}

/// A line that closes an "Acceptance criteria" section: another heading or a
/// `Label:` line.
fn ends_section(trimmed: &str) -> bool {
    trimmed.starts_with('#') || trimmed.ends_with(':')
}

/// Markers that denote a criterion wherever they appear: `1.`, `1)`, `AC1:`,
/// and checklist boxes. Returns the text after the marker.
fn strong_item(trimmed: &str) -> Option<&str> {
    numbered(trimmed)
        .or_else(|| ac_label(trimmed))
        .or_else(|| checkbox(trimmed))
}

fn after_marker(rest: &str) -> Option<&str> {
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix(' ')
    }
}

fn numbered(trimmed: &str) -> Option<&str> {
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = trimmed[digits..]
        .strip_prefix('.')
        .or_else(|| trimmed[digits..].strip_prefix(')'))?;
    after_marker(rest)
}

fn ac_label(trimmed: &str) -> Option<&str> {
    let bytes = trimmed.as_bytes();
    if bytes.len() < 3 || !bytes[..2].eq_ignore_ascii_case(b"ac") {
        return None;
    }
    let mut at = 2;
    if matches!(bytes[at], b' ' | b'-') {
        at += 1;
    }
    let digits = bytes[at..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 || digits > 3 {
        return None;
    }
    at += digits;
    if !matches!(bytes.get(at), Some(b':' | b'.' | b')')) {
        return None;
    }
    Some(trimmed[at + 1..].trim_start())
}

fn checkbox(trimmed: &str) -> Option<&str> {
    let rest = bullet(trimmed).unwrap_or(trimmed);
    let rest = rest
        .strip_prefix("[ ]")
        .or_else(|| rest.strip_prefix("[x]"))
        .or_else(|| rest.strip_prefix("[X]"))?;
    after_marker(rest)
}

fn bullet(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("• "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(item: &WorkItem) -> Vec<&str> {
        item.acceptance_criteria
            .iter()
            .map(|c| c.text.as_str())
            .collect()
    }

    #[test]
    fn a_jira_key_alone_is_recorded_as_the_source_and_gets_a_drafted_criterion() {
        let item = normalize("  DPES-673 \n").unwrap();
        assert_eq!(item.source.kind, SourceKind::Jira);
        assert_eq!(item.source.reference.as_deref(), Some("DPES-673"));
        assert_eq!(item.title, "DPES-673");
        assert_eq!(item.ask, "DPES-673");
        assert_eq!(item.acceptance_criteria.len(), 1);
        assert!(item.acceptance_criteria[0].drafted);
        assert!(!item.acceptance_criteria[0].confirmed);
        assert!(item.acceptance_criteria[0].text.contains("DPES-673"));
    }

    #[test]
    fn a_jira_key_in_a_title_line_keeps_the_whole_line_as_the_title() {
        let item = normalize("[DPES-12]: Add intake\nWe need an intake step.").unwrap();
        assert_eq!(item.source.reference.as_deref(), Some("DPES-12"));
        assert_eq!(item.title, "[DPES-12]: Add intake");
        assert_eq!(item.ask, "We need an intake step.");
    }

    #[test]
    fn lowercase_and_malformed_keys_are_text() {
        for input in ["dpes-1", "D-1", "ABC-", "ABC-1x", "A1-"] {
            assert_eq!(
                normalize(input).unwrap().source.kind,
                SourceKind::Text,
                "{input}"
            );
        }
    }

    #[test]
    fn a_github_reference_and_an_issues_url_are_recognised() {
        let by_ref = normalize("mc-innersource/motif-mcao#128").unwrap();
        assert_eq!(by_ref.source.kind, SourceKind::GithubIssue);
        assert_eq!(
            by_ref.source.reference.as_deref(),
            Some("mc-innersource/motif-mcao#128")
        );
        let by_url =
            normalize("Fix it https://github.com/mc-innersource/motif-mcao/issues/128?x=1")
                .unwrap();
        assert_eq!(by_url.source.kind, SourceKind::GithubIssue);
        assert_eq!(
            by_url.source.reference.as_deref(),
            Some("mc-innersource/motif-mcao#128")
        );
        for text in [
            "#128",
            "https://github.com/a/b/pull/3",
            "https://evil.example/a/b/issues/3",
        ] {
            assert_eq!(
                normalize(text).unwrap().source.kind,
                SourceKind::Text,
                "{text}"
            );
        }
    }

    #[test]
    fn only_the_first_line_names_the_source() {
        let item = normalize("Add intake\nSee DPES-9 and a/b#1").unwrap();
        assert_eq!(item.source.kind, SourceKind::Text);
        assert_eq!(item.source.reference, None);
    }

    #[test]
    fn free_text_with_numbered_criteria_extracts_title_ask_and_criteria() {
        let item = normalize(
            "Add an intake step\nPeople paste issues.\nThey need a draft.\n\nAcceptance criteria:\n1. Keys are recorded\n2) Text is bounded\nAC3: Drafts are marked\n",
        )
        .unwrap();
        assert_eq!(item.source.kind, SourceKind::Text);
        assert_eq!(item.title, "Add an intake step");
        assert_eq!(item.ask, "People paste issues.\nThey need a draft.");
        assert_eq!(
            texts(&item),
            ["Keys are recorded", "Text is bounded", "Drafts are marked"]
        );
        assert!(item
            .acceptance_criteria
            .iter()
            .all(|c| !c.drafted && !c.confirmed));
    }

    #[test]
    fn free_text_with_no_criteria_drafts_one_from_the_ask() {
        let item = normalize("Speed up startup\nCold start takes 9 seconds.").unwrap();
        assert_eq!(item.ask, "Cold start takes 9 seconds.");
        assert_eq!(item.acceptance_criteria.len(), 1);
        let drafted = &item.acceptance_criteria[0];
        assert!(drafted.drafted && !drafted.confirmed);
        assert!(drafted.text.contains("Cold start takes 9 seconds."));
    }

    #[test]
    fn a_markdown_checklist_yields_criteria_whether_or_not_boxes_are_ticked() {
        let item =
            normalize("Ship it\n- [ ] one\n* [x] two\n[X] three\n- not a criterion").unwrap();
        assert_eq!(texts(&item), ["one", "two", "three"]);
        assert_eq!(item.ask, "- not a criterion");
        assert!(item
            .acceptance_criteria
            .iter()
            .all(|c| !c.confirmed && !c.drafted));
    }

    #[test]
    fn bullets_and_plain_lines_count_only_under_the_criteria_heading() {
        let item = normalize(
            "T\nBody - with dash\n## Acceptance Criteria\n- first\n  continued\n\nsecond line\n## Notes\nlater",
        )
        .unwrap();
        assert_eq!(texts(&item), ["first continued", "second line"]);
        assert_eq!(item.ask, "Body - with dash\n## Notes\nlater");
    }

    #[test]
    fn a_list_only_input_uses_the_first_item_as_title_and_criterion() {
        let item = normalize("1. a\n2. b").unwrap();
        assert_eq!(item.title, "a");
        assert_eq!(texts(&item), ["a", "b"]);
        assert_eq!(item.ask, "a");
    }

    #[test]
    fn version_numbers_and_decimals_are_not_list_items() {
        let item = normalize("T\n3.5 is the target\n10x faster").unwrap();
        assert_eq!(item.ask, "3.5 is the target\n10x faster");
        assert!(item.acceptance_criteria[0].drafted);
    }

    #[test]
    fn limits_are_exact_and_over_limit_input_is_refused() {
        assert_eq!(
            normalize(&"a".repeat(MAX_INPUT_BYTES + 1)),
            Err(IntakeError::InputTooLarge)
        );
        assert_eq!(normalize(" \n\t\n"), Err(IntakeError::EmptyInput));
        assert_eq!(normalize("\u{0}\u{7}"), Err(IntakeError::EmptyInput));

        assert!(normalize(&"t".repeat(MAX_TITLE_CHARS)).is_ok());
        assert_eq!(
            normalize(&"t".repeat(MAX_TITLE_CHARS + 1)),
            Err(IntakeError::TitleTooLong)
        );
        // Characters, not bytes, are counted.
        assert!(normalize(&"é".repeat(MAX_TITLE_CHARS)).is_ok());

        let many = |n: usize| {
            let mut text = String::from("T\n");
            for i in 1..=n {
                text.push_str(&format!("{i}. item\n"));
            }
            text
        };
        assert_eq!(
            normalize(&many(MAX_CRITERIA))
                .unwrap()
                .acceptance_criteria
                .len(),
            MAX_CRITERIA
        );
        assert_eq!(
            normalize(&many(MAX_CRITERIA + 1)),
            Err(IntakeError::TooManyCriteria)
        );

        let one = |n: usize| format!("T\n1. {}", "c".repeat(n));
        assert!(normalize(&one(MAX_CRITERION_CHARS)).is_ok());
        assert_eq!(
            normalize(&one(MAX_CRITERION_CHARS + 1)),
            Err(IntakeError::CriterionTooLong)
        );

        let at_limit = format!("T\n{}", "x".repeat(MAX_INPUT_BYTES - 2));
        let item = normalize(&at_limit).unwrap();
        assert_eq!(item.ask.len(), MAX_INPUT_BYTES - 2);
        assert!(item.acceptance_criteria[0].text.chars().count() <= MAX_CRITERION_CHARS);
    }

    #[test]
    fn control_and_bidirectional_characters_are_stripped_in_every_field() {
        let item = normalize(
            "Ti\u{0}tle\u{1b}[31m \u{202e}DPES-1\r\nAs\u{7}k\u{85}\r\nAC1:\tdo\u{200f} it\u{7f}\r",
        )
        .unwrap();
        assert_eq!(item.title, "Title[31m DPES-1");
        assert_eq!(item.ask, "Ask");
        assert_eq!(texts(&item), ["do it"]);
        for text in [&item.title, &item.ask, &item.acceptance_criteria[0].text] {
            assert!(!text.chars().any(|c| c.is_control() || is_bidi_control(c)));
        }
    }

    #[test]
    fn markup_is_kept_verbatim_not_interpreted() {
        let item =
            normalize("<script>alert(1)</script>\n**bold** [x](http://e)\n1. <b>a</b>").unwrap();
        assert_eq!(item.title, "<script>alert(1)</script>");
        assert_eq!(item.ask, "**bold** [x](http://e)");
        assert_eq!(texts(&item), ["<b>a</b>"]);
    }

    #[test]
    fn a_long_ask_line_is_shortened_only_in_the_drafted_criterion() {
        let line = "w".repeat(400);
        let item = normalize(&format!("T\n{line}")).unwrap();
        assert_eq!(item.ask, line);
        let drafted = &item.acceptance_criteria[0].text;
        assert!(drafted.ends_with('…'));
        assert!(drafted.chars().count() <= MAX_CRITERION_CHARS);
    }

    #[test]
    fn the_work_item_wire_shape_is_stable() {
        let item = normalize("Jira DPES-1\n1. a").unwrap();
        assert_eq!(
            serde_json::to_value(&item).unwrap(),
            serde_json::json!({
                "source": {"kind": "jira", "reference": "DPES-1"},
                "title": "Jira DPES-1",
                "ask": "Jira DPES-1",
                "acceptance_criteria": [{"text": "a", "drafted": false, "confirmed": false}],
            })
        );
        let text = normalize("plain").unwrap();
        assert_eq!(
            serde_json::to_value(&text.source).unwrap(),
            serde_json::json!({"kind": "text", "reference": null})
        );
        assert!(serde_json::from_value::<WorkItem>(serde_json::json!({
            "source": {"kind": "text", "reference": null}, "title": "t", "ask": "a",
            "acceptance_criteria": [], "extra": 1
        }))
        .is_err());
    }
}
