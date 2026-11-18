//! Bounded runs of child processes, shared by the `gh` and `treehouse` seams.
//!
//! The child is killed and reaped at the deadline. Each output stream is read
//! to the end on its own thread, keeping a fixed number of bytes, so a chatty
//! child never blocks on a full pipe.

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Stderr kept from a child; anything more is read and discarded.
const MAX_STDERR_BYTES: usize = 16 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long a stream may take to finish once the child has exited.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Why a bounded run produced no [`Output`]. Fixed text only: the child's own
/// output never reaches an error.
#[derive(Debug)]
pub(crate) enum RunError {
    /// The program did not start, for example because it is not installed.
    Spawn(io::Error),
    /// The child was still running at the deadline. It was killed and reaped.
    Timeout,
    /// Waiting on the child or reading its output failed. The code names the step.
    Failed(&'static str),
}

/// Runs `command` to completion within `timeout`, keeping at most `max_stdout`
/// bytes of stdout. Stdin is null; stderr is kept up to a fixed cap.
pub(crate) fn run_bounded(
    command: &mut Command,
    timeout: Duration,
    max_stdout: usize,
) -> Result<Output, RunError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RunError::Spawn)?;
    let stdout = drain(child.stdout.take(), max_stdout);
    let stderr = drain(child.stderr.take(), MAX_STDERR_BYTES);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout);
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Failed("wait"));
            }
        }
    };
    let stdout = stdout
        .recv_timeout(DRAIN_GRACE)
        .map_err(|_| RunError::Failed("output"))?;
    let stderr = stderr
        .recv_timeout(DRAIN_GRACE)
        .map_err(|_| RunError::Failed("output"))?;
    if stdout.overflow {
        return Err(RunError::Failed("output_too_large"));
    }
    Ok(Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

struct Drained {
    bytes: Vec<u8>,
    overflow: bool,
}

/// Reads `stream` to the end on its own thread, keeping at most `cap` bytes and discarding the
/// rest, so a chatty child never blocks on a full pipe.
fn drain<R: Read + Send + 'static>(stream: Option<R>, cap: usize) -> mpsc::Receiver<Drained> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut overflow = false;
        if let Some(mut stream) = stream {
            let mut buffer = [0u8; 8192];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        let room = cap.saturating_sub(bytes.len());
                        overflow |= read > room;
                        bytes.extend_from_slice(&buffer[..read.min(room)]);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
        let _ = sender.send(Drained { bytes, overflow });
    });
    receiver
}
