//! Bounded, in-memory ring of recent agent output per session, behind the
//! read-only `sessions.output_tail` RPC (DPES-848).
//!
//! What it holds: the text the executor already reads from each agent through
//! its adapter, split into lines. Message text is stored as `stdout` and
//! protocol errors as `stderr`. The adapters keep raw stderr diagnostic and do
//! not hand it to the executor, so raw stderr that is not a protocol error is
//! not retained here.
//!
//! Bounds (all hard):
//! - at most [`RING_LINES`] lines per session; older lines are dropped;
//! - at most [`MAX_LINE_BYTES`] bytes per line; longer lines are cut at a
//!   character boundary and flagged `line_truncated`;
//! - at most [`RETAINED_ENDED_SESSIONS`] ended sessions are kept, oldest
//!   evicted first; live sessions are bounded by the scheduler limits.
//!
//! Lifetime choice: a session's ring is kept in memory after the session ends
//! (flagged `ended`) for the last few sessions only, so a caller can still read
//! the final lines of a run that just finished. Nothing is written to disk and
//! everything is lost when the daemon exits.
//!
//! Safety for the agent: recording only copies text the executor already holds
//! into this structure under a short mutex; it never touches the agent's
//! pipes, process or exit status. Reading copies at most
//! [`MAX_TAIL_LINES`] lines and has no side effect on the session.
//!
//! Secrets: nothing is redacted. Agent output may contain secrets. The RPC is
//! read-only, requires the `sessions` capability on the owner-authenticated
//! channel like the other session inspection methods, and retained output is
//! held only in memory.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// Lines returned when the caller names no limit.
pub(crate) const DEFAULT_TAIL_LINES: usize = 40;
/// Most lines one fetch returns; larger requests are clamped to this.
pub(crate) const MAX_TAIL_LINES: usize = 100;
/// Lines retained per session.
pub(crate) const RING_LINES: usize = 500;
/// Bytes retained per line.
pub(crate) const MAX_LINE_BYTES: usize = 4096;
/// Ended sessions whose rings stay readable.
pub(crate) const RETAINED_ENDED_SESSIONS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Line {
    /// Monotonic per session, starting at 1.
    pub(crate) sequence: u64,
    pub(crate) stream: Stream,
    pub(crate) text: String,
    pub(crate) line_truncated: bool,
}

/// One fetch. `lines` is oldest first.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Tail {
    pub(crate) lines: Vec<Line>,
    /// Lines after the caller's cursor exist that are not in `lines`, because
    /// they were dropped from the ring or exceed the requested limit.
    pub(crate) truncated_lines: bool,
    /// Sequence of the newest line recorded so far (0 when none); pass it as
    /// the cursor to see only later lines.
    pub(crate) last_sequence: u64,
    pub(crate) ended: bool,
}

/// Collects one stream's text into lines, keeping at most one bounded
/// partial line.
#[derive(Default)]
struct Assembler {
    pending: String,
    truncated: bool,
    open: bool,
}

impl Assembler {
    fn push_segment(&mut self, segment: &str) {
        self.open = true;
        let room = MAX_LINE_BYTES - self.pending.len();
        if segment.len() <= room {
            self.pending.push_str(segment);
            return;
        }
        let mut cut = room;
        while !segment.is_char_boundary(cut) {
            cut -= 1;
        }
        self.pending.push_str(&segment[..cut]);
        self.truncated = true;
    }

    fn take(&mut self) -> Option<(String, bool)> {
        if !self.open {
            return None;
        }
        self.open = false;
        let mut text = std::mem::take(&mut self.pending);
        if text.ends_with('\r') {
            text.pop();
        }
        Some((text, std::mem::take(&mut self.truncated)))
    }
}

pub(crate) struct OutputRing {
    lines: VecDeque<Line>,
    last_sequence: u64,
    stdout: Assembler,
    stderr: Assembler,
    ended: bool,
}

