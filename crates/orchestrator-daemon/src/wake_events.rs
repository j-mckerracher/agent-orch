//! Classifies durable events into wake candidates (DPES-1105 phase 3c).
//!
//! Pure: no store, no clock. Every notice is written here from a fixed template out of a task
//! id, a short objective, a state word, a file count and a stable code. Agent-written text
//! reaches a notice in two places only: the objective and a failure reason. Both go through
//! [`redact`], and the reason also through `sanitize`, so a path or a token cannot leave this file.

use orchestrator_core::supervision::Stall;
use orchestrator_store::{StreamEvent, TaskWakeFacts, WakeKind};
use serde_json::Value;

use super::conversation_agent::sanitize;

/// Longest id kept in a notice or a dedupe key, in bytes.
const ID_BYTES: usize = 64;
/// A run of this many id characters looks like a token and is not shown.
const TOKEN_LIKE_CHARS: usize = 32;
/// Stable failure code an attempt may carry when the agent needs a sign-in. Deferred: the
/// store records no stable failure code yet, so the sign-in event never fires in production.
pub(crate) const SIGN_IN_FAILURE_CODE: &str = "sign_in_required";

/// One event worth a notice. The scope comes from the task's facts, so it is not stored here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub task_id: String,
    pub kind: WakeKind,
    pub subject: String,
    pub dedupe_key: String,
    pub notice: String,
}

/// Keeps a daemon-made id short and plain.
fn safe_id(id: &str) -> String {
    let mut safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':') {
                c
            } else {
                '_'
            }
        })
        .collect();
    safe.truncate(ID_BYTES);
    safe
}

/// Replaces every word that looks like a path, a URL or a token with a fixed marker. Runs on
/// user text: a task title.
pub(crate) fn redact(text: &str) -> String {
    redact_with(text, true)
}

/// Redacts the snapshot a wake turn is shown. Titles get the full [`redact`] treatment, so a
/// token typed into another task's or a workstream's title does not reach the agent. An id is
/// as long as a token and must stay, so a word in an id position is checked for paths only:
/// after `[id ` or `workstream `, or the id that opens an open-task line.
pub(crate) fn redact_snapshot(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let words: Vec<&str> = line.split(' ').collect();
            let task_line = words.first() == Some(&"-") && words.get(3) == Some(&"project");
            let mut out = Vec::with_capacity(words.len());
            for (index, word) in words.iter().enumerate() {
                let after_marker = index > 0 && matches!(words[index - 1], "[id" | "workstream");
                let is_id = after_marker || (task_line && index == 1);
                out.push(redact_word(word, !is_id));
            }
            out.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_with(text: &str, tokens: bool) -> String {
    text.split_inclusive(char::is_whitespace)
        .map(|piece| {
            let word = piece.trim_end();
            let rest = &piece[word.len()..];
            format!("{}{rest}", redact_word(word, tokens))
        })
        .collect()
}

fn redact_word(word: &str, tokens: bool) -> String {
    let core = word.trim_matches(|c: char| !(c.is_alphanumeric() || matches!(c, '/' | '\\' | '~')));
    let drive =
        core.len() > 2 && core.as_bytes()[1] == b':' && core.as_bytes()[0].is_ascii_alphabetic();
    // A word with one slash, such as a fraction, is not a path.
    // A lone slash, as in the check name `ci / build`, is a separator, not a path.
    let pathlike = (core.starts_with(['/', '~']) && core != "/")
        || core.contains('\\')
        || core.matches('/').count() >= 2
        || drive;
    let tokenlike = tokens
        && core.len() >= TOKEN_LIKE_CHARS
        && core
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '='));
    if pathlike {
        "[path]".to_owned()
    } else if tokenlike {
        "[redacted]".to_owned()
    } else {
        word.to_owned()
    }
}

/// "t-12 (Fix the login bug)", or just the id when the task has no objective.
fn describe(task_id: &str, facts: Option<&TaskWakeFacts>) -> String {
    let id = safe_id(task_id);
    match facts.map(|facts| redact(&facts.objective)) {
        Some(objective) if !objective.trim().is_empty() => format!("Task {id} ({objective})"),
        _ => format!("Task {id}"),
    }
}

