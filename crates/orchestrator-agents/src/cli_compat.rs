//! Metadata-only compatibility checks for the installed agent executable.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent};
use tokio::sync::watch;

const OUTPUT_LIMIT: usize = 256 * 1024;
const MAX_CACHE_ENTRIES: usize = 64;
/// A good answer older than this is read again; until the new read lands, the
/// old answer is still served, so a refresh never refuses a working CLI.
const CACHE_TTL: Duration = Duration::from_secs(30);
/// A good answer older than this is dropped instead of served.
const STALE_LIMIT: Duration = Duration::from_secs(600);
/// How long one `--help` read may run. It is longer than any caller waits: a
/// CLI started through a wrapper (such as a terminal's shim script) while other
/// probes run can take over 2 s, and callers such as `agents.capabilities` must
/// answer inside a 2 s client timeout. The read runs on its own thread, so a
/// caller that stops waiting does not cancel it, and the next call finds it.
const PROBE_LIMIT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Executable {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
    #[cfg(unix)]
    file_id: (u64, u64, i64, i64),
}

impl Executable {
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = path.metadata()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            path: path.canonicalize()?,
            modified: metadata.modified()?,
            size: metadata.len(),
            #[cfg(unix)]
            file_id: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        })
    }

    pub fn matches(&self, path: &Path) -> bool {
        Self::read(path).is_ok_and(|current| current == *self)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompatibilityError {
    Unconfirmed,
    MissingFlag(String),
}

impl std::fmt::Display for CompatibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unconfirmed => write!(f, "Could not confirm CLI compatibility. Retry the check."),
            Self::MissingFlag(flag) => write!(
                f,
                "The installed CLI does not advertise required option {flag}."
            ),
        }
    }
}

impl std::error::Error for CompatibilityError {}

#[derive(Debug)]
pub struct CliHelp(HashSet<String>);

impl CliHelp {
    pub fn from_text(text: &str) -> Option<Self> {
        let flags: HashSet<_> = text
            .lines()
            .flat_map(option_names)
            .map(str::to_owned)
            .collect();
        (!flags.is_empty()).then_some(Self(flags))
    }

    pub fn offers(&self, flag: &str) -> bool {
        self.0.contains(flag)
    }

    /// Validate the options in the actual argv, not the prompt or option values.
    pub fn check(&self, agent: &str, args: &[String]) -> Result<(), CompatibilityError> {
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            if arg == "--" {
                break;
            }
            let flag = arg.split_once('=').map_or(arg.as_str(), |(name, _)| name);
            if !is_flag(flag) {
                continue;
            }
            if !self.offers(flag) {
                return Err(CompatibilityError::MissingFlag(flag.to_owned()));
            }
            if !arg.contains('=') && takes_value(agent, flag) {
                args.next();
            }
        }
        Ok(())
    }
}

// Values may look like flags. In particular, never report a Claude prompt
// or a model ID as an unsupported option.
fn takes_value(agent: &str, flag: &str) -> bool {
    match agent {
        "codex" => matches!(flag, "-c" | "-m" | "--sandbox" | "--add-dir"),
        "claude-code" => matches!(
            flag,
            "-p" | "--system-prompt"
                | "--append-system-prompt"
                | "--output-format"
                | "--permission-mode"
                | "--permission-prompts"
                | "--tools"
                | "--allowedTools"
                | "--disallowedTools"
                | "--mcp-config"
                | "--model"
                | "--effort"
                | "--agent"
                | "--plugin-dir"
                | "--add-dir"
                | "--session-id"
                | "--resume"
        ),
        "github-copilot" => matches!(
            flag,
            "--prompt"
                | "--output-format"
                | "--log-level"
                | "--usage-output-file"
                | "-C"
                | "--allow-tool"
                | "--deny-tool"
                | "--model"
                | "--mode"
                | "--max-autopilot-continues"
                | "--reasoning-effort"
                | "--max-ai-credits"
                | "--agent"
                | "--plugin-dir"
                | "--add-dir"
        ),
        _ => false,
    }
}

fn is_flag(word: &str) -> bool {
    if let Some(name) = word.strip_prefix("--") {
        !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    } else {
        word.len() == 2 && word.starts_with('-') && word.as_bytes()[1].is_ascii_alphabetic()
    }
}

fn option_names(line: &str) -> impl Iterator<Item = &str> {
    let mut declaration = Some(line.trim_start());
    std::iter::from_fn(move || {
        let line = declaration.take()?;
        let flag = line
            .split(|c: char| c.is_whitespace() || c == ',' || c == '=')
            .next()?;
        if !is_flag(flag) {
            return None;
        }
        declaration = line[flag.len()..].strip_prefix(',').map(str::trim_start);
        Some(flag)
    })
}

type Key = (String, Executable);
/// `None` until the read finishes, then its answer.
type Reading = watch::Receiver<Option<Option<Arc<CliHelp>>>>;

#[derive(Default)]
struct Entry {
    good: Option<(Instant, Arc<CliHelp>)>,
    reading: Option<Reading>,
}

#[derive(Default)]
pub struct CompatibilityCache(Arc<Mutex<HashMap<Key, Entry>>>);

