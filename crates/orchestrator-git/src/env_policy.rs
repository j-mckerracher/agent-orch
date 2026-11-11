//! Environment allowlist for child processes (plan section 13, "Environment
//! handling").
//!
//! Agents and validation commands run with an explicit allowlist, never the
//! whole parent environment. A child receives:
//!
//! 1. the baseline below: what a process needs to start and reach the
//!    network (locale, temp, home, proxy and CA settings, Windows system
//!    roots) and no credentials;
//! 2. names the launcher adds for one provider (for example the provider's
//!    API key variable); and
//! 3. names the user lists in `AGENT_ORCH_ENV_PASSTHROUGH` (comma
//!    separated), the documented way to forward other authentication or
//!    toolchain variables.
//!
//! Only names are ever listed or logged; values are never recorded. A rejected
//! `AGENT_ORCH_ENV_PASSTHROUGH` entry is reported by position and, when the
//! text before any `=` looks like a plain variable name, that name. The rest
//! of the entry is never printed, so a mis-pasted `NAME=secret` stays out of
//! the daemon log.
//! Variables whose name starts with `AGENT_ORCH_` are orchestrator
//! configuration (data directory, program overrides, fault gates) and are
//! never forwarded, even when listed: the data directory locates the daemon's
//! IPC token.
//!
//! This is hardening of what the child inherits, not containment. A child
//! runs as the user's OS account and can read any file that account can,
//! including files under its home directory.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::Write;

/// Variable naming extra parent variables to forward, comma separated.
pub const PASSTHROUGH_VARIABLE: &str = "AGENT_ORCH_ENV_PASSTHROUGH";

/// Prefix of orchestrator-internal configuration that is never forwarded.
const INTERNAL_PREFIX: &str = "AGENT_ORCH_";

/// Exact baseline names. Windows matches these case-insensitively because the
/// platform treats variable names that way.
const BASELINE: &[&str] = &[
    // Process start and locale.
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "TERM",
    "TZ",
    "TMPDIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    // Network reachability. Proxy URLs can embed credentials; forwarding them
    // is required for agents to work behind a proxy.
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    // Locale categories (POSIX and glibc); an arbitrary `LC_*` name is not one.
    "LC_ALL",
    "LC_CTYPE",
    "LC_COLLATE",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
    // Windows process start and profile locations.
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "PATHEXT",
    "TEMP",
    "TMP",
    "USERNAME",
    "USERDOMAIN",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "CommonProgramW6432",
    "ALLUSERSPROFILE",
    "PUBLIC",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
];

/// Decision for one environment variable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPolicy {
    extra: Vec<String>,
    case_insensitive: bool,
}

impl EnvPolicy {
    /// Baseline plus the given provider names. Invalid names are dropped;
    /// callers pass compile-time constants.
    pub fn with_names<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::with_names_and_case(names, cfg!(windows))
    }

    /// As [`EnvPolicy::with_names`] with the platform's name-matching rule
    /// chosen by the caller, so both rules are testable on one host.
    pub fn with_names_and_case<I, S>(names: I, case_insensitive: bool) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            extra: names
                .into_iter()
                .map(|name| name.as_ref().to_owned())
                .filter(|name| is_forwardable_name(name))
                .collect(),
            case_insensitive,
        }
    }

    /// Baseline plus provider names plus the user's
    /// `AGENT_ORCH_ENV_PASSTHROUGH` list. Rejected user entries are reported
    /// to stderr without their values (see [`Rejection`]).
    pub fn for_launch<I, S>(provider_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::for_launch_with(
            provider_names,
            std::env::var_os(PASSTHROUGH_VARIABLE).as_deref(),
            &mut std::io::stderr().lock(),
        )
    }

    /// As [`EnvPolicy::for_launch`] with the passthrough list and the report
    /// sink chosen by the caller. A failing sink never changes the policy.
    pub fn for_launch_with<I, S>(
        provider_names: I,
        passthrough: Option<&OsStr>,
        report: &mut dyn Write,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut policy = Self::with_names(provider_names);
        if let Some(raw) = passthrough {
            let (accepted, rejected) = parse_passthrough(&raw.to_string_lossy());
            for rejection in &rejected {
                let _ = writeln!(
                    report,
                    "orchestrator: ignoring {PASSTHROUGH_VARIABLE} entry {rejection}"
                );
            }
            policy.extra.extend(accepted);
        }
        policy
    }

    /// Whether `name` is forwarded.
    pub fn allows(&self, name: &OsStr) -> bool {
        let Some(name) = name.to_str() else {
            return false;
        };
        if is_internal(name) {
            return false;
        }
        let same = |candidate: &str| {
            if self.case_insensitive {
                candidate.eq_ignore_ascii_case(name)
            } else {
                candidate == name
            }
        };
        BASELINE.iter().any(|candidate| same(candidate))
            || self.extra.iter().any(|candidate| same(candidate))
    }

    /// The allowed subset of `parent`, in the parent's order.
    pub fn filter<I>(&self, parent: I) -> Vec<(OsString, OsString)>
    where
        I: IntoIterator<Item = (OsString, OsString)>,
    {
        parent
            .into_iter()
            .filter(|(name, _)| self.allows(name))
            .collect()
    }

    /// The allowed subset of this process's environment.
    pub fn from_process_env(&self) -> Vec<(OsString, OsString)> {
        self.filter(std::env::vars_os())
    }

    /// Clears `command`'s environment and installs the allowed subset.
    pub fn apply(&self, command: &mut std::process::Command) {
        command.env_clear().envs(self.from_process_env());
    }
}

