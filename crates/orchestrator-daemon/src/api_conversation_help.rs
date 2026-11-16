//! What an installed agent CLI still offers, read from its `--help` text
//! (DPES-1106). A version the daemon has not verified runs when its help lists
//! every flag the chat argv needs at the requested permission level. It is
//! refused only when the help was read and a required flag is absent.
//!
//! The help text is read only; no agent turn runs here. A help text that cannot
//! be read (the probe times out, fails, or is too large) is `Unreadable`, and an
//! unreadable help never refuses a version.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orchestrator_agents::{ClaudeCodeAdapter, CodexAdapter};
use orchestrator_core::permission::PermissionLevel;

use super::conversation_agent::{kill_probe, CLAUDE, CODEX};

/// The help text of a CLI is read in at most this long.
const HELP_TIMEOUT: Duration = Duration::from_secs(5);
/// Enough for any real `--help`; a longer text is not read as help.
const MAX_HELP_OUTPUT: usize = 256 * 1024;
const POLL: Duration = Duration::from_millis(10);

/// What one read of an installed CLI's help established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Help {
    /// Every flag the help lists, as the words it prints.
    Listed(HashSet<String>),
    /// The help could not be read or does not show flags. Never a refusal.
    Unreadable,
}

/// Flags the chat argv for `level` passes, in the order the adapter builds
/// them. Read from the adapter's own argv so the check cannot drift from what
/// runs. A value never starts with `-`, so the flag names are the tokens that do.
pub(crate) fn required_flags(agent: &str, level: PermissionLevel) -> Vec<String> {
    let argv: Vec<String> = if agent == CODEX {
        CodexAdapter::args_for_conversation(level, "p", None, None).unwrap_or_default()
    } else if agent == CLAUDE {
        ClaudeCodeAdapter::args_for_conversation(level, "p", None, None).unwrap_or_default()
    } else {
        // Copilot's chat argv is built with a run directory and a usage file that
        // this module does not have; it is checked against its own top-level flags.
        [
            "--prompt",
            "--output-format",
            "--no-ask-user",
            "--log-level",
        ]
        .map(str::to_owned)
        .to_vec()
    };
    // Everything after `--` is the prompt, not a flag; `--` itself is a separator.
    // A flag is required under the name the argv uses, short or long. The
    // selection flags (`-m`, `-c`) are optional and are required only when the
    // chat passes them with a choice; the base chat argv has none of them.
    let mut flags: Vec<String> = Vec::new();
    for token in argv.into_iter().take_while(|token| token != "--") {
        if token.starts_with('-') && !flags.contains(&token) {
            flags.push(token);
        }
    }
    flags
}

/// The first required flag the help does not list, or `None` when all are listed.
pub(crate) fn missing_flag(help: &HashSet<String>, required: &[String]) -> Option<String> {
    required.iter().find(|flag| !help.contains(*flag)).cloned()
}

/// The flag words a help text names, long (`--flag`) and short (`-f`). Help lists
/// `--flag <VALUE>` and `-f, --flag`; each word is split on whitespace, commas and
/// `=`, so a flag is found wherever it is named, in either form.
pub(crate) fn listed_flags(text: &str) -> HashSet<String> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == '=')
        .filter(|word| is_flag(word))
        .map(|word| word.trim_end_matches(['.', ';', ')', '`']).to_owned())
        .collect()
}

/// `--name` or a single short letter after one dash. A bare `-` or `--` is not a flag.
fn is_flag(word: &str) -> bool {
    match word.strip_prefix("--") {
        Some(name) => !name.is_empty(),
        None => {
            word.len() == 2 && word.starts_with('-') && word.as_bytes()[1].is_ascii_alphabetic()
        }
    }
}