impl CompatibilityCache {
    /// The help of `program`, waiting at most `wait` for a read. A failed read
    /// is not cached. Concurrent requests for the same binary share one read,
    /// which runs on its own thread for up to `PROBE_LIMIT` and is cached when
    /// it succeeds, even after every caller stopped waiting.
    pub async fn help(
        &self,
        agent: &str,
        program: &Path,
        executable: &Executable,
        wait: Duration,
    ) -> Option<Arc<CliHelp>> {
        if !executable.matches(program) {
            return None;
        }
        let key = (agent.to_owned(), executable.clone());
        let mut reading = {
            let mut entries = self.0.lock().ok()?;
            entries.retain(|_, entry| {
                entry.reading.is_some()
                    || entry
                        .good
                        .as_ref()
                        .is_some_and(|(at, _)| at.elapsed() < STALE_LIMIT)
            });
            if entries.len() >= MAX_CACHE_ENTRIES && !entries.contains_key(&key) {
                return None;
            }
            let entry = entries.entry(key.clone()).or_default();
            let good = entry.good.clone();
            if good
                .as_ref()
                .is_some_and(|(at, _)| at.elapsed() < CACHE_TTL)
            {
                return good.map(|(_, help)| help);
            }
            if entry.reading.is_none() {
                entry.reading = self.start(key, program.to_path_buf());
            }
            if let Some((_, help)) = good {
                return Some(help);
            }
            entry.reading.clone()?
        };
        let answer = tokio::time::timeout(wait, reading.wait_for(Option::is_some))
            .await
            .ok()?
            .ok()?
            .clone()
            .flatten();
        answer.filter(|_| executable.matches(program))
    }

    /// Starts the read for `key` on its own thread; `None` if it cannot start.
    fn start(&self, key: Key, program: PathBuf) -> Option<Reading> {
        let (sender, receiver) = watch::channel(None);
        let entries = self.0.clone();
        std::thread::Builder::new()
            .name("cli-help".into())
            .spawn(move || {
                let answer = std::panic::catch_unwind(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok()?
                        .block_on(read_help(&key.0, &program, PROBE_LIMIT))
                })
                .ok()
                .flatten();
                if let Ok(mut entries) = entries.lock() {
                    if let Some(entry) = entries.get_mut(&key) {
                        entry.reading = None;
                        if let Some(help) = &answer {
                            entry.good = Some((Instant::now(), help.clone()));
                        }
                    }
                }
                let _ = sender.send(Some(answer));
            })
            .ok()?;
        Some(receiver)
    }
}

async fn read_help(agent: &str, program: &Path, limit: Duration) -> Option<Arc<CliHelp>> {
    let args = if agent == "codex" {
        &["exec", "--help"][..]
    } else {
        &["--help"]
    };
    let mut child = ChildSupervisor::launch(&LaunchRequest {
        git_guard: false,
        program: program.to_path_buf(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        working_dir: std::env::temp_dir(),
        env_names: Vec::new(),
    })
    .ok()?;
    let probe = async {
        let mut remaining = OUTPUT_LIMIT;
        let mut flags = HashSet::new();
        while let Some(event) = child.next_event().await {
            match event {
                SupervisionEvent::StdoutLine(line) => {
                    remaining = remaining.checked_sub(line.len() + 1)?;
                    flags.extend(option_names(&line).map(str::to_owned));
                }
                SupervisionEvent::StderrLine(line) => {
                    remaining = remaining.checked_sub(line.len() + 1)?;
                }
                SupervisionEvent::Exited { code: Some(0), .. } if !flags.is_empty() => {
                    return Some(Arc::new(CliHelp(flags)));
                }
                SupervisionEvent::Exited { .. } => return None,
            }
        }
        None
    };
    tokio::time::timeout(limit, probe).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow_agent::AGENT_ID;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn agent_and_plugin_dir_values_are_not_read_as_flag_names() {
        let help = CliHelp::from_text("  --agent <id>\n  --plugin-dir <dir>\n").unwrap();
        for agent in ["claude-code", "github-copilot"] {
            for flag in ["--agent", "--plugin-dir"] {
                assert_eq!(help.check(agent, &argv(&[flag, "--unadvertised"])), Ok(()));
            }
        }
        assert_eq!(
            help.check(
                "claude-code",
                &argv(&["--agent", AGENT_ID, "--plugin-dir", "/x"])
            ),
            Ok(())
        );
    }

    /// A CLI slower than the caller's wait (a shim under load) is still read:
    /// the read outlives the call, and a later call gets its answer.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn slow_help_finishes_after_the_caller_stops_waiting() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("slow-cli");
        std::fs::write(&program, "#!/bin/sh\nsleep 1\necho '  --model <id>'\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let executable = Executable::read(&program).unwrap();
        let cache = CompatibilityCache::default();
        let wait = Duration::from_millis(100);

        assert!(cache
            .help("claude-code", &program, &executable, wait)
            .await
            .is_none());
        let deadline = Instant::now() + Duration::from_secs(10);
        let help = loop {
            if let Some(help) = cache.help("claude-code", &program, &executable, wait).await {
                break help;
            }
            assert!(
                Instant::now() < deadline,
                "the background read never landed"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        assert!(help.offers("--model"));
    }
}
