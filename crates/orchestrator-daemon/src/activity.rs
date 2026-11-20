//! Live agent activity (DPES-1044): what an agent is doing right now, kept in memory, and the
//! bounds every tool text passes through before it is stored or shown.
//!
//! - [`ActivityBoard`] holds one slot per running chat turn (keyed by turn id) or task session
//!   (keyed by session id): the phase, the tool in flight, when the phase began and when the
//!   last event arrived. It is in memory only, so a restart empties it. The wake scheduler's
//!   stall check reads the last-event time.
//! - [`ToolStart`] and [`ToolEnd`] are a tool event after sanitizing, redaction and truncation.
//!   Adapters hand over raw text; nothing else may store it.
//! - [`Coalescer`] joins thinking deltas so a long turn writes few rows.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use orchestrator_diagnostics::{redact, truncate};
use serde_json::{json, Value};

use crate::api::conversation_agent::sanitize;
use crate::executor::unix_now_ms;

/// Characters kept of a started tool's summary.
const SUMMARY_CHARS: usize = 160;
/// Characters kept of a finished tool's result preview.
const RESULT_CHARS: usize = 400;
const ID_CHARS: usize = 64;
const NAME_CHARS: usize = 64;
/// Thinking is written once the buffer reaches this size.
const FLUSH_BYTES: usize = 2048;
/// Thinking is written this long after its first buffered delta.
const FLUSH_AFTER: Duration = Duration::from_millis(250);

/// Sanitizes, folds newlines to spaces, redacts, then truncates. The order matters: a cut made
/// before redaction could leave half a credential that no pattern recognizes.
fn bounded(text: &str, max_chars: usize) -> String {
    let folded: String = sanitize(text)
        .chars()
        .map(|c| if matches!(c, '\n' | '\t') { ' ' } else { c })
        .collect();
    truncate(&redact(&folded), max_chars)
}

/// A tool call that began, with every text field bounded and redacted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolStart {
    pub id: String,
    pub name: String,
    pub summary: String,
}

impl ToolStart {
    /// `None` when the provider gave no id: such a call cannot be matched to its end.
    pub(crate) fn bounded(id: &str, name: &str, summary: &str) -> Option<Self> {
        let id = bounded(id, ID_CHARS);
        (!id.trim().is_empty()).then(|| Self {
            id,
            name: bounded(name, NAME_CHARS),
            summary: bounded(summary, SUMMARY_CHARS),
        })
    }
}

/// A tool call that ended, with its result preview bounded and redacted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolEnd {
    pub id: String,
    pub ok: bool,
    pub summary: String,
}

impl ToolEnd {
    pub(crate) fn bounded(id: &str, ok: bool, summary: &str) -> Option<Self> {
        let id = bounded(id, ID_CHARS);
        (!id.trim().is_empty()).then(|| Self {
            id,
            ok,
            summary: bounded(summary, RESULT_CHARS),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Starting,
    Thinking,
    Tool,
    Replying,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Thinking => "thinking",
            Self::Tool => "tool",
            Self::Replying => "replying",
        }
    }
}

struct Slot {
    phase: Phase,
    tool: Option<ToolStart>,
    phase_since_ms: u64,
    last_event_ms: u64,
}

impl Slot {
    fn new(now_ms: u64) -> Self {
        Self {
            phase: Phase::Starting,
            tool: None,
            phase_since_ms: now_ms,
            last_event_ms: now_ms,
        }
    }
}

/// See the module note.
#[derive(Default)]
pub(crate) struct ActivityBoard {
    slots: Mutex<HashMap<String, Slot>>,
}

impl ActivityBoard {
    fn slots(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.slots.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Opens a slot in the `starting` phase. Replaces a stale one.
    pub(crate) fn begin(&self, key: &str) {
        self.slots()
            .insert(key.to_owned(), Slot::new(unix_now_ms()));
    }

    pub(crate) fn touch(&self, key: &str) {
        self.touch_at(key, unix_now_ms());
    }

    pub(crate) fn touch_at(&self, key: &str, now_ms: u64) {
        self.slots()
            .entry(key.to_owned())
            .or_insert_with(|| Slot::new(now_ms))
            .last_event_ms = now_ms;
    }

    pub(crate) fn last_event_ms(&self, key: &str) -> Option<u64> {
        self.slots().get(key).map(|slot| slot.last_event_ms)
    }

    pub(crate) fn forget(&self, key: &str) {
        self.slots().remove(key);
    }

    /// Moves to `phase` without a tool in flight. The phase clock restarts only on a change.
    pub(crate) fn set_phase(&self, key: &str, phase: Phase) {
        let now = unix_now_ms();
        let mut slots = self.slots();
        let slot = slots
            .entry(key.to_owned())
            .or_insert_with(|| Slot::new(now));
        if slot.phase != phase || slot.tool.is_some() {
            slot.phase = phase;
            slot.tool = None;
            slot.phase_since_ms = now;
        }
        slot.last_event_ms = now;
    }

    pub(crate) fn tool_started(&self, key: &str, tool: ToolStart) {
        let now = unix_now_ms();
        let mut slots = self.slots();
        let slot = slots
            .entry(key.to_owned())
            .or_insert_with(|| Slot::new(now));
        slot.phase = Phase::Tool;
        slot.tool = Some(tool);
        slot.phase_since_ms = now;
        slot.last_event_ms = now;
    }

    /// Ends the displayed tool when `id` is it, back to `thinking`. The end of another call
    /// (tools can overlap) only counts as an event.
    pub(crate) fn tool_finished(&self, key: &str, id: &str) {
        let shown = self
            .slots()
            .get(key)
            .is_some_and(|slot| slot.tool.as_ref().is_some_and(|tool| tool.id == id));
        if shown {
            self.set_phase(key, Phase::Thinking);
        } else {
            self.touch(key);
        }
    }

    /// The slot as the wire shows it: durations measured now, never timestamps.
    pub(crate) fn snapshot(&self, key: &str) -> Option<Value> {
        let now = unix_now_ms();
        self.slots().get(key).map(|slot| {
            json!({
                "phase": slot.phase.as_str(),
                "tool": slot.tool.as_ref().map(|tool| json!({
                    "id": tool.id,
                    "name": tool.name,
                    "summary": tool.summary,
                })),
                "phase_ms": now.saturating_sub(slot.phase_since_ms),
                "idle_ms": now.saturating_sub(slot.last_event_ms),
            })
        })
    }
}

/// Joins thinking deltas. The caller writes what [`Self::push`] or [`Self::take`] returns.
#[derive(Default)]
pub(crate) struct Coalescer {
    buffer: String,
    since: Option<Instant>,
}

impl Coalescer {
    /// Adds a delta. Returns the joined text once it reached 2 KiB or its age passed 250 ms.
    pub(crate) fn push(&mut self, text: &str) -> Option<String> {
        if self.buffer.is_empty() {
            self.since = Some(Instant::now());
        }
        self.buffer.push_str(text);
        if self.buffer.len() >= FLUSH_BYTES || self.due() {
            self.take()
        } else {
            None
        }
    }

