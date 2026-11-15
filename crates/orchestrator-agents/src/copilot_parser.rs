//! Translation of the Copilot CLI `--output-format json` stream (DPES-1105).
//!
//! The stream is one JSON object per line. Shapes were verified against
//! Copilot CLI 1.0.109 (`tests/fixtures/copilot/`). `session.start`,
//! `assistant.message` and `session.error` keep their fields under `data`.
//! `result` keeps `exitCode` and `usage` at the top level.
//!
//! A run completes when the stream has a `result` with exit code 0 and at least
//! one non-empty assistant message. A `session.error` reported by a user hook
//! alongside a completed turn is a warning, not a failure (the decision is
//! recorded in the PR). Unknown types and lines that are not JSON are ignored.
//! Stream text is provider output, so it is sanitized before it reaches an
//! error, a warning or a log.

use serde_json::Value;
use uuid::Uuid;

use crate::contract::{AgentEvent, TurnOutcome};

/// Longest provider-reported error text kept, in characters.
const MAX_ERROR_CHARS: usize = 200;
/// A word this long made only of token characters may be a credential.
const TOKEN_MIN_LEN: usize = 24;

/// Incremental translator for one Copilot run.
#[derive(Debug, Clone, Default)]
pub struct CopilotParser {
    session_id: Option<Uuid>,
    last_message: Option<String>,
    session_error: Option<String>,
    result_seen: bool,
    result_exit_code: Option<i64>,
}

impl CopilotParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// The session id from `session.start`, if one was read.
    pub fn session_id(&self) -> Option<Uuid> {
        self.session_id
    }

    /// The latest non-empty assistant message.
    pub fn last_message(&self) -> Option<&str> {
        self.last_message.as_deref()
    }

    /// True when the run finished with exit code 0 and a real reply. A session
    /// error alongside them is a warning, not a failure: the turn happened.
    pub fn completed(&self) -> bool {
        self.result_seen && self.result_exit_code == Some(0) && self.last_message.is_some()
    }

    /// A session error the run reported, sanitized. It is shown as a warning
    /// when the run completed, and it is the failure reason when it did not.
    pub fn warning(&self) -> Option<&str> {
        self.session_error.as_deref()
    }

    /// Why the run did not complete, from provider-reported facts. `None` when
    /// it completed, and before a `result` arrives when no error was reported.
    pub fn failure(&self) -> Option<String> {
        if !self.result_seen {
            return self.session_error.clone();
        }
        if self.completed() {
            return None;
        }
        let reason = match self.result_exit_code {
            Some(0) if self.last_message.is_none() => "Copilot finished without a reply".to_owned(),
            Some(0) => "Copilot reported no reply".to_owned(),
            Some(code) => format!("Copilot reported exit code {code}"),
            None => "Copilot reported no exit code".to_owned(),
        };
        Some(match &self.session_error {
            Some(error) => format!("{reason}; {error}"),
            None => reason,
        })
    }

    /// Translates one stdout line. Returns `None` for lines that have no
    /// canonical event, including lines that are not JSON.
    pub fn feed(&mut self, line: &str) -> Option<AgentEvent> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        let object = value.as_object()?;
        let data = object.get("data");
        match object.get("type")?.as_str()? {
            "session.start" => {
                let session_id = data?
                    .get("sessionId")?
                    .as_str()
                    .and_then(|id| Uuid::parse_str(id).ok())?;
                self.session_id = Some(session_id);
                Some(AgentEvent::SessionStarted { session_id })
            }
            // Whole messages, not deltas: one final reply per assistant message.
            "assistant.message" => {
                let text = data?
                    .get("content")?
                    .as_str()
                    .filter(|text| !text.is_empty())?;
                self.last_message = Some(text.to_owned());
                Some(AgentEvent::MessageDelta {
                    text: text.to_owned(),
                })
            }
            "session.error" => {
                if self.session_error.is_none() {
                    let message = data
                        .and_then(|data| data.get("message").or_else(|| data.get("errorType")))
                        .and_then(Value::as_str)
                        .unwrap_or("Copilot reported an error");
                    self.session_error = Some(sanitize(message));
                }
                None
            }
            "result" => {
                self.result_seen = true;
                self.result_exit_code = object.get("exitCode").and_then(Value::as_i64);
                Some(AgentEvent::TurnFinished {
                    outcome: if self.completed() {
                        TurnOutcome::Success
                    } else {
                        TurnOutcome::Failure
                    },
                })
            }
            _ => None,
        }
    }
}

/// Makes provider text safe to show or log. Control characters become spaces,
/// credential-shaped words become `[REDACTED]`, and the result is capped at
/// [`MAX_ERROR_CHARS`] characters.
pub(crate) fn sanitize(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    cleaned
        .split_whitespace()
        .map(|word| {
            if is_token_like(word) {
                "[REDACTED]".to_owned()
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_ERROR_CHARS)
        .collect()
}

fn is_token_like(word: &str) -> bool {
    let tokenish = word.len() >= TOKEN_MIN_LEN
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '='));
    let pathish = word.contains(['/', '\\', ':']);
    tokenish && !pathish
}
