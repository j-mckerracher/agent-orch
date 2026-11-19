//! The charter of a project coordinator, and the text rules of its parent channel (DPES-1108
//! phase 7).
//!
//! A coordinator is a persistent orchestrator for one project, below the global orchestrator.
//! The charter is the coordinator's identity and operating rules. It is rendered from the
//! project's scope sentence and handed to every coordinator turn outside the app state, so the
//! model reads it as instructions, not data. Pure text, no I/O.

use serde::{Deserialize, Serialize};

use crate::orchestrator_action::is_disallowed_objective_char;

/// Longest scope sentence, in bytes. Migration 0071 holds the same limit in characters.
pub const MAX_SCOPE_SENTENCE_BYTES: usize = 300;
/// Longest work text of a routed request, in bytes.
pub const MAX_ROUTED_WORK_BYTES: usize = 4096;
/// Longest note of a coordinator report, in bytes.
pub const MAX_REPORT_NOTE_BYTES: usize = 1024;
/// Longest project label the charter names, in bytes.
pub const MAX_LABEL_BYTES: usize = 160;
/// Largest rendered charter, in bytes. It shares the command line with the preamble and the
/// snapshot, so it has its own ceiling.
pub const MAX_CHARTER_BYTES: usize = 3 * 1024;
/// Longest routed request id: `r-` and lowercase hex.
pub const MAX_REQUEST_ID_CHARS: usize = 64;

/// The first line of the charter block.
pub const CHARTER_BEGIN: &str = "=== COORDINATOR CHARTER ===";
/// The last line of the charter block.
pub const CHARTER_END: &str = "=== END COORDINATOR CHARTER ===";

const IDENTITY: &str = "You are the coordinator of one project in MCAO. This charter replaces \
the identity in the instructions above; their rules still hold. You are a persistent \
orchestrator for this project only. The global orchestrator is your parent.";

const OPERATING_MODEL: &str = "Operating model: you do not do project work yourself. Every \
change is a task you propose in this project. You act only inside this project: its tasks, its \
questions and its merges. You never create workstreams, remove the project or route work to a \
coordinator.";

const IDLE: &str = "Idle by default: you do not generate your own work. Act only on a request \
routed to you or on a message from the user in this conversation. An empty queue is a resting \
state: when nothing is routed and the user asked nothing, propose nothing and say only what \
changed. Never start a survey, an audit or a cleanup on your own.";

const REQUESTS: &str = "Requests from the orchestrator: the app state lists each open request \
with its id (r-...). Put that id in the request field of every task.propose made for it. When \
the request is done, or cannot be done, send coordinator.report with verb answer, that request \
id and a short answer. A request stays open until you answer it.";

const PARENT_CHANNEL: &str = "Parent channel: the global orchestrator never reads this \
conversation. The app itself tells it when your tasks are ready, finish, fail or wait, when a \
question is raised or answered, and when work merges. Add only judgment, with \
coordinator.report: answer (needs request), caveat, or blocked (a blocker of your own). Keep \
the note short and factual.";

const USER: &str = "The user: a message from the user in this conversation is authoritative. \
Answer it here.";

const DONE: &str = "Definition of done: you are persistent and have no final state. A turn is \
done when each open request has its tasks proposed or its answer reported, and the user's \
message is answered.";

const NO_SCOPE: &str = "not set yet. The user sets it in the project view. Until then, take \
only what the user or a routed request names.";

const FIXED_BYTES: usize = CHARTER_BEGIN.len()
    + IDENTITY.len()
    + OPERATING_MODEL.len()
    + IDLE.len()
    + REQUESTS.len()
    + PARENT_CHANNEL.len()
    + USER.len()
    + DONE.len()
    + CHARTER_END.len()
    + "Project: \nScope: \n".len()
    + 16;

// The largest label and scope sentence still fit the ceiling.
const _: () = assert!(
    FIXED_BYTES + MAX_LABEL_BYTES + MAX_SCOPE_SENTENCE_BYTES + NO_SCOPE.len() <= MAX_CHARTER_BYTES
);

/// What a coordinator adds to its parent channel. The daemon writes every durable fact itself;
/// these are the coordinator's judgment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportVerb {
    /// The answer to a routed request. It closes the request.
    Answer,
    /// Something the orchestrator should know about finished or ongoing work.
    Caveat,
    /// A blocker of the coordinator's own.
    Blocked,
}

impl ReportVerb {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::Caveat => "caveat",
            Self::Blocked => "blocked",
        }
    }
}

/// Renders the charter of the coordinator of the project `label` names (its folder name and
/// code). `scope_sentence` is the project's stored sentence. The label is cut to
/// [`MAX_LABEL_BYTES`] and flattened to one line; the sentence was checked when it was stored.
pub fn render(label: &str, scope_sentence: Option<&str>) -> String {
    let label = one_line(label, MAX_LABEL_BYTES);
    let scope = scope_sentence
        .map(|sentence| one_line(sentence, MAX_SCOPE_SENTENCE_BYTES))
        .filter(|sentence| !sentence.is_empty())
        .unwrap_or_else(|| NO_SCOPE.to_owned());
    [
        CHARTER_BEGIN,
        IDENTITY,
        &format!("Project: {label}"),
        &format!("Scope: {scope}"),
        OPERATING_MODEL,
        IDLE,
        REQUESTS,
        PARENT_CHANNEL,
        USER,
        DONE,
        CHARTER_END,
    ]
    .join("\n")
}

