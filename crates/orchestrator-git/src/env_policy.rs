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
//! Only names are ever listed or logged; values are never recorded.
//! Variables whose name starts with `AGENT_ORCH_` are orchestrator
//! configuration (data directory, program overrides, fault gates) and are
//! never forwarded, even when listed: the data directory locates the daemon's
//! IPC token.
//!
//! This is hardening of what the child inherits, not containment. A child
//! runs as the user's OS account and can read any file that account can,
//! including files under its home directory.

use std::ffi::{OsStr, OsString};

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

/// Baseline prefix: every `LC_*` locale category.
const BASELINE_PREFIX: &str = "LC_";

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
    /// to stderr by name only.
    pub fn for_launch<I, S>(provider_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut policy = Self::with_names(provider_names);
        if let Some(raw) = std::env::var_os(PASSTHROUGH_VARIABLE) {
            let (accepted, rejected) = parse_passthrough(&raw.to_string_lossy());
            for name in &rejected {
                eprintln!(
                    "orchestrator: ignoring {PASSTHROUGH_VARIABLE} entry {name:?}: not a forwardable variable name"
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
        let has_prefix = name.len() >= BASELINE_PREFIX.len()
            && name.is_char_boundary(BASELINE_PREFIX.len())
            && name[..BASELINE_PREFIX.len()].eq_ignore_ascii_case(BASELINE_PREFIX)
            && (self.case_insensitive || name.starts_with(BASELINE_PREFIX));
        has_prefix || BASELINE.iter().any(|candidate| same(candidate)) || {
            self.extra.iter().any(|candidate| same(candidate))
        }
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

/// A listable name: non-empty, no `=`, NUL, whitespace or control
/// characters, and not orchestrator-internal.
pub fn is_forwardable_name(name: &str) -> bool {
    !name.is_empty()
        && !is_internal(name)
        && name
            .chars()
            .all(|c| c != '=' && c != '\0' && !c.is_whitespace() && !c.is_control())
}

/// Splits a comma-separated list into `(accepted, rejected)` names. Blank
/// entries are skipped, not rejected, so a trailing comma is harmless.
pub fn parse_passthrough(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        if is_forwardable_name(entry) {
            accepted.push(entry.to_owned());
        } else {
            rejected.push(entry.to_owned());
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
        assert_eq!(rejected, ["FAKE=1", "AGENT_ORCH_DATA_DIR", "HAS SPACE"]);
    }

    #[test]
    fn non_ascii_name_boundaries_do_not_panic() {
        let policy = EnvPolicy::with_names_and_case(Vec::<String>::new(), true);
        assert!(!policy.allows(OsStr::new("L\u{e9}")));
        assert!(!policy.allows(OsStr::new("\u{e9}\u{e9}\u{e9}")));
    }
}