impl OutputRing {
    pub(crate) fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            last_sequence: 0,
            stdout: Assembler::default(),
            stderr: Assembler::default(),
            ended: false,
        }
    }

    fn append(&mut self, stream: Stream, text: String, line_truncated: bool) {
        self.last_sequence += 1;
        if self.lines.len() == RING_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(Line {
            sequence: self.last_sequence,
            stream,
            text,
            line_truncated,
        });
    }

    /// Adds text; every `\n` completes a line, and text after the last `\n`
    /// waits for more text or for [`OutputRing::finish`].
    pub(crate) fn push_text(&mut self, stream: Stream, text: &str) {
        if self.ended {
            return;
        }
        let mut rest = text;
        while !rest.is_empty() {
            let (segment, complete) = match rest.find('\n') {
                Some(index) => (&rest[..index], true),
                None => (rest, false),
            };
            let assembler = match stream {
                Stream::Stdout => &mut self.stdout,
                Stream::Stderr => &mut self.stderr,
            };
            assembler.push_segment(segment);
            if complete {
                if let Some((line, truncated)) = assembler.take() {
                    self.append(stream, line, truncated);
                }
                rest = &rest[segment.len() + 1..];
            } else {
                break;
            }
        }
    }

    /// Ends the session: flushes partial lines and refuses further text.
    pub(crate) fn finish(&mut self) {
        for stream in [Stream::Stdout, Stream::Stderr] {
            let assembler = match stream {
                Stream::Stdout => &mut self.stdout,
                Stream::Stderr => &mut self.stderr,
            };
            if let Some((line, truncated)) = assembler.take() {
                self.append(stream, line, truncated);
            }
        }
        self.ended = true;
    }

    /// The newest `limit` lines (at most [`MAX_TAIL_LINES`]) after `after`.
    pub(crate) fn tail(&self, limit: usize, after: u64) -> Tail {
        let limit = limit.clamp(1, MAX_TAIL_LINES);
        let newer = self.lines.iter().filter(|line| line.sequence > after);
        let count = newer.clone().count();
        let lines: Vec<Line> = newer.skip(count.saturating_sub(limit)).cloned().collect();
        let wanted = self.last_sequence.saturating_sub(after);
        Tail {
            truncated_lines: wanted > lines.len() as u64,
            lines,
            last_sequence: self.last_sequence,
            ended: self.ended,
        }
    }
}

#[derive(Default)]
struct Rings {
    rings: HashMap<String, OutputRing>,
    ended: VecDeque<String>,
}

/// Rings of every session this daemon has run recently, by session id.
#[derive(Default)]
pub(crate) struct OutputStore {
    inner: Mutex<Rings>,
}

impl OutputStore {
    fn rings(&self) -> std::sync::MutexGuard<'_, Rings> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub(crate) fn begin(&self, session_id: &str) {
        self.rings()
            .rings
            .insert(session_id.to_owned(), OutputRing::new());
    }

    pub(crate) fn push(&self, session_id: &str, stream: Stream, text: &str) {
        if let Some(ring) = self.rings().rings.get_mut(session_id) {
            ring.push_text(stream, text);
        }
    }

    pub(crate) fn end(&self, session_id: &str) {
        let mut rings = self.rings();
        let Some(ring) = rings.rings.get_mut(session_id) else {
            return;
        };
        if ring.ended {
            return;
        }
        ring.finish();
        rings.ended.push_back(session_id.to_owned());
        while rings.ended.len() > RETAINED_ENDED_SESSIONS {
            if let Some(oldest) = rings.ended.pop_front() {
                rings.rings.remove(&oldest);
            }
        }
    }

    /// `None` when no ring exists (never started, or evicted after ending).
    pub(crate) fn tail(&self, session_id: &str, limit: usize, after: u64) -> Option<Tail> {
        self.rings()
            .rings
            .get(session_id)
            .map(|ring| ring.tail(limit, after))
    }
}

/// Ends a session's ring when the supervisor leaves, however it leaves.
pub(crate) struct EndOnDrop<'a> {
    pub(crate) store: &'a OutputStore,
    pub(crate) session_id: &'a str,
}