/// Whether `text` may be stored as a scope sentence: one line of 1 to
/// [`MAX_SCOPE_SENTENCE_BYTES`] bytes with a visible character, and no control, bidi or
/// invisible character.
pub fn check_scope_sentence(text: &str) -> bool {
    is_text(text, MAX_SCOPE_SENTENCE_BYTES) && !text.contains(['\n', '\r', '\t'])
}

/// Whether `text` may be the work of a routed request.
pub fn check_routed_work(text: &str) -> bool {
    is_text(text, MAX_ROUTED_WORK_BYTES)
}

/// Whether `text` may be the note of a coordinator report.
pub fn check_report_note(text: &str) -> bool {
    is_text(text, MAX_REPORT_NOTE_BYTES)
}

/// Whether `id` has the shape of a routed request id: `r-` then lowercase letters, digits or
/// `-`, at most [`MAX_REQUEST_ID_CHARS`] in all.
pub fn is_request_id(id: &str) -> bool {
    id.len() <= MAX_REQUEST_ID_CHARS
        && id.strip_prefix("r-").is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        })
}

fn is_text(text: &str, max_bytes: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= max_bytes
        && !text.chars().any(is_disallowed_objective_char)
}

/// `text` on one line: controls become spaces, runs of whitespace collapse, cut to `max` bytes on
/// a character boundary.
fn one_line(text: &str, max: usize) -> String {
    let flat = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut end = flat.len().min(max);
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    flat[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_charter_has_every_fixed_section_in_order_and_names_the_scope() {
        let charter = render("billing (p-3fa9c1d2)", Some("Payments and invoices."));
        let order = [
            CHARTER_BEGIN,
            "You are the coordinator of one project",
            "Project: billing (p-3fa9c1d2)",
            "Scope: Payments and invoices.",
            "Operating model:",
            "Idle by default:",
            "Requests from the orchestrator:",
            "Parent channel:",
            "The user:",
            "Definition of done:",
            CHARTER_END,
        ];
        let mut from = 0;
        for part in order {
            let at = charter[from..]
                .find(part)
                .unwrap_or_else(|| panic!("{part} missing or out of order"));
            from += at + part.len();
        }
        assert!(charter.contains("Never start a survey, an audit or a cleanup on your own."));
        assert!(charter.contains("An empty queue is a resting state"));
        assert!(charter.contains("never reads this conversation"));
        assert!(charter.contains("A request stays open until you answer it."));
        assert!(charter.contains("persistent and have no final state"));
        assert!(!charter.to_lowercase().contains("captain"));
    }

    #[test]
    fn a_missing_scope_sentence_says_so_and_a_long_label_is_cut_to_one_line() {
        let charter = render("a\nb", None);
        assert!(charter.contains("Project: a b\n"));
        assert!(charter.contains("Scope: not set yet."));
        let label = "x".repeat(MAX_LABEL_BYTES * 2);
        let charter = render(&label, Some(&"y".repeat(MAX_SCOPE_SENTENCE_BYTES)));
        assert!(charter.contains(&format!("Project: {}\n", "x".repeat(MAX_LABEL_BYTES))));
        assert!(charter.len() <= MAX_CHARTER_BYTES);
        // A sentence that is only whitespace is no sentence.
        assert!(render("p", Some(" ")).contains("Scope: not set yet."));
    }

    #[test]
    fn a_scope_sentence_is_one_bounded_visible_line() {
        assert!(check_scope_sentence("Payments, invoices and refunds."));
        assert!(!check_scope_sentence(""));
        assert!(!check_scope_sentence("   "));
        assert!(!check_scope_sentence("two\nlines"));
        assert!(!check_scope_sentence("tab\there"));
        assert!(!check_scope_sentence("a\u{202E}b"));
        assert!(!check_scope_sentence("a\u{200B}b"));
        assert!(check_scope_sentence(&"x".repeat(MAX_SCOPE_SENTENCE_BYTES)));
        assert!(!check_scope_sentence(
            &"x".repeat(MAX_SCOPE_SENTENCE_BYTES + 1)
        ));
    }

    #[test]
    fn routed_work_and_report_notes_are_bounded_visible_text() {
        assert!(check_routed_work(
            "Find why the nightly export fails.\nThen fix it."
        ));
        assert!(!check_routed_work(" \n"));
        assert!(!check_routed_work(&"x".repeat(MAX_ROUTED_WORK_BYTES + 1)));
        assert!(check_report_note("Done: the export runs again."));
        assert!(!check_report_note(&"x".repeat(MAX_REPORT_NOTE_BYTES + 1)));
        assert!(!check_report_note("a\u{0007}b"));
    }

    #[test]
    fn a_request_id_is_r_dash_and_a_lowercase_slug() {
        assert!(is_request_id("r-0a1b2c3d4e5f"));
        assert!(!is_request_id("r-"));
        assert!(!is_request_id("x-0a1b"));
        assert!(!is_request_id("r-0A1B"));
        assert!(!is_request_id(&format!(
            "r-{}",
            "a".repeat(MAX_REQUEST_ID_CHARS)
        )));
    }

    #[test]
    fn report_verbs_have_their_wire_names() {
        for (verb, name) in [
            (ReportVerb::Answer, "answer"),
            (ReportVerb::Caveat, "caveat"),
            (ReportVerb::Blocked, "blocked"),
        ] {
            assert_eq!(verb.as_str(), name);
            assert_eq!(serde_json::to_string(&verb).unwrap(), format!("\"{name}\""));
        }
    }
}
