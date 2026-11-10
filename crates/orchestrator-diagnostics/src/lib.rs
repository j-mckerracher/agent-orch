//! Structured, redacted diagnostics for the daemon, its tests and the desktop
//! host. See [`redact`] for what is masked and [`Diagnostics`] for the line
//! format.

pub mod redact;

pub use redact::{is_sensitive_key, redact, truncate, MASK};

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Longest value kept in one diagnostic field, after redaction.
pub const MAX_FIELD_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

enum Sink {
    Stderr,
    Memory(Mutex<Vec<String>>),
}

/// Emits one JSON object per line: `ts` (UTC, RFC 3339), `level`,
/// `component`, `event`, then the caller's fields. Every field value is
/// redacted and truncated, and a field whose *name* says it is a credential is
/// masked outright. Emission never fails or panics: diagnostics must not be a
/// new way for the daemon to stop.
#[derive(Clone)]
pub struct Diagnostics {
    sink: Arc<Sink>,
}

impl fmt::Debug for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Diagnostics")
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::stderr()
    }
}

impl Diagnostics {
    /// Writes lines to the process stderr, which the desktop host and the
    /// lifecycle test redirect into `daemon.log`.
    pub fn stderr() -> Self {
        Self {
            sink: Arc::new(Sink::Stderr),
        }
    }

    /// Collects lines in memory so tests can assert on them.
    pub fn memory() -> Self {
        Self {
            sink: Arc::new(Sink::Memory(Mutex::new(Vec::new()))),
        }
    }

    /// Lines emitted so far (memory sinks only; empty for stderr).
    pub fn lines(&self) -> Vec<String> {
        match &*self.sink {
            Sink::Memory(lines) => lines.lock().map(|l| l.clone()).unwrap_or_default(),
            Sink::Stderr => Vec::new(),
        }
    }

    pub fn info(&self, component: &str, event: &str, fields: &[(&str, &str)]) {
        self.emit(Level::Info, component, event, fields);
    }

    pub fn warn(&self, component: &str, event: &str, fields: &[(&str, &str)]) {
        self.emit(Level::Warn, component, event, fields);
    }

    pub fn error(&self, component: &str, event: &str, fields: &[(&str, &str)]) {
        self.emit(Level::Error, component, event, fields);
    }

    pub fn emit(&self, level: Level, component: &str, event: &str, fields: &[(&str, &str)]) {
        let line = format_event(level, component, event, fields);
        match &*self.sink {
            Sink::Stderr => {
                let stderr = std::io::stderr();
                let mut handle = stderr.lock();
                let _ = writeln!(handle, "{line}");
            }
            Sink::Memory(lines) => {
                if let Ok(mut lines) = lines.lock() {
                    lines.push(line);
                }
            }
        }
    }
}

/// Builds one diagnostic line without emitting it.
pub fn format_event(level: Level, component: &str, event: &str, fields: &[(&str, &str)]) -> String {
    let mut object = serde_json::Map::new();
    object.insert("ts".into(), now_rfc3339().into());
    object.insert("level".into(), level.as_str().into());
    object.insert("component".into(), clean(component).into());
    object.insert("event".into(), clean(event).into());
    for (key, value) in fields {
        let name = if matches!(*key, "ts" | "level" | "component" | "event") {
            format!("field_{key}")
        } else {
            clean(key)
        };
        let shown = if is_sensitive_key(key) {
            MASK.to_owned()
        } else {
            clean(value)
        };
        object.insert(name, shown.into());
    }
    serde_json::Value::Object(object).to_string()
}

fn clean(text: &str) -> String {
    truncate(&redact(text), MAX_FIELD_CHARS)
}

/// Routes panics through `diagnostics` as a redacted `panic` event, so a
/// panicking supervisor or request thread leaves a structured record rather
/// than a raw message that may quote a credential.
pub fn install_panic_hook(diagnostics: Diagnostics, component: &'static str) {
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
        diagnostics.error(
            component,
            "panic",
            &[("message", &message), ("location", &location), ("thread", &thread)],
        );
    }));
}

/// The redacted tail of a log file: at most `max_bytes` from the end, starting
/// on a line boundary when the file was cut. Reading, not writing: the log is
/// left untouched.
pub fn tail_redacted(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let cut = len > max_bytes;
    if cut {
        file.seek(SeekFrom::Start(len - max_bytes))?;
    }
    let mut raw = Vec::new();
    file.take(max_bytes).read_to_end(&mut raw)?;
    let mut text = String::from_utf8_lossy(&raw).into_owned();
    if cut {
        if let Some(newline) = text.find('\n') {
            text.drain(..=newline);
        }
    }
    Ok(redact(text.trim_end()))
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339_utc(secs)
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (civil-from-days algorithm).
pub fn rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}