impl Drop for EndOnDrop<'_> {
    fn drop(&mut self) {
        self.store.end(self.session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(tail: &Tail) -> Vec<&str> {
        tail.lines.iter().map(|line| line.text.as_str()).collect()
    }

    #[test]
    fn lines_are_split_numbered_and_tagged_with_their_stream() {
        let mut ring = OutputRing::new();
        ring.push_text(Stream::Stdout, "one\ntwo\r\npar");
        ring.push_text(Stream::Stderr, "bad\n");
        ring.push_text(Stream::Stdout, "tial\n");
        let tail = ring.tail(DEFAULT_TAIL_LINES, 0);
        assert_eq!(texts(&tail), ["one", "two", "bad", "partial"]);
        let sequences: Vec<u64> = tail.lines.iter().map(|line| line.sequence).collect();
        assert_eq!(sequences, [1, 2, 3, 4]);
        assert_eq!(tail.lines[2].stream, Stream::Stderr);
        assert!(!tail.truncated_lines && !tail.ended);
        assert_eq!(tail.last_sequence, 4);
    }

    #[test]
    fn a_partial_line_is_flushed_when_the_session_ends_and_later_text_is_ignored() {
        let mut ring = OutputRing::new();
        ring.push_text(Stream::Stdout, "no newline");
        assert!(ring.tail(10, 0).lines.is_empty());
        ring.finish();
        ring.push_text(Stream::Stdout, "late\n");
        let tail = ring.tail(10, 0);
        assert_eq!(texts(&tail), ["no newline"]);
        assert!(tail.ended);
    }

    #[test]
    fn the_cursor_returns_only_newer_lines() {
        let mut ring = OutputRing::new();
        ring.push_text(Stream::Stdout, "a\nb\nc\n");
        let first = ring.tail(10, 0);
        ring.push_text(Stream::Stdout, "d\n");
        let next = ring.tail(10, first.last_sequence);
        assert_eq!(texts(&next), ["d"]);
        assert!(!next.truncated_lines);
        assert!(ring.tail(10, next.last_sequence).lines.is_empty());
        assert!(ring.tail(10, 999).lines.is_empty());
    }

    #[test]
    fn a_limit_returns_the_newest_lines_and_flags_the_rest() {
        let mut ring = OutputRing::new();
        for number in 0..10 {
            ring.push_text(Stream::Stdout, &format!("l{number}\n"));
        }
        let tail = ring.tail(3, 0);
        assert_eq!(texts(&tail), ["l7", "l8", "l9"]);
        assert!(tail.truncated_lines);
        assert!(!ring.tail(10, 0).truncated_lines);
        assert!(!ring.tail(3, 7).truncated_lines);
    }

    #[test]
    fn the_limit_is_capped_at_the_maximum() {
        let mut ring = OutputRing::new();
        for _ in 0..(MAX_TAIL_LINES + 20) {
            ring.push_text(Stream::Stdout, "x\n");
        }
        let tail = ring.tail(10_000, 0);
        assert_eq!(tail.lines.len(), MAX_TAIL_LINES);
        assert!(tail.truncated_lines);
    }

    #[test]
    fn a_flood_never_exceeds_the_line_or_byte_caps() {
        let mut ring = OutputRing::new();
        let long = "é".repeat(MAX_LINE_BYTES);
        for _ in 0..(RING_LINES * 4) {
            ring.push_text(Stream::Stdout, &format!("{long}\n"));
        }
        let burst: String = (0..100_000).map(|n| format!("row {n}\n")).collect();
        ring.push_text(Stream::Stderr, &burst);
        ring.push_text(Stream::Stdout, &"y".repeat(MAX_LINE_BYTES * 50));
        ring.finish();
        assert_eq!(ring.lines.len(), RING_LINES);
        assert!(ring
            .lines
            .iter()
            .all(|line| line.text.len() <= MAX_LINE_BYTES));
        let last = ring.lines.back().unwrap();
        assert!(last.line_truncated);
        assert_eq!(last.text.len(), MAX_LINE_BYTES);
        assert_eq!(ring.last_sequence, (RING_LINES * 4 + 100_000 + 1) as u64);
        let all = ring.tail(MAX_TAIL_LINES, 0);
        assert!(all.truncated_lines);
        assert_eq!(all.lines.len(), MAX_TAIL_LINES);
    }

    #[test]
    fn long_lines_are_cut_on_a_character_boundary_and_flagged() {
        let mut ring = OutputRing::new();
        ring.push_text(Stream::Stdout, &format!("{}\n", "é".repeat(MAX_LINE_BYTES)));
        ring.push_text(Stream::Stdout, "short\n");
        let tail = ring.tail(10, 0);
        assert!(tail.lines[0].line_truncated);
        assert_eq!(tail.lines[0].text.len(), MAX_LINE_BYTES);
        assert!(!tail.lines[1].line_truncated);
    }

    #[test]
    fn a_line_split_across_deltas_is_cut_at_the_same_bound() {
        let mut ring = OutputRing::new();
        for _ in 0..10 {
            ring.push_text(Stream::Stdout, &"z".repeat(MAX_LINE_BYTES / 2));
        }
        ring.push_text(Stream::Stdout, "\n");
        let line = &ring.tail(1, 0).lines[0];
        assert_eq!(line.text.len(), MAX_LINE_BYTES);
        assert!(line.line_truncated);
    }

    #[test]
    fn the_store_keeps_only_a_few_ended_sessions() {
        let store = OutputStore::default();
        for number in 0..(RETAINED_ENDED_SESSIONS + 3) {
            let id = format!("s{number}");
            store.begin(&id);
            store.push(&id, Stream::Stdout, "hello\n");
            store.end(&id);
        }
        assert!(store.tail("s0", 10, 0).is_none());
        assert!(store.tail("s2", 10, 0).is_none());
        let kept = store.tail("s3", 10, 0).unwrap();
        assert!(kept.ended);
        assert_eq!(kept.lines.len(), 1);
        store.begin("live");
        assert!(!store.tail("live", 10, 0).unwrap().ended);
        assert!(store.tail("unknown", 10, 0).is_none());
        store.push("unknown", Stream::Stdout, "ignored\n");
        store.end("unknown");
    }

    #[test]
    fn the_guard_ends_the_session_when_dropped() {
        let store = OutputStore::default();
        store.begin("s");
        {
            let _guard = EndOnDrop {
                store: &store,
                session_id: "s",
            };
            store.push("s", Stream::Stdout, "tail");
        }
        let tail = store.tail("s", 10, 0).unwrap();
        assert!(tail.ended);
        assert_eq!(texts(&tail), ["tail"]);
    }
}
