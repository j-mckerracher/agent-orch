//! Codex `exec --json` NDJSON protocol translation.
//!
//! Codex output is untrusted input. The parser accepts the known envelope
//! shapes recorded in `docs/evidence/m0-03-codex-exec-windows.md` and rejects
//! malformed lines, unknown top-level `type`s, and required protocol
//! violations (thread before turn, items outside a turn, missing
//! `turn.completed`) as errors. It never interprets an exit as success; the
//! caller maps `TurnOutcome` and the process exit code itself.
//!
//! The transition table lives only here.

use crate::contract::{AgentEvent, TurnOutcome};
use serde_json::Value;
use uuid::Uuid;

/// Violations of the required protocol ordering, or an unparseable line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolViolation {
    #[error("malformed JSON: {0}")]
    MalformedJson(String),
    #[error("missing or invalid required field `{0}`")]
    BadField(&'static str),
    #[error("unknown event type `{0}`")]
    UnknownType(String),
    #[error("protocol violation: {0}")]
    Ordering(String),
}

/// Internal protocol phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    AwaitingThread,
    AwaitingTurn,
    InTurn,
    TurnDone,
    /// After any violation: keep consuming input, keep reporting the
    /// original violation, track nothing further.
    Faulted,
}

/// One parser per running agent process. Feed stdout lines in order.
#[derive(Debug, Clone)]
pub struct Parser {
    phase: Phase,
    last_message: Option<String>,
    turn_finished: bool,
    violation: Option<ProtocolViolation>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            phase: Phase::AwaitingThread,
            last_message: None,
            turn_finished: false,
            violation: None,
        }
    }

    /// True once `turn.completed` has been seen.
    pub fn turn_finished(&self) -> bool {
        self.turn_finished
    }

    /// The first recorded violation, if any.
    pub fn violation(&self) -> Option<&ProtocolViolation> {
        self.violation.as_ref()
    }

    /// The most recent `agent_message` text, if any.
    pub fn last_message(&self) -> Option<&str> {
        self.last_message.as_deref()
    }

    /// Translate one stdout line in order. Returns `None` when the line has
    /// no canonical counterpart (blank lines, item types without a
    /// canonical event), `Some(Ok)` for a mapped event, `Some(Err)` for a
    /// protocol violation. After a violation every later line returns the
    /// original error.
    pub fn feed(&mut self, line: &str) -> Option<Result<AgentEvent, ProtocolViolation>> {
        if self.phase == Phase::Faulted {
            return Some(Err(self
                .violation
                .clone()
                .unwrap_or(ProtocolViolation::Ordering("already faulted".into()))));
        }
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                let v = ProtocolViolation::MalformedJson(truncate(line, 200));
                self.fault(v.clone());
                return Some(Err(v));
            }
        };
        match self.translate(&value) {
            Ok(None) => None,
            Ok(Some(ev)) => Some(Ok(ev)),
            Err(v) => {
                self.fault(v.clone());
                Some(Err(v))
            }
        }
    }

    fn fault(&mut self, v: ProtocolViolation) {
        if self.violation.is_none() {
            self.violation = Some(v);
        }
        self.phase = Phase::Faulted;
    }

    fn translate(&mut self, v: &Value) -> Result<Option<AgentEvent>, ProtocolViolation> {
        let r = v.as_object().ok_or(ProtocolViolation::BadField("type"))?;
        let ty = r
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or(ProtocolViolation::BadField("type"))?;
        match ty {
            "thread.started" => {
                if self.phase != Phase::AwaitingThread {
                    return Err(ProtocolViolation::Ordering(
                        "thread.started must be the first event".into(),
                    ));
                }
                let sid = r
                    .get("thread_id")
                    .and_then(|t| t.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or(ProtocolViolation::BadField("thread_id"))?;
                self.phase = Phase::AwaitingTurn;
                Ok(Some(AgentEvent::SessionStarted { session_id: sid }))
            }
            "turn.started" => {
                if self.phase != Phase::AwaitingTurn {
                    return Err(ProtocolViolation::Ordering(
                        "turn.started requires thread.started first".into(),
                    ));
                }
                self.phase = Phase::InTurn;
                Ok(None)
            }
            "item.started" => {
                if self.phase != Phase::InTurn {
                    return Err(ProtocolViolation::Ordering(
                        "item.started outside a turn".into(),
                    ));
                }
                let item = r
                    .get("item")
                    .and_then(|i| i.as_object())
                    .ok_or(ProtocolViolation::BadField("item"))?;
                item.get("type")
                    .and_then(|t| t.as_str())
                    .ok_or(ProtocolViolation::BadField("item.type"))?;
                Ok(None)
            }
            "item.completed" => {
                if self.phase != Phase::InTurn {
                    return Err(ProtocolViolation::Ordering(
                        "item.completed outside a turn".into(),
                    ));
                }
                let item = r
                    .get("item")
                    .and_then(|i| i.as_object())
                    .ok_or(ProtocolViolation::BadField("item"))?;
                let item_type = item
                    .get("type")
                    .and_then(|t| t.as_str())
                    .ok_or(ProtocolViolation::BadField("item.type"))?;
                if item_type == "agent_message" {
                    let text = item
                        .get("text")
                        .and_then(|t| t.as_str())
                        .ok_or(ProtocolViolation::BadField("item.text"))?
                        .to_string();
                    self.last_message = Some(text.clone());
                    Ok(Some(AgentEvent::MessageDelta { text }))
                } else {
                    // Unknown item types (e.g. reasoning) are not a protocol
                    // violation; they carry no canonical event.
                    Ok(None)
                }
            }
            "turn.completed" => {
                if self.phase != Phase::InTurn {
                    return Err(ProtocolViolation::Ordering(
                        "turn.completed outside a turn".into(),
                    ));
                }
                self.turn_finished = true;
                self.phase = Phase::TurnDone;
                Ok(Some(AgentEvent::TurnFinished {
                    outcome: TurnOutcome::Success,
                }))
            }
            other => Err(ProtocolViolation::UnknownType(other.to_string())),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let b = s.len().min(max);
    let mut t = s.get(..b).unwrap_or(s).to_string();
    if b < s.len() {
        t.push_str("…");
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";

    fn thread() -> String {
        format!(r#"{{"type":"thread.started","thread_id":"{SID}"}}"#)
    }
    const TURN_STARTED: &str = r#"{"type":"turn.started"}"#;
    const MSG: &str =
        r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"PONG"}}"#;
    const TURN_DONE: &str = r#"{"type":"turn.completed","usage":{"input_tokens":1}}"#;

    #[test]
    fn full_happy_sequence() {
        let mut p = Parser::new();
        match p.feed(&thread()) {
            Some(Ok(AgentEvent::SessionStarted { session_id })) => {
                assert_eq!(session_id.to_string(), SID);
            }
            other => panic!("expected SessionStarted, got {other:?}"),
        }
        assert_eq!(p.feed(TURN_STARTED), None);
        assert!(matches!(
            p.feed(MSG),
            Some(Ok(AgentEvent::MessageDelta { text })) if text == "PONG"
        ));
        assert!(matches!(
            p.feed(TURN_DONE),
            Some(Ok(AgentEvent::TurnFinished { .. }))
        ));
        assert!(p.turn_finished());
        assert_eq!(p.last_message(), Some("PONG"));
    }

    #[test]
    fn blank_and_unknown_item_lines_are_skipped() {
        let mut p = Parser::new();
        p.feed(&thread()).unwrap().unwrap();
        assert_eq!(p.feed(TURN_STARTED), None);
        assert_eq!(p.feed(""), None);
        let reasoning =
            r#"{"type":"item.completed","item":{"id":"item_1","type":"reasoning","text":"hmm"}}"#;
        assert_eq!(p.feed(reasoning), None);
        assert!(p.feed(TURN_DONE).unwrap().is_ok());
        assert_eq!(p.last_message(), None);
    }

    #[test]
    fn malformed_json_faults() {
        let mut p = Parser::new();
        assert_eq!(
            p.feed(&thread()).unwrap(),
            Ok(AgentEvent::SessionStarted {
                session_id: Uuid::parse_str(SID).unwrap()
            })
        );
        assert_eq!(p.feed(TURN_STARTED), None);
        let err = p.feed("not json at all").unwrap().unwrap_err();
        assert!(matches!(err, ProtocolViolation::MalformedJson(_)));
        assert!(p.violation().is_some());
        // Subsequent lines keep reporting the same violation.
        let err2 = p.feed(TURN_DONE).unwrap().unwrap_err();
        assert!(matches!(err2, ProtocolViolation::MalformedJson(_)));
    }

    #[test]
    fn unknown_type_faults() {
        let mut p = Parser::new();
        p.feed(&thread()).unwrap().unwrap();
        assert_eq!(p.feed(TURN_STARTED), None);
        let err = p.feed(r#"{"type":"mystery.event"}"#).unwrap().unwrap_err();
        assert!(matches!(err, ProtocolViolation::UnknownType(t) if t == "mystery.event"));
    }

    #[test]
    fn ordering_violations_fault() {
        let mut p = Parser::new();
        let err = p.feed(TURN_STARTED).unwrap().unwrap_err();
        assert!(matches!(err, ProtocolViolation::Ordering(_)));
        assert!(p.violation().is_some());

        let mut p = Parser::new();
        p.feed(&thread()).unwrap().unwrap();
        assert_eq!(p.feed(TURN_STARTED), None);
        let err = p.feed(&thread()).unwrap().unwrap_err();
        assert!(matches!(err, ProtocolViolation::Ordering(_)));
    }

    #[test]
    fn missing_turn_completed_is_detected() {
        let mut p = Parser::new();
        p.feed(&thread()).unwrap().unwrap();
        assert_eq!(p.feed(TURN_STARTED), None);
        p.feed(MSG).unwrap().unwrap();
        assert!(!p.turn_finished());
    }

    #[test]
    fn bad_thread_id_faults() {
        let mut p = Parser::new();
        let err = p
            .feed(r#"{"type":"thread.started","thread_id":"nope"}"#)
            .unwrap()
            .unwrap_err();
        assert!(matches!(err, ProtocolViolation::BadField("thread_id")));
    }

    #[test]
    fn non_object_line_faults() {
        let mut p = Parser::new();
        let err = p.feed("42").unwrap().unwrap_err();
        assert!(matches!(err, ProtocolViolation::BadField("type")));
    }
}