fn is_internal(name: &str) -> bool {
    name.len() >= INTERNAL_PREFIX.len()
        && name.is_char_boundary(INTERNAL_PREFIX.len())
        && name[..INTERNAL_PREFIX.len()].eq_ignore_ascii_case(INTERNAL_PREFIX)
}

/// A listable name: non-empty, no `=`, `*`, NUL, whitespace or control
/// characters, and not orchestrator-internal. Wildcards are refused because
/// matching is by exact name; a `*` entry would silently forward nothing.
pub fn is_forwardable_name(name: &str) -> bool {
    !name.is_empty()
        && !is_internal(name)
        && name
            .chars()
            .all(|c| c != '=' && c != '*' && c != '\0' && !c.is_whitespace() && !c.is_control())
}

/// One rejected `AGENT_ORCH_ENV_PASSTHROUGH` entry, described without
/// disclosing its text. A user may paste `NAME=secret` or a bare secret, so
/// only a plain-looking name before any `=` and a fixed reason are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// 1-based position among the non-blank entries.
    pub position: usize,
    /// The text before the first `=`, only when it is a plain variable name.
    pub name: Option<String>,
    pub reason: &'static str,
}

impl fmt::Display for Rejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}", self.position)?;
        if let Some(name) = &self.name {
            write!(formatter, " ({name})")?;
        }
        write!(formatter, ": {}", self.reason)
    }
}