/// One durable event gives zero or one candidate. `facts` are the task's, when it still exists.
pub(crate) fn classify(event: &StreamEvent, facts: Option<&TaskWakeFacts>) -> Option<Candidate> {
    let task = describe(&event.task_id, facts);
    let task_id = safe_id(&event.task_id);
    match event.kind.as_str() {
        "task.report_ready" => Some(Candidate {
            subject: event.task_id.clone(),
            dedupe_key: format!("report:{}", event.task_id),
            notice: format!("{task} has a report ready. Relay its findings and any decisions."),
            task_id,
            kind: WakeKind::ScoutReportReady,
        }),
        "task.transition" => match event.payload.get("state")?.as_str()? {
            "Failed" => Some(failure(event, &task, task_id, "failed", facts)),
            "VerificationFailed" => {
                Some(failure(event, &task, task_id, "failed verification", facts))
            }
            // A report-only task (an investigation) has no candidate to review: its report already
            // wakes the model, so it gets no second wake.
            "Verifying" if facts.is_some_and(|facts| facts.report_only) => None,
            // The agent left a candidate. It waits for review, and the wake turn reads its
            // summary. The subject is the raw id, as a report's is, so the turn can read the
            // task's facts.
            "Verifying" => Some(Candidate {
                subject: event.task_id.clone(),
                dedupe_key: format!("candidate:{task_id}:{}", event.sequence),
                notice: candidate_notice(&task, facts),
                task_id,
                kind: WakeKind::CandidateReady,
            }),
            _ => None,
        },
        "approval.requested" => {
            if event.payload.get("event")?.get("event")?.as_str()? != "requested" {
                return None;
            }
            let approval = safe_id(event.payload.get("approval_id")?.as_str()?);
            Some(Candidate {
                subject: task_id.clone(),
                dedupe_key: format!("approval:{approval}"),
                notice: format!("{task} is ready for your approval."),
                task_id,
                kind: WakeKind::DeliveryWaiting,
            })
        }
        "attempt.state_changed" => {
            // Deferred: the store records no stable failure code yet (decisions-3 Q13), so this
            // arm only matches a payload that carries one. Only a stable code counts, and no
            // session output is read or parsed.
            if event.payload.get("state")?.as_str()? != "Failed"
                || event.payload.get("failure_code").and_then(Value::as_str)
                    != Some(SIGN_IN_FAILURE_CODE)
            {
                return None;
            }
            Some(Candidate {
                subject: task_id.clone(),
                dedupe_key: format!("signin:{task_id}:{}", event.sequence),
                notice: format!("{task} needs you to sign in to its agent."),
                task_id,
                kind: WakeKind::SignInNeeded,
            })
        }
        _ => None,
    }
}

/// A task that failed. The stored reason, when there is one, is named in the notice.
fn failure(
    event: &StreamEvent,
    task: &str,
    task_id: String,
    words: &str,
    facts: Option<&TaskWakeFacts>,
) -> Candidate {
    let reason = facts
        .and_then(|facts| facts.failure_reason.as_deref())
        .map(reason_text)
        .filter(|reason| !reason.is_empty());
    let notice = match reason {
        Some(reason) => format!("{task} {words}: {reason}"),
        None => format!("{task} {words}."),
    };
    Candidate {
        subject: task_id.clone(),
        dedupe_key: format!("failed:{task_id}:{}", event.sequence),
        notice,
        task_id,
        kind: WakeKind::TaskFailed,
    }
}