    /// Whether buffered text has waited 250 ms.
    pub(crate) fn due(&self) -> bool {
        self.since
            .is_some_and(|since| since.elapsed() >= FLUSH_AFTER)
    }

    /// Empties the buffer; `None` when nothing is buffered.
    pub(crate) fn take(&mut self) -> Option<String> {
        self.since = None;
        (!self.buffer.is_empty()).then(|| std::mem::take(&mut self.buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_text_is_redacted_before_it_is_cut_and_folded_to_one_line() {
        let secret = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
        let long = format!("{} {secret}", "x".repeat(SUMMARY_CHARS - 5));
        let start = ToolStart::bounded("tool-1", "Bash", &long).unwrap();
        assert!(!start.summary.contains("ghp_"), "{}", start.summary);
        assert!(
            !start.summary.contains("a1B2c3D4e5a1B2"),
            "{}",
            start.summary
        );
        let end = ToolEnd::bounded(
            "tool-1",
            true,
            "line one\nline two\u{1b}[31m token=hunter2value",
        )
        .unwrap();
        assert!(!end.summary.contains('\n') && !end.summary.contains('\u{1b}'));
        assert!(!end.summary.contains("hunter2value"), "{}", end.summary);
        let wide = ToolEnd::bounded("tool-1", false, &"y".repeat(RESULT_CHARS + 50)).unwrap();
        assert!(
            wide.summary.ends_with("...[truncated 50 chars]"),
            "{}",
            wide.summary
        );
        assert!(ToolStart::bounded("  ", "Bash", "x").is_none());
    }

    #[test]
    fn board_tracks_phase_and_tool_and_forgets() {
        let board = ActivityBoard::default();
        assert!(board.snapshot("k").is_none());
        board.begin("k");
        assert_eq!(board.snapshot("k").unwrap()["phase"], "starting");
        board.set_phase("k", Phase::Thinking);
        let tool = ToolStart::bounded("a", "Read", "f.rs").unwrap();
        board.tool_started("k", tool);
        let snap = board.snapshot("k").unwrap();
        assert_eq!(snap["phase"], "tool");
        assert_eq!(snap["tool"]["name"], "Read");
        board.tool_finished("k", "other");
        assert_eq!(board.snapshot("k").unwrap()["phase"], "tool");
        board.tool_finished("k", "a");
        let snap = board.snapshot("k").unwrap();
        assert_eq!(snap["phase"], "thinking");
        assert!(snap["tool"].is_null());
        board.touch_at("k", 5);
        assert_eq!(board.last_event_ms("k"), Some(5));
        board.forget("k");
        assert_eq!(board.last_event_ms("k"), None);
    }

    #[test]
    fn coalescer_flushes_at_two_kib_or_when_taken() {
        let mut joined = Coalescer::default();
        assert_eq!(joined.push("a"), None);
        assert_eq!(joined.push("b"), None);
        assert_eq!(joined.take().as_deref(), Some("ab"));
        assert_eq!(joined.take(), None);
        let big = "z".repeat(FLUSH_BYTES);
        assert_eq!(joined.push(&big).as_deref(), Some(big.as_str()));
        joined.push("late");
        std::thread::sleep(FLUSH_AFTER + Duration::from_millis(20));
        assert!(joined.due());
        assert_eq!(joined.push("r").as_deref(), Some("later"));
    }
}
