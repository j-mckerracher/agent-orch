//! Metadata-only compatibility checks for the installed agent executable.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent};
use tokio::sync::OnceCell;

const OUTPUT_LIMIT: usize = 256 * 1024;
const MAX_CACHE_ENTRIES: usize = 64;
const CACHE_TTL: Duration = Duration::from_secs(30);

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

type Probe = OnceCell<Option<(Instant, Arc<CliHelp>)>>;

#[derive(Default)]
pub struct CompatibilityCache(Mutex<HashMap<(String, Executable), Arc<Probe>>>);

impl CompatibilityCache {
    /// A failed probe is not cached. Concurrent requests for the same binary
    /// share a read; no subprocess runs with the map locked.
    pub async fn help(
        &self,
        agent: &str,
        program: &Path,
        executable: &Executable,
        limit: Duration,
    ) -> Option<Arc<CliHelp>> {
        if !executable.matches(program) {
            return None;
        }
        let key = (agent.to_owned(), executable.clone());
        let cell = {
            let mut entries = self.0.lock().ok()?;
            entries.retain(|_, entry| match entry.get() {
                Some(Some((at, _))) => at.elapsed() < CACHE_TTL,
                _ => Arc::strong_count(entry) > 1,
            });
            if entries.len() >= MAX_CACHE_ENTRIES && !entries.contains_key(&key) {
                return None;
            }
            entries.entry(key.clone()).or_default().clone()
        };
        let result = cell
            .get_or_init(|| async {
                read_help(agent, program, limit)
                    .await
                    .map(|help| (Instant::now(), help))
            })
            .await
            .as_ref()
            .map(|(_, help)| help.clone());
        if result.is_none() {
            if let Ok(mut entries) = self.0.lock() {
                if entries
                    .get(&key)
                    .is_some_and(|entry| Arc::ptr_eq(entry, &cell))
                {
                    entries.remove(&key);
                }
            }
        }
        result.filter(|_| executable.matches(program))
    }
}

async fn read_help(agent: &str, program: &Path, limit: Duration) -> Option<Arc<CliHelp>> {
    let args = if agent == "codex" {
        &["exec", "--help"][..]
    } else {
        &["--help"]
    };
    let mut child = ChildSupervisor::launch(&LaunchRequest {
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
}