/// A stored failure reason as one line of notice text: sanitized, redacted and with runs of
/// whitespace collapsed. The store caps the reason, so the notice stays short.
fn reason_text(reason: &str) -> String {
    redact(&sanitize(reason))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Task t-1 (…) finished with a candidate (3 files changed) and waits for review." The count
/// is left out when the store holds none.
fn candidate_notice(task: &str, facts: Option<&TaskWakeFacts>) -> String {
    let files = facts
        .and_then(|facts| facts.changed_files)
        .map_or(String::new(), |count| format!(" ({count} files changed)"));
    format!("{task} finished with a candidate{files} and waits for review.")
}

/// The notice a scout report leaves when its automatic relay failed twice. The report stays in
/// its task, and the notice says where to read it.
pub(crate) fn report_relay_failed_notice(task_id: &str) -> String {
    format!(
        "Report from task {} is ready; automatic relay failed. Open the task to read it.",
        safe_id(task_id)
    )
}

/// The candidate for one stall finding. One per attempt and rule, by its dedupe key.
pub(crate) fn stall_candidate(
    task_id: &str,
    attempt_id: &str,
    stall: Stall,
    facts: Option<&TaskWakeFacts>,
) -> Candidate {
    let task = describe(task_id, facts);
    let attempt = safe_id(attempt_id);
    let (rule, notice) = match stall {
        Stall::NoEvents { quiet_ms } => (
            "no_events",
            format!("{task} has been silent for {} minutes.", quiet_ms / 60_000),
        ),
        Stall::OverWallTime { running_ms } => (
            "wall_time",
            format!(
                "{task} has run for {} minutes, over its time limit.",
                running_ms / 60_000
            ),
        ),
    };
    Candidate {
        task_id: safe_id(task_id),
        kind: WakeKind::TaskStalled,
        subject: safe_id(task_id),
        dedupe_key: format!("stall:{attempt}:{rule}"),
        notice,
    }
}

/// The reminder for a proposal that has waited. `kind` is the proposal kind label, shown as
/// plain text and never with the proposal's parameters.
pub(crate) fn proposal_notice(kind: &str) -> String {
    let label: String = kind
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(ID_BYTES)
        .collect();
    format!("A {label} proposal is waiting for your decision.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::task::TaskState;
    use serde_json::json;

    fn event(kind: &str, payload: Value) -> StreamEvent {
        StreamEvent {
            sequence: 41,
            task_id: "t-12ab".into(),
            session_id: None,
            kind: kind.into(),
            payload,
            created_at: "2026-11-16 02:10:03".into(),
        }
    }

    fn facts(objective: &str) -> TaskWakeFacts {
        TaskWakeFacts {
            scope: "conversation:global".into(),
            objective: objective.into(),
            failure_reason: None,
            changed_files: None,
            summary: None,
            report_only: false,
        }
    }

    #[test]
    fn the_task_state_words_the_classifier_reads_are_what_the_store_writes() {
        assert_eq!(json!(TaskState::Failed), "Failed");
        assert_eq!(json!(TaskState::VerificationFailed), "VerificationFailed");
    }

    #[test]
    fn classify_maps_each_durable_event_to_its_kind() {
        let facts = facts("Fix the login bug");
        let failed = classify(
            &event("task.transition", json!({"state": "Failed"})),
            Some(&facts),
        );
        let failed = failed.unwrap();
        assert_eq!(failed.kind, WakeKind::TaskFailed);
        assert_eq!(failed.dedupe_key, "failed:t-12ab:41");
        assert_eq!(failed.notice, "Task t-12ab (Fix the login bug) failed.");

        let verification = classify(
            &event("task.transition", json!({"state": "VerificationFailed"})),
            Some(&facts),
        )
        .unwrap();
        assert_eq!(verification.kind, WakeKind::TaskFailed);
        assert!(verification.notice.ends_with("failed verification."));

        let approval = classify(
            &event(
                "approval.requested",
                json!({"approval_id": "ap-1", "event": {"event": "requested", "by": "x"}}),
            ),
            Some(&facts),
        )
        .unwrap();
        assert_eq!(approval.kind, WakeKind::DeliveryWaiting);
        assert_eq!(approval.dedupe_key, "approval:ap-1");
        assert!(!WakeKind::DeliveryWaiting.needs_model());

        let sign_in = classify(
            &event(
                "attempt.state_changed",
                json!({"state": "Failed", "failure_code": SIGN_IN_FAILURE_CODE}),
            ),
            None,
        )
        .unwrap();
        assert_eq!(sign_in.kind, WakeKind::SignInNeeded);
        assert_eq!(
            sign_in.notice,
            "Task t-12ab needs you to sign in to its agent."
        );

        let stall = stall_candidate("t-12ab", "a-1", Stall::NoEvents { quiet_ms: 600_000 }, None);
        assert_eq!(stall.kind, WakeKind::TaskStalled);
        assert_eq!(stall.dedupe_key, "stall:a-1:no_events");
        let wall = stall_candidate(
            "t-12ab",
            "a-1",
            Stall::OverWallTime {
                running_ms: 7_200_000,
            },
            None,
        );
        assert_eq!(wall.dedupe_key, "stall:a-1:wall_time");
    }

    #[test]
    fn classify_ignores_other_events() {
        for (kind, payload) in [
            ("task.transition", json!({"state": "Running"})),
            ("task.transition", json!({"state": "Succeeded"})),
            ("session.created", json!({})),
            ("session.state_changed", json!({"state": "Failed"})),
            (
                "approval.requested",
                json!({"approval_id": "a", "event": {"event": "approved"}}),
            ),
            // A failed attempt without the stable code is not a sign-in event.
            (
                "attempt.state_changed",
                json!({"state": "Failed", "failure_reason_available": true}),
            ),
            ("attempt.state_changed", json!({"state": "Running"})),
        ] {
            assert_eq!(
                classify(&event(kind, payload.clone()), None),
                None,
                "{kind} {payload}"
            );
        }
    }

    #[test]
    fn notices_hold_no_path_or_token() {
        let secret = "sk-abcdefghijklmnopqrstuvwxyz0123456789ABCD";
        let objective =
            format!("Fix /Users/me/secret/repo and C:\\work\\x then ~/notes use {secret} ok");
        let facts = facts(&objective);
        let notice = classify(
            &event("task.transition", json!({"state": "Failed"})),
            Some(&facts),
        )
        .unwrap()
        .notice;
        for needle in ["/Users", "secret/repo", "C:\\", "~/", secret] {
            assert!(!notice.contains(needle), "{notice}");
        }
        assert!(
            notice.contains("[path]") && notice.contains("[redacted]"),
            "{notice}"
        );
        // A path in a session event payload is never read.
        let session = event(
            "session.state_changed",
            json!({"state": "Failed", "output": "/tmp/leak"}),
        );
        assert_eq!(classify(&session, Some(&facts)), None);
    }

    #[test]
    fn the_sign_in_event_is_deferred_until_the_store_records_a_failure_code() {
        // What the store writes today for a failed attempt: a flag, never a code.
        let candidate = classify(
            &event(
                "attempt.state_changed",
                json!({"state": "Failed", "failure_reason_available": true}),
            ),
            None,
        );
        assert_eq!(candidate, None);
        // The ordinary failed event is the one a real failure raises.
        let failed = classify(&event("task.transition", json!({"state": "Failed"})), None).unwrap();
        assert_eq!(failed.kind, WakeKind::TaskFailed);
        assert_ne!(failed.kind, WakeKind::SignInNeeded);
    }

    #[test]
    fn a_fraction_in_the_snapshot_is_not_redacted_as_a_path() {
        let line = "Tasks: gates 3/5 verified, 80/100 done; see /tmp/x and a/b/c";
        assert_eq!(
            redact_snapshot(line),
            "Tasks: gates 3/5 verified, 80/100 done; see [path] and [path]"
        );
        // A long id stays, a long secret-like word in a title does not.
        let id = "w".repeat(40);
        let token = "k".repeat(40);
        assert_eq!(
            redact_snapshot(&format!("- Ship {token} [id {id}] state open")),
            format!("- Ship [redacted] [id {id}] state open")
        );
    }

    #[test]
    fn a_failed_task_notice_names_the_stored_reason() {
        let with_reason = TaskWakeFacts {
            failure_reason: Some(
                "The agent failed: build broke in /Users/me/app/src/main.rs".into(),
            ),
            ..facts("Fix the login bug")
        };
        let failed = classify(
            &event("task.transition", json!({"state": "Failed"})),
            Some(&with_reason),
        )
        .unwrap();
        assert_eq!(
            failed.notice,
            "Task t-12ab (Fix the login bug) failed: The agent failed: build broke in [path]"
        );
        assert_eq!(failed.dedupe_key, "failed:t-12ab:41");

        let verification = classify(
            &event("task.transition", json!({"state": "VerificationFailed"})),
            Some(&with_reason),
        )
        .unwrap();
        assert!(
            verification
                .notice
                .starts_with("Task t-12ab (Fix the login bug) failed verification: The agent"),
            "{}",
            verification.notice
        );

        // A blank reason reads like no reason at all.
        let blank = TaskWakeFacts {
            failure_reason: Some("  \n ".into()),
            ..facts("Fix the login bug")
        };
        let plain = classify(
            &event("task.transition", json!({"state": "Failed"})),
            Some(&blank),
        )
        .unwrap();
        assert_eq!(plain.notice, "Task t-12ab (Fix the login bug) failed.");
    }

    #[test]
    fn a_candidate_notice_counts_its_files_or_leaves_the_count_out() {
        let counted = TaskWakeFacts {
            changed_files: Some(3),
            ..facts("Fix the login bug")
        };
        let candidate = classify(
            &event("task.transition", json!({"state": "Verifying"})),
            Some(&counted),
        )
        .unwrap();
        assert_eq!(candidate.kind, WakeKind::CandidateReady);
        assert_eq!(candidate.subject, "t-12ab");
        assert_eq!(candidate.dedupe_key, "candidate:t-12ab:41");
        assert_eq!(
            candidate.notice,
            "Task t-12ab (Fix the login bug) finished with a candidate (3 files changed) and waits for review."
        );
        assert!(WakeKind::CandidateReady.needs_model());

        let uncounted = classify(
            &event("task.transition", json!({"state": "Verifying"})),
            Some(&facts("Fix the login bug")),
        )
        .unwrap();
        assert_eq!(
            uncounted.notice,
            "Task t-12ab (Fix the login bug) finished with a candidate and waits for review."
        );
    }

    #[test]
    fn the_relay_failure_notice_names_the_report_and_keeps_the_id_plain() {
        assert_eq!(
            report_relay_failed_notice("scout-1"),
            "Report from task scout-1 is ready; automatic relay failed. Open the task to read it."
        );
        assert!(!report_relay_failed_notice("a/b c").contains('/'));
    }

    #[test]
    fn redact_keeps_plain_words() {
        assert_eq!(
            redact("Fix the login bug (urgent)"),
            "Fix the login bug (urgent)"
        );
        assert_eq!(redact("see /tmp/x now"), "see [path] now");
    }
}