impl Rejection {
    fn new(position: usize, entry: &str) -> Self {
        let name = entry
            .split('=')
            .next()
            .filter(|candidate| {
                !candidate.is_empty()
                    && candidate.len() <= 64
                    && candidate
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .map(str::to_owned);
        let reason = if entry.contains('*') {
            "wildcards are not supported; list each variable name"
        } else if entry.contains('=') {
            "list the variable name only, never NAME=value"
        } else if is_internal(entry) {
            "orchestrator-internal variables are never forwarded"
        } else {
            "not a forwardable variable name"
        };
        Self {
            position,
            name,
            reason,
        }
    }
}

/// Splits a comma-separated list into `(accepted names, rejections)`. Blank
/// entries are skipped, not rejected, so a trailing comma is harmless.
pub fn parse_passthrough(raw: &str) -> (Vec<String>, Vec<Rejection>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let entries = raw.split(',').map(str::trim).filter(|e| !e.is_empty());
    for (index, entry) in entries.enumerate() {
        if is_forwardable_name(entry) {
            accepted.push(entry.to_owned());
        } else {
            rejected.push(Rejection::new(index + 1, entry));
        }
    }
    (accepted, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> OsString {
        OsString::from(value)
    }

    fn pairs(names: &[&str]) -> Vec<(OsString, OsString)> {
        names.iter().map(|n| (os(n), os("value"))).collect()
    }

    fn kept(policy: &EnvPolicy, names: &[&str]) -> Vec<String> {
        policy
            .filter(pairs(names))
            .into_iter()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn baseline_keeps_process_start_and_network_names_only() {
        let policy = EnvPolicy::with_names_and_case(Vec::<String>::new(), false);
        assert_eq!(
            kept(
                &policy,
                &[
                    "PATH",
                    "HOME",
                    "LC_ALL",
                    "HTTPS_proxy",
                    "https_proxy",
                    "SSL_CERT_FILE"
                ]
            ),
            ["PATH", "HOME", "LC_ALL", "https_proxy", "SSL_CERT_FILE"]
        );
    }

    #[test]
    fn credentials_and_agent_sockets_are_not_in_the_baseline() {
        let policy = EnvPolicy::with_names_and_case(Vec::<String>::new(), false);
        let secrets = [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "SSH_AUTH_SOCK",
            "NPM_TOKEN",
            "CARGO_REGISTRY_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ];
        assert!(kept(&policy, &secrets).is_empty());
    }

    #[test]
    fn provider_names_extend_the_baseline() {
        let policy = EnvPolicy::with_names_and_case(["OPENAI_API_KEY", "CODEX_HOME"], false);
        assert_eq!(
            kept(
                &policy,
                &["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "CODEX_HOME"]
            ),
            ["OPENAI_API_KEY", "CODEX_HOME"]
        );
    }

    #[test]
    fn orchestrator_configuration_is_never_forwarded_even_when_listed() {
        let policy = EnvPolicy::with_names_and_case(
            [
                "AGENT_ORCH_DATA_DIR",
                "agent_orch_fault_gate",
                "OPENAI_API_KEY",
            ],
            true,
        );
        assert_eq!(
            kept(
                &policy,
                &[
                    "AGENT_ORCH_DATA_DIR",
                    "AGENT_ORCH_FAULT_GATE",
                    "OPENAI_API_KEY"
                ]
            ),
            ["OPENAI_API_KEY"]
        );
        assert!(!is_forwardable_name("AGENT_ORCH_ENV_PASSTHROUGH"));
        assert!(!is_forwardable_name("Agent_Orch_X"));
    }

    #[test]
    fn unix_names_are_case_sensitive_and_windows_names_are_not() {
        let sensitive = EnvPolicy::with_names_and_case(["MY_KEY"], false);
        assert_eq!(
            kept(&sensitive, &["Path", "SYSTEMROOT", "my_key", "lc_all"]).len(),
            0
        );
        let insensitive = EnvPolicy::with_names_and_case(["MY_KEY"], true);
        assert_eq!(
            kept(
                &insensitive,
                &["Path", "SYSTEMROOT", "my_key", "lc_all", "Other"]
            ),
            ["Path", "SYSTEMROOT", "my_key", "lc_all"]
        );
    }

    #[test]
    fn non_unicode_names_are_dropped() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let policy = EnvPolicy::with_names_and_case(Vec::<String>::new(), false);
            let bad = OsString::from_vec(vec![b'P', 0xff]);
            assert!(!policy.allows(&bad));
        }
    }

    #[test]
    fn passthrough_list_splits_and_rejects_bad_entries() {
        let (accepted, rejected) = parse_passthrough(
            " OPENAI_API_KEY , ,FAKE=1,AGENT_ORCH_DATA_DIR,HAS SPACE,PATH,\tFOO_BAR,",
        );
        assert_eq!(accepted, ["OPENAI_API_KEY", "PATH", "FOO_BAR"]);
        let described: Vec<_> = rejected.iter().map(ToString::to_string).collect();
        assert_eq!(
            described,
            [
                "#2 (FAKE): list the variable name only, never NAME=value",
                "#3 (AGENT_ORCH_DATA_DIR): orchestrator-internal variables are never forwarded",
                "#4: not a forwardable variable name",
            ]
        );
    }

    #[test]
    fn a_wildcard_entry_is_rejected_with_a_clear_reason() {
        let (accepted, rejected) = parse_passthrough("*,AWS_*, OK");
        assert_eq!(accepted, ["OK"]);
        assert_eq!(rejected.len(), 2);
        assert!(rejected.iter().all(|r| r.reason.contains("wildcards")));
        assert!(!is_forwardable_name("*"));
    }

    #[test]
    fn locale_names_are_exact_not_a_prefix() {
        let policy = EnvPolicy::with_names_and_case(Vec::<String>::new(), false);
        assert_eq!(
            kept(
                &policy,
                &["LC_ALL", "LC_TIME", "LC_SECRET", "LC_", "LC_API_KEY"]
            ),
            ["LC_ALL", "LC_TIME"]
        );
    }

    #[test]
    fn rejected_entries_never_report_a_value() {
        const CANARY: &str = "sk-LEAKCANARY-123";
        let raw = format!(
            "OPENAI_API_KEY={CANARY},{CANARY} with space,Bearer {CANARY},{CANARY}=x y,=={CANARY},\
             AGENT_ORCH_DATA_DIR={CANARY},*{CANARY},OK_NAME"
        );
        let mut report = Vec::new();
        let policy =
            EnvPolicy::for_launch_with(Vec::<String>::new(), Some(OsStr::new(&raw)), &mut report);
        let report = String::from_utf8(report).unwrap();
        assert!(!report.contains(CANARY), "{report}");
        assert!(!report.contains("sk-"), "{report}");
        assert!(report.contains("OPENAI_API_KEY"), "{report}");
        assert_eq!(report.lines().count(), 7, "{report}");
        assert_eq!(kept(&policy, &["OK_NAME", "OPENAI_API_KEY"]), ["OK_NAME"]);
    }

    #[test]
    fn non_ascii_name_boundaries_do_not_panic() {
        let policy = EnvPolicy::with_names_and_case(Vec::<String>::new(), true);
        assert!(!policy.allows(OsStr::new("L\u{e9}")));
        assert!(!policy.allows(OsStr::new("\u{e9}\u{e9}\u{e9}")));
    }
}