/// Reads the help of `program` for `agent`, once per (program, version) for the
/// daemon's lifetime. `subcommand` is `exec` for Codex, which has the chat's flags
/// under `exec`; the others list them at top level.
pub(crate) fn read_help(program: &Path, agent: &str) -> Help {
    let mut command = Command::new(program);
    if agent == CODEX {
        command.args(["exec", "--help"]);
    } else {
        command.arg("--help");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let Ok(mut child) = command.spawn() else {
        return Help::Unreadable;
    };
    // Drained while the child runs: a help text larger than the pipe buffer
    // would otherwise block the child until the timeout.
    let Some(stdout) = child.stdout.take() else {
        kill_probe(&mut child);
        return Help::Unreadable;
    };
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let read = stdout
            .take(MAX_HELP_OUTPUT as u64 + 1)
            .read_to_end(&mut output);
        (read.is_ok(), output)
    });
    let deadline = Instant::now() + HELP_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) | Err(_) => {
                kill_probe(&mut child);
                let _ = reader.join();
                return Help::Unreadable;
            }
        }
    };
    let Ok((true, output)) = reader.join() else {
        return Help::Unreadable;
    };
    if !status.success() || output.len() > MAX_HELP_OUTPUT {
        return Help::Unreadable;
    }
    let text = String::from_utf8_lossy(&output);
    let flags = listed_flags(&text);
    if flags.is_empty() {
        // Output with no flags is not a help text this probe can read.
        return Help::Unreadable;
    }
    Help::Listed(flags)
}

/// One help read per program and version, kept for the daemon's lifetime, so a
/// message never waits on a second probe of the same CLI.
#[derive(Default)]
pub(crate) struct HelpCache(Mutex<HashMap<(PathBuf, String), Arc<Help>>>);

impl HelpCache {
    pub(crate) fn get_or_read(&self, program: &Path, agent: &str, version: &str) -> Arc<Help> {
        let key = (program.to_path_buf(), format!("{agent}@{version}"));
        if let Some(help) = self.lock().get(&key) {
            return help.clone();
        }
        let help = Arc::new(read_help(program, agent));
        self.lock().entry(key).or_insert(help).clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(PathBuf, String), Arc<Help>>> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_read_from_help_in_long_and_short_form() {
        let text = "  -s, --sandbox <SANDBOX_MODE>\n      --json\n  -m, --model <MODEL>\n      --skip-git-repo-check";
        let flags = listed_flags(text);
        for flag in [
            "--sandbox",
            "--json",
            "--model",
            "--skip-git-repo-check",
            "-s",
            "-m",
        ] {
            assert!(flags.contains(flag), "{flag} missing from {flags:?}");
        }
    }

    #[test]
    fn a_bare_dash_or_double_dash_is_never_a_flag() {
        let flags = listed_flags("usage: tool -- prompt - value --");
        assert!(flags.is_empty(), "{flags:?}");
    }

    #[test]
    fn a_missing_required_flag_is_named_and_a_present_set_is_not() {
        let help: HashSet<String> = ["--json", "--sandbox"].map(str::to_owned).into();
        let required = vec!["--json".to_owned(), "--ephemeral".to_owned()];
        assert_eq!(
            missing_flag(&help, &required),
            Some("--ephemeral".to_owned())
        );
        assert_eq!(missing_flag(&help, &["--json".to_owned()]), None);
    }

    #[test]
    fn the_codex_chat_flags_are_the_adapters_own_argv() {
        let restricted = required_flags(CODEX, PermissionLevel::Restricted);
        assert_eq!(
            restricted,
            [
                "--json",
                "--sandbox",
                "--skip-git-repo-check",
                "--ephemeral"
            ]
        );
        assert!(
            required_flags(CODEX, PermissionLevel::Auto).contains(&"--approve-for-me".to_owned())
        );
        assert!(!required_flags(CODEX, PermissionLevel::Full).contains(&"--sandbox".to_owned()));
    }

    #[test]
    fn the_claude_chat_flags_include_the_short_print_flag() {
        let restricted = required_flags(CLAUDE, PermissionLevel::Restricted);
        assert!(restricted.contains(&"-p".to_owned()), "{restricted:?}");
        assert!(
            restricted.contains(&"--output-format".to_owned()),
            "{restricted:?}"
        );
    }
}
