//! GitHub access behind one trait (DPES-1105, phase 4c).
//!
//! [`GitHub`] is the ONLY place the daemon touches a remote forge. [`GhCli`] is the real
//! implementation: it runs the user's own `gh` as a child process and nothing else. It never
//! reads a token, `gh`'s config or its keyring, never builds a shell string, never prints the
//! child's output, and never passes `--admin` or `--auto`. Every test of the daemon uses
//! `FakeGitHub` (see `github_fake`) instead.
//!
//! UNVERIFIED against live gh output. Every JSON field name, `bucket` value, exit-code
//! assumption and error-message substring this file relies on lives in the private `wire`
//! module below and was written from the `gh` help text and memory, not from a live call:
//! no real `gh` call was allowed while this was built. The user checks them once on a
//! throwaway repository (plan-4 section 11.5) before any project is put in pull request
//! mode. Until then a surprise fails closed: unknown text maps to a refusing error, an
//! unknown check state never counts as passed, and a changed field name fails the parse.
//! The host-qualified `--repo github.com/owner/name` form and the `isCrossRepository` and
//! `headRepositoryOwner` (an object with `login`) fields must be checked live too.
//!
//! The error type holds only fixed text, so no child output (which may carry a path, a token
//! or a URL with credentials) can reach a log, a card or an error reply.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use orchestrator_core::landing::{CheckState, MergeMethod, PrState};
use serde_json::Value;

use crate::branch_prefix::is_safe_branch_name;
use crate::env_policy::EnvPolicy;
use crate::git::git;
use crate::process::{run_bounded, RunError};
use crate::Repository;

/// Variable that names a different `gh` program (tests and unusual installs).
pub const GH_PROGRAM_VARIABLE: &str = "AGENT_ORCH_GH_PROGRAM";

const VIEW_TIMEOUT: Duration = Duration::from_secs(20);
const MERGE_TIMEOUT: Duration = Duration::from_secs(60);
/// Longest stdout read from `gh`, in bytes. More is a failure, not a truncation.
const MAX_STDOUT_BYTES: usize = 1024 * 1024;

/// Most required checks one view carries. More is refused: dropping some could hide a red one.
pub const MAX_CHECKS: usize = 100;
/// Longest pull request title kept, in characters.
pub const MAX_TITLE_CHARS: usize = 200;
/// Longest check name kept, in characters.
pub const MAX_CHECK_NAME_CHARS: usize = 80;
const MAX_REPO_PART_CHARS: usize = 100;
const MAX_REF_CHARS: usize = 255;

/// Everything about the `gh` wire format the code guesses. UNVERIFIED against live gh output.
mod wire {
    /// `gh pr view --json` fields.
    pub const VIEW_FIELDS: &str =
        "number,state,headRefName,baseRefName,headRefOid,mergeable,title,mergeCommit,\
         isCrossRepository,headRepositoryOwner";
    /// `gh pr checks --required --json` fields. `bucket` is pass, fail, pending, skipping or cancel.
    pub const CHECK_FIELDS: &str = "name,bucket";
    /// `gh pr list --json` fields.
    pub const LIST_FIELDS: &str = "number,state,isCrossRepository,headRepositoryOwner";

    pub const STATE_OPEN: &str = "OPEN";
    pub const STATE_MERGED: &str = "MERGED";
    pub const STATE_CLOSED: &str = "CLOSED";

    pub const MERGEABLE_YES: &str = "MERGEABLE";
    pub const MERGEABLE_NO: &str = "CONFLICTING";

    pub const BUCKET_PASS: &str = "pass";
    pub const BUCKET_FAIL: &str = "fail";
    pub const BUCKET_CANCEL: &str = "cancel";
    pub const BUCKET_PENDING: &str = "pending";

    /// `gh pr checks --required` says this on stderr, with a non-zero exit, when none exist.
    pub const NO_REQUIRED_CHECKS: &str = "no required checks";

    /// Fixed child environment, added after the allowlist. None of it is a credential.
    pub const CHILD_ENV: [(&str, &str); 4] = [
        ("GH_PROMPT_DISABLED", "1"),
        ("GH_NO_UPDATE_NOTIFIER", "1"),
        ("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1"),
        ("NO_COLOR", "1"),
    ];

    /// Maps stderr (lower case) to an error. The first matching rule wins; the text is dropped.
    pub fn classify(stderr: &str) -> super::GitHubError {
        use super::GitHubError as E;
        let has = |needle: &str| stderr.contains(needle);
        if has("head branch") && has("modified") {
            E::HeadMoved
        } else if has("not logged in")
            || has("gh auth login")
            || has("no oauth token")
            || has("http 401")
        {
            E::NotSignedIn
        } else if has("could not resolve host") || has("error connecting") {
            E::Failed("network")
        } else if has("could not resolve to") || has("http 404") {
            E::NotFound
        } else if has("rate limit") {
            E::RateLimited
        } else if has("not mergeable")
            || has("merge conflict")
            || has("required status check")
            || has("review required")
            || has("protected branch")
            || has("http 405")
        {
            E::NotMergeable
        } else {
            E::Failed("gh_failed")
        }
    }
}

/// A repository on GitHub, named the way the `origin` remote names it. Never a URL.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoRef {
    owner: String,
    name: String,
}

impl RepoRef {
    /// `owner` and `name` of letters, digits, `.`, `_` and `-`, at most 100 characters each,
    /// not starting with `-` (an option) and not `.` or `..`.
    pub fn parse(owner: &str, name: &str) -> Option<Self> {
        let valid = |part: &str| {
            !part.is_empty()
                && part.len() <= MAX_REPO_PART_CHARS
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                && !part.starts_with('-')
                && part != "."
                && part != ".."
        };
        (valid(owner) && valid(name)).then(|| Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    /// Reads `https://github.com/o/n(.git)`, `ssh://git@github.com/o/n.git` and
    /// `git@github.com:o/n.git`. Userinfo (a token in a URL) is dropped, never kept or logged.
    /// Any other host, including a look-alike or a GitHub Enterprise host, gives `None`.
    pub fn from_remote_url(url: &str) -> Option<Self> {
        let url = url.trim();
        let rest = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("ssh://"));
        let (authority, path) = match rest {
            Some(rest) => rest.split_once('/')?,
            None if !url.contains("://") => url.split_once(':')?,
            None => return None,
        };
        // Userinfo may carry a token, so `_` is allowed there; the host is stricter.
        let plain = |text: &str, extra: &[char]| {
            text.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '.' | '@' | ':' | '-')
                    || extra.contains(&c)
            })
        };
        if !plain(authority, &['_']) {
            return None;
        }
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        if !plain(host, &[]) || !host.eq_ignore_ascii_case("github.com") {
            return None;
        }
        let path = path.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, name) = path.split_once('/')?;
        Self::parse(owner, name)
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for RepoRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.owner, self.name)
    }
}

/// One required check. A check that is not required is never listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckRun {
    /// Remote text: controls stripped, at most 80 characters.
    pub name: String,
    pub state: CheckState,
}

/// What the daemon reads of one pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestView {
    pub number: u64,
    pub state: PrState,
    pub head_branch: String,
    pub base_branch: String,
    /// 40 lower-case hex characters.
    pub head_sha: String,
    /// `None`: GitHub has not computed it, or the required checks could not be read.
    pub mergeable: Option<bool>,
    /// Remote text: controls stripped, at most 200 characters.
    pub title: String,
    /// The REQUIRED checks only, at most 100. Empty when there are none or they could not
    /// be read; the merge rule refuses both.
    pub checks: Vec<CheckRun>,
    /// Set when the pull request is merged.
    pub merge_sha: Option<String>,
    /// The head lives in the base repository itself, not in a fork. A client that cannot tell
    /// refuses the view (`cross_repo`), so a real view is always `true`; the merge grant
    /// checks it again for itself.
    pub same_repository: bool,
}

/// Why a GitHub call failed. Fixed text only: never the child's output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitHubError {
    /// No `gh` could be started.
    #[error("gh is not available")]
    NotAvailable,
    /// `gh` has no credential for github.com.
    #[error("gh is not signed in")]
    NotSignedIn,
    /// No such repository or pull request.
    #[error("repository or pull request not found")]
    NotFound,
    /// The pull request head is not the commit the caller bound.
    #[error("the pull request head moved")]
    HeadMoved,
    /// The host refused: conflicts, branch protection, a required review or check.
    #[error("the host refused the merge")]
    NotMergeable,
    #[error("rate limited")]
    RateLimited,
    #[error("timed out")]
    Timeout,
    /// A fixed code. Never text from the child.
    #[error("github call failed: {0}")]
    Failed(&'static str),
}

impl GitHubError {
    /// Whether a request may have left this machine and landed before the error: a merge
    /// that failed this way must be read back, never repeated blindly. False for an error
    /// that came before any request or that the host itself answered.
    pub const fn may_have_reached_host(self) -> bool {
        match self {
            Self::NotAvailable
            | Self::NotSignedIn
            | Self::NotFound
            | Self::HeadMoved
            | Self::NotMergeable
            | Self::RateLimited => false,
            Self::Timeout => true,
            Self::Failed(code) => {
                !matches!(code.as_bytes(), b"bad_argument" | b"spawn" | b"cross_repo")
            }
        }
    }
}

/// The remote forge. Blocking and object safe. The daemon holds an `Arc<dyn GitHub>`.
pub trait GitHub: Send + Sync {
    /// The repository `origin` points at, when it is on github.com.
    fn repo_of(&self, repository: &Repository) -> Result<RepoRef, GitHubError>;

    /// The open pull request whose head branch is `branch`, else the most recent one.
    fn find_by_branch(&self, repo: &RepoRef, branch: &str) -> Result<Option<u64>, GitHubError>;

    fn view(&self, repo: &RepoRef, number: u64) -> Result<PullRequestView, GitHubError>;

    /// Merges `number` ONLY if its head is still `head_sha`; the host enforces it. A
    /// `Timeout` or `Failed` error may follow a merge that landed (see
    /// [`GitHubError::may_have_reached_host`]): read the pull request back, never call
    /// this again for the same approval.
    fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        head_sha: &str,
        method: MergeMethod,
    ) -> Result<(), GitHubError>;
}

fn is_head_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The `--repo` value: host-qualified, so `gh` never falls back to its own default host.
fn repo_arg(repo: &RepoRef) -> String {
    format!("github.com/{repo}")
}

/// `gh pr view`.
pub fn view_args(repo: &RepoRef, number: u64) -> Vec<String> {
    vec![
        "pr".into(),
        "view".into(),
        number.to_string(),
        "--repo".into(),
        repo_arg(repo),
        "--json".into(),
        wire::VIEW_FIELDS.into(),
    ]
}

/// `gh pr checks`, required checks only.
pub fn checks_args(repo: &RepoRef, number: u64) -> Vec<String> {
    vec![
        "pr".into(),
        "checks".into(),
        number.to_string(),
        "--repo".into(),
        repo_arg(repo),
        "--required".into(),
        "--json".into(),
        wire::CHECK_FIELDS.into(),
    ]
}

/// `gh pr list` for one head branch. A branch that is not a safe branch name is refused.
pub fn find_args(repo: &RepoRef, branch: &str) -> Result<Vec<String>, GitHubError> {
    if !is_safe_branch_name(branch) {
        return Err(GitHubError::Failed("bad_argument"));
    }
    Ok(vec![
        "pr".into(),
        "list".into(),
        "--repo".into(),
        repo_arg(repo),
        "--head".into(),
        branch.to_owned(),
        "--state".into(),
        "all".into(),
        "--limit".into(),
        "20".into(),
        "--json".into(),
        wire::LIST_FIELDS.into(),
    ])
}

/// `gh pr merge`, bound to the head commit. Never `--admin`, never `--auto`. A head that is
/// not 40 lower-case hex characters is refused.
pub fn merge_args(
    repo: &RepoRef,
    number: u64,
    head_sha: &str,
    method: MergeMethod,
) -> Result<Vec<String>, GitHubError> {
    if !is_head_sha(head_sha) {
        return Err(GitHubError::Failed("bad_argument"));
    }
    let flag = match method {
        MergeMethod::Merge => "--merge",
        MergeMethod::Squash => "--squash",
        MergeMethod::Rebase => "--rebase",
    };
    Ok(vec![
        "pr".into(),
        "merge".into(),
        number.to_string(),
        "--repo".into(),
        repo_arg(repo),
        "--match-head-commit".into(),
        head_sha.to_owned(),
        flag.into(),
    ])
}

/// Plain text from the remote: controls stripped, cut to `max` characters.
fn plain(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

fn parse_json(stdout: &[u8]) -> Result<Value, GitHubError> {
    serde_json::from_slice(stdout).map_err(|_| GitHubError::Failed("bad_json"))
}

fn text_field<'a>(value: &'a Value, key: &str) -> Result<&'a str, GitHubError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(GitHubError::Failed("bad_json"))
}

fn ref_field(value: &Value, key: &str) -> Result<String, GitHubError> {
    let text = text_field(value, key)?;
    if text.is_empty() || text.chars().count() > MAX_REF_CHARS || text.chars().any(char::is_control)
    {
        return Err(GitHubError::Failed("bad_json"));
    }
    Ok(text.to_owned())
}

fn parse_pr_state(text: &str) -> Result<PrState, GitHubError> {
    match text {
        wire::STATE_OPEN => Ok(PrState::Open),
        wire::STATE_MERGED => Ok(PrState::Merged),
        wire::STATE_CLOSED => Ok(PrState::Closed),
        _ => Err(GitHubError::Failed("unknown_state")),
    }
}

/// Whether the pull request head lives in `repo` itself, not in a fork. Fails closed: a
/// missing `isCrossRepository` is an error; a missing head owner (a deleted fork) is a fork.
fn is_from_repo(item: &Value, repo: &RepoRef) -> Result<bool, GitHubError> {
    let cross = item
        .get("isCrossRepository")
        .and_then(Value::as_bool)
        .ok_or(GitHubError::Failed("bad_json"))?;
    let owner = item
        .get("headRepositoryOwner")
        .and_then(|owner| owner.get("login"))
        .and_then(Value::as_str);
    Ok(!cross && owner.is_some_and(|login| login.eq_ignore_ascii_case(repo.owner())))
}

/// Reads `gh pr view --json` output into a view with no checks. A different number than the
/// one asked for, an unknown state or a head that is not 40 hex characters is an error.
pub fn parse_view(
    stdout: &[u8],
    number: u64,
    repo: &RepoRef,
) -> Result<PullRequestView, GitHubError> {
    let value = parse_json(stdout)?;
    if value.get("number").and_then(Value::as_u64) != Some(number) {
        return Err(GitHubError::Failed("wrong_number"));
    }
    if !is_from_repo(&value, repo)? {
        return Err(GitHubError::Failed("cross_repo"));
    }
    let state = parse_pr_state(text_field(&value, "state")?)?;
    let head_sha = text_field(&value, "headRefOid")?;
    if !is_head_sha(head_sha) {
        return Err(GitHubError::Failed("bad_head_sha"));
    }
    let mergeable = match value.get("mergeable").and_then(Value::as_str) {
        Some(wire::MERGEABLE_YES) => Some(true),
        Some(wire::MERGEABLE_NO) => Some(false),
        _ => None,
    };
    let merge_sha = match value.get("mergeCommit").and_then(|c| c.get("oid")) {
        Some(Value::String(oid)) if is_head_sha(oid) => Some(oid.clone()),
        Some(Value::String(_)) => return Err(GitHubError::Failed("bad_head_sha")),
        _ => None,
    };
    Ok(PullRequestView {
        number,
        state,
        head_branch: ref_field(&value, "headRefName")?,
        base_branch: ref_field(&value, "baseRefName")?,
        head_sha: head_sha.to_owned(),
        mergeable,
        title: plain(text_field(&value, "title")?, MAX_TITLE_CHARS),
        checks: Vec::new(),
        merge_sha,
        // `is_from_repo` refused a fork above.
        same_repository: true,
    })
}

/// Reads `gh pr checks --required --json` output. More than [`MAX_CHECKS`] is an error
/// rather than a cut, because a cut could drop a red check. An unknown `bucket` is
/// `Unknown`; a skipped check is also `Unknown`, so it never counts as passed.
pub fn parse_checks(stdout: &[u8]) -> Result<Vec<CheckRun>, GitHubError> {
    let value = parse_json(stdout)?;
    let items = value.as_array().ok_or(GitHubError::Failed("bad_json"))?;
    if items.len() > MAX_CHECKS {
        return Err(GitHubError::Failed("too_many_checks"));
    }
    items
        .iter()
        .map(|item| {
            let state = match text_field(item, "bucket")? {
                wire::BUCKET_PASS => CheckState::Passed,
                wire::BUCKET_FAIL | wire::BUCKET_CANCEL => CheckState::Failed,
                wire::BUCKET_PENDING => CheckState::Pending,
                _ => CheckState::Unknown,
            };
            Ok(CheckRun {
                name: plain(text_field(item, "name")?, MAX_CHECK_NAME_CHARS),
                state,
            })
        })
        .collect()
}

/// Picks the open pull request with the highest number, else the highest number of any state.
/// A pull request from a fork is skipped, never picked.
pub fn parse_found(stdout: &[u8], repo: &RepoRef) -> Result<Option<u64>, GitHubError> {
    let value = parse_json(stdout)?;
    let items = value.as_array().ok_or(GitHubError::Failed("bad_json"))?;
    let mut best: Option<(bool, u64)> = None;
    for item in items {
        if !is_from_repo(item, repo)? {
            continue;
        }
        let number = item
            .get("number")
            .and_then(Value::as_u64)
            .ok_or(GitHubError::Failed("bad_json"))?;
        let open = parse_pr_state(text_field(item, "state")?)? == PrState::Open;
        if best.is_none_or(|current| (open, number) > current) {
            best = Some((open, number));
        }
    }
    Ok(best.map(|(_, number)| number))
}

/// The environment `gh` gets: the baseline allowlist and nothing else. `GH_TOKEN`,
/// `GITHUB_TOKEN` and `GH_HOST` are not forwarded, and neither is the user's
/// `AGENT_ORCH_ENV_PASSTHROUGH` list (it also reaches agents). `gh` reads its own stored
/// sign-in.
pub fn gh_env_policy() -> EnvPolicy {
    EnvPolicy::with_names(std::iter::empty::<&str>())
}

/// What the `gh` child gets: the allowed environment plus the fixed additions, and a working
/// directory. Pure, so a test can assert it.
struct ChildParts {
    env: Vec<(OsString, OsString)>,
    cwd: PathBuf,
}

/// The working directory is the user's home when it is an existing directory (`gh` reads its
/// own config from there anyway and a home is not world-writable), else the temp directory;
/// never the repository, so a `gh` config file in the checkout is never read.
fn child_command_parts<I>(policy: &EnvPolicy, parent: I, home: Option<OsString>) -> ChildParts
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut env = policy.filter(parent);
    env.extend(
        wire::CHILD_ENV
            .iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value))),
    );
    let cwd = home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    ChildParts { env, cwd }
}

struct Finished {
    success: bool,
    stdout: Vec<u8>,
    /// Lower case, for matching only. Never logged or returned.
    stderr: String,
}

fn gh_error(error: RunError) -> GitHubError {
    match error {
        RunError::Spawn(error) if error.kind() == std::io::ErrorKind::NotFound => {
            GitHubError::NotAvailable
        }
        RunError::Spawn(_) => GitHubError::Failed("spawn"),
        RunError::Timeout => GitHubError::Timeout,
        RunError::Failed(code) => GitHubError::Failed(code),
    }
}

/// The real implementation: the user's `gh` as a child process.
pub struct GhCli {
    program: PathBuf,
    env: EnvPolicy,
    /// One `gh` at a time, so a slow host cannot start a thread storm.
    gate: Mutex<()>,
    view_timeout: Duration,
    merge_timeout: Duration,
    max_stdout: usize,
}

impl GhCli {
    /// `gh` from `PATH`, or the program named by `AGENT_ORCH_GH_PROGRAM`.
    pub fn from_env() -> Self {
        let program = std::env::var_os(GH_PROGRAM_VARIABLE)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| OsString::from("gh"));
        Self::with_program(PathBuf::from(program))
    }

    pub fn with_program(program: PathBuf) -> Self {
        Self::with_limits(program, VIEW_TIMEOUT, MERGE_TIMEOUT, MAX_STDOUT_BYTES)
    }

    /// As [`GhCli::with_program`] with the time and output limits chosen by the caller (tests).
    pub fn with_limits(
        program: PathBuf,
        view_timeout: Duration,
        merge_timeout: Duration,
        max_stdout: usize,
    ) -> Self {
        Self {
            program,
            env: gh_env_policy(),
            gate: Mutex::new(()),
            view_timeout,
            merge_timeout,
            max_stdout,
        }
    }

    /// The only place that starts a process. Argv array, no shell, no stdin, allowlisted
    /// environment, bounded time and output; the child is killed and reaped on timeout.
    fn run_gh(&self, args: &[String], timeout: Duration) -> Result<Finished, GitHubError> {
        let _one_at_a_time = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let parts = child_command_parts(
            &self.env,
            std::env::vars_os(),
            std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
        );
        let mut command = Command::new(&self.program);
        command
            .env_clear()
            .envs(parts.env)
            .args(args)
            .current_dir(parts.cwd);
        let output = run_bounded(&mut command, timeout, self.max_stdout).map_err(gh_error)?;
        Ok(Finished {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).to_lowercase(),
        })
    }

    /// The required checks. `gh pr checks` exits non-zero for pending or failing checks
    /// while still printing JSON, so parseable output wins over the exit status.
    fn required_checks(&self, repo: &RepoRef, number: u64) -> Result<Vec<CheckRun>, GitHubError> {
        let done = self.run_gh(&checks_args(repo, number), self.view_timeout)?;
        if !done.stdout.is_empty() {
            return parse_checks(&done.stdout);
        }
        if done.stderr.contains(wire::NO_REQUIRED_CHECKS) {
            return Ok(Vec::new());
        }
        if done.success {
            return Err(GitHubError::Failed("bad_json"));
        }
        Err(wire::classify(&done.stderr))
    }
}

impl GitHub for GhCli {
    fn repo_of(&self, repository: &Repository) -> Result<RepoRef, GitHubError> {
        let url = git(
            "read origin url",
            repository.path(),
            ["remote", "get-url", "origin"],
        )
        .map_err(|_| GitHubError::Failed("no_remote"))?;
        RepoRef::from_remote_url(&url).ok_or(GitHubError::Failed("not_github"))
    }

    fn find_by_branch(&self, repo: &RepoRef, branch: &str) -> Result<Option<u64>, GitHubError> {
        let done = self.run_gh(&find_args(repo, branch)?, self.view_timeout)?;
        if !done.success {
            return Err(wire::classify(&done.stderr));
        }
        parse_found(&done.stdout, repo)
    }

    fn view(&self, repo: &RepoRef, number: u64) -> Result<PullRequestView, GitHubError> {
        let done = self.run_gh(&view_args(repo, number), self.view_timeout)?;
        if !done.success {
            return Err(wire::classify(&done.stderr));
        }
        let mut view = parse_view(&done.stdout, number, repo)?;
        match self.required_checks(repo, number) {
            Ok(checks) => view.checks = checks,
            // Could not learn the required set: say so with no checks and no mergeable
            // answer, which the merge rule refuses. Never assume there are none.
            Err(GitHubError::Failed(_)) => {
                view.mergeable = None;
                view.checks = Vec::new();
            }
            Err(other) => return Err(other),
        }
        Ok(view)
    }

    fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        head_sha: &str,
        method: MergeMethod,
    ) -> Result<(), GitHubError> {
        let args = merge_args(repo, number, head_sha, method)?;
        let done = self.run_gh(&args, self.merge_timeout)?;
        if done.success {
            Ok(())
        } else {
            Err(wire::classify(&done.stderr))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn repo() -> RepoRef {
        RepoRef::parse("octo", "demo").unwrap()
    }

    fn view_json(state: &str, mergeable: &str, merge_commit: &str) -> String {
        format!(
            r#"{{"number":7,"state":"{state}","headRefName":"moa/fix","baseRefName":"main",
            "headRefOid":"{SHA}","mergeable":"{mergeable}","title":"Fix it",
            "mergeCommit":{merge_commit},
            "isCrossRepository":false,"headRepositoryOwner":{{"login":"octo"}}}}"#
        )
    }

    #[test]
    fn github_repo_ref_reads_the_forms_origin_urls_take() {
        let expected = Some(repo());
        for url in [
            "https://github.com/octo/demo",
            "https://github.com/octo/demo.git",
            "https://github.com/octo/demo/",
            "https://x-access-token:ghp_secret@github.com/octo/demo.git",
            "https://user@github.com/octo/demo.git",
            "ssh://git@github.com/octo/demo.git",
            "git@github.com:octo/demo.git",
            "github.com:octo/demo",
            "https://GitHub.com/octo/demo",
        ] {
            assert_eq!(RepoRef::from_remote_url(url), expected, "{url}");
        }
    }

    #[test]
    fn github_repo_ref_refuses_other_hosts_and_odd_paths() {
        for url in [
            "https://gitlab.com/octo/demo",
            "https://github.com.evil.example/octo/demo",
            "https://evilgithub.com/octo/demo",
            "https://github.com@evil.example/octo/demo",
            "http://github.com/octo/demo",
            "git://github.com/octo/demo",
            "https://github.com/octo",
            "https://github.com/octo/demo/extra",
            "https://github.com/-flag/demo",
            "https://github.com/octo/..",
            "https://github.com/oc to/demo",
            "/home/user/demo",
            "C:\\work\\demo",
            "",
        ] {
            assert_eq!(RepoRef::from_remote_url(url), None, "{url}");
        }
    }

    #[test]
    fn github_repo_ref_display_never_holds_userinfo() {
        let parsed =
            RepoRef::from_remote_url("https://tok:secret@github.com/octo/demo.git").unwrap();
        assert_eq!(parsed.to_string(), "octo/demo");
        assert!(!format!("{parsed:?}").contains("secret"));
    }

    #[test]
    fn github_merge_argv_binds_the_head_and_never_admin_or_auto() {
        let args = merge_args(&repo(), 7, SHA, MergeMethod::Squash).unwrap();
        assert_eq!(
            args,
            [
                "pr",
                "merge",
                "7",
                "--repo",
                "github.com/octo/demo",
                "--match-head-commit",
                SHA,
                "--squash"
            ]
        );
        for method in [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase] {
            let args = merge_args(&repo(), 7, SHA, method).unwrap();
            assert!(!args.iter().any(|a| a == "--admin" || a == "--auto"));
            assert!(args.iter().any(|a| a == "--match-head-commit"));
        }
        assert!(merge_args(&repo(), 7, SHA, MergeMethod::Merge)
            .unwrap()
            .contains(&"--merge".to_owned()));
        assert!(merge_args(&repo(), 7, SHA, MergeMethod::Rebase)
            .unwrap()
            .contains(&"--rebase".to_owned()));
    }

    #[test]
    fn github_merge_argv_refuses_a_head_that_is_not_40_lower_hex() {
        let long = format!("{SHA}0");
        let upper = SHA.to_uppercase();
        for bad in [
            "",
            &SHA[..39],
            long.as_str(),
            upper.as_str(),
            "--admin",
            "-0123456789abcdef0123456789abcdef0123456",
        ] {
            assert_eq!(
                merge_args(&repo(), 7, bad, MergeMethod::Squash),
                Err(GitHubError::Failed("bad_argument")),
                "{bad}"
            );
        }
    }

    #[test]
    fn github_find_argv_refuses_an_option_like_or_unsafe_branch() {
        for bad in ["", "-x", "--head", "a b", "a;b", "a..b", "x/-y", "HEAD"] {
            assert_eq!(
                find_args(&repo(), bad),
                Err(GitHubError::Failed("bad_argument")),
                "{bad}"
            );
        }
        let args = find_args(&repo(), "moa/fix-1").unwrap();
        assert!(args
            .windows(2)
            .any(|w| w == ["--head", "moa/fix-1"].as_slice()));
    }

    #[test]
    fn github_argv_values_that_are_not_listed_flags_never_start_with_a_dash() {
        let flags = [
            "--repo",
            "--json",
            "--required",
            "--head",
            "--state",
            "--limit",
            "--match-head-commit",
            "--squash",
        ];
        let mut all = vec![view_args(&repo(), 7), checks_args(&repo(), 7)];
        all.push(find_args(&repo(), "moa/x").unwrap());
        all.push(merge_args(&repo(), 7, SHA, MergeMethod::Squash).unwrap());
        for args in all {
            for arg in args.iter().filter(|a| a.starts_with('-')) {
                assert!(flags.contains(&arg.as_str()), "{arg}");
            }
        }
    }

    #[test]
    fn github_view_and_checks_argv_name_the_wire_fields() {
        let view = view_args(&repo(), 7);
        assert_eq!(
            view[..5],
            ["pr", "view", "7", "--repo", "github.com/octo/demo"]
        );
        assert!(view.last().unwrap().contains("headRefOid"));
        assert!(view.last().unwrap().contains("isCrossRepository"));
        assert!(view.last().unwrap().contains("headRepositoryOwner"));
        let checks = checks_args(&repo(), 7);
        assert!(checks.contains(&"--required".to_owned()));
        assert_eq!(checks.last().unwrap(), "name,bucket");
        let found = find_args(&repo(), "moa/x").unwrap();
        assert!(found.last().unwrap().contains("isCrossRepository"));
        assert!(found.last().unwrap().contains("headRepositoryOwner"));
        for args in [view, checks, found] {
            assert!(args
                .windows(2)
                .any(|w| w == ["--repo", "github.com/octo/demo"].as_slice()));
        }
    }

    #[test]
    fn github_parse_view_reads_an_open_pull_request() {
        let json = view_json("OPEN", "MERGEABLE", "null");
        let view = parse_view(json.as_bytes(), 7, &repo()).unwrap();
        assert_eq!(view.state, PrState::Open);
        assert_eq!(view.head_sha, SHA);
        assert_eq!(view.mergeable, Some(true));
        assert_eq!(view.head_branch, "moa/fix");
        assert_eq!(view.base_branch, "main");
        assert_eq!(view.merge_sha, None);
        assert!(view.checks.is_empty());
    }

    #[test]
    fn github_parse_view_maps_states_mergeable_and_merge_commit() {
        let json = view_json("CLOSED", "CONFLICTING", "null");
        let view = parse_view(json.as_bytes(), 7, &repo()).unwrap();
        assert_eq!((view.state, view.mergeable), (PrState::Closed, Some(false)));
        let json = view_json("OPEN", "UNKNOWN", "null");
        assert_eq!(
            parse_view(json.as_bytes(), 7, &repo()).unwrap().mergeable,
            None
        );
        let merged = format!(r#"{{"oid":"{SHA}"}}"#);
        let json = view_json("MERGED", "UNKNOWN", &merged);
        let view = parse_view(json.as_bytes(), 7, &repo()).unwrap();
        assert_eq!(view.state, PrState::Merged);
        assert_eq!(view.merge_sha.as_deref(), Some(SHA));
    }

    #[test]
    fn github_parse_view_refuses_what_it_cannot_place() {
        let unknown = view_json("DRAFTISH", "MERGEABLE", "null");
        assert_eq!(
            parse_view(unknown.as_bytes(), 7, &repo()),
            Err(GitHubError::Failed("unknown_state"))
        );
        let ok = view_json("OPEN", "MERGEABLE", "null");
        assert_eq!(
            parse_view(ok.as_bytes(), 8, &repo()),
            Err(GitHubError::Failed("wrong_number"))
        );
        let short = ok.replace(SHA, "abc");
        assert_eq!(
            parse_view(short.as_bytes(), 7, &repo()),
            Err(GitHubError::Failed("bad_head_sha"))
        );
        let upper = ok.replace(SHA, &SHA.to_uppercase());
        assert_eq!(
            parse_view(upper.as_bytes(), 7, &repo()),
            Err(GitHubError::Failed("bad_head_sha"))
        );
        for junk in ["", "[]", "{", "null", r#"{"number":7}"#] {
            assert!(parse_view(junk.as_bytes(), 7, &repo()).is_err(), "{junk}");
        }
    }

    #[test]
    fn github_parse_view_strips_controls_and_caps_the_title() {
        let long = "t".repeat(500);
        let json = view_json("OPEN", "MERGEABLE", "null")
            .replace("Fix it", &format!("a\\u001b[31mb\\n{long}"));
        let view = parse_view(json.as_bytes(), 7, &repo()).unwrap();
        assert_eq!(view.title.chars().count(), MAX_TITLE_CHARS);
        assert!(view.title.starts_with("a[31mb"));
        assert!(!view.title.chars().any(char::is_control));
    }

    #[test]
    fn github_parse_checks_maps_buckets_and_never_passes_the_unknown() {
        let json = r#"[{"name":"a","bucket":"pass"},{"name":"b","bucket":"fail"},
            {"name":"c","bucket":"cancel"},{"name":"d","bucket":"pending"},
            {"name":"e","bucket":"skipping"},{"name":"f","bucket":"novel"}]"#;
        let states: Vec<_> = parse_checks(json.as_bytes())
            .unwrap()
            .into_iter()
            .map(|check| check.state)
            .collect();
        assert_eq!(
            states,
            [
                CheckState::Passed,
                CheckState::Failed,
                CheckState::Failed,
                CheckState::Pending,
                CheckState::Unknown,
                CheckState::Unknown
            ]
        );
    }

    #[test]
    fn github_parse_checks_refuses_too_many_and_garbage() {
        let one = r#"{"name":"a","bucket":"pass"}"#;
        let ok = format!("[{}]", vec![one; MAX_CHECKS].join(","));
        assert_eq!(parse_checks(ok.as_bytes()).unwrap().len(), MAX_CHECKS);
        let over = format!("[{}]", vec![one; MAX_CHECKS + 1].join(","));
        assert_eq!(
            parse_checks(over.as_bytes()),
            Err(GitHubError::Failed("too_many_checks"))
        );
        assert_eq!(parse_checks(b"[]").unwrap(), Vec::new());
        for junk in ["", "{}", r#"[{"name":"a"}]"#, r#"[{"bucket":"pass"}]"#] {
            assert!(parse_checks(junk.as_bytes()).is_err(), "{junk}");
        }
    }

    #[test]
    fn github_parse_checks_caps_and_cleans_names() {
        let name = format!("x\\u0007{}", "n".repeat(300));
        let json = format!(r#"[{{"name":"{name}","bucket":"pass"}}]"#);
        let checks = parse_checks(json.as_bytes()).unwrap();
        assert_eq!(checks[0].name.chars().count(), MAX_CHECK_NAME_CHARS);
        assert!(!checks[0].name.chars().any(char::is_control));
    }

    fn list_item(number: u64, state: &str) -> String {
        format!(
            r#"{{"number":{number},"state":"{state}","isCrossRepository":false,
            "headRepositoryOwner":{{"login":"octo"}}}}"#
        )
    }

    fn fork_item(number: u64, state: &str) -> String {
        format!(
            r#"{{"number":{number},"state":"{state}","isCrossRepository":true,
            "headRepositoryOwner":{{"login":"stranger"}}}}"#
        )
    }

    fn list(items: &[String]) -> String {
        format!("[{}]", items.join(","))
    }

    #[test]
    fn github_parse_found_prefers_open_then_highest_number() {
        let json = list(&[
            list_item(3, "MERGED"),
            list_item(9, "CLOSED"),
            list_item(5, "OPEN"),
            list_item(4, "OPEN"),
        ]);
        assert_eq!(parse_found(json.as_bytes(), &repo()), Ok(Some(5)));
        let closed = list(&[list_item(3, "MERGED"), list_item(9, "CLOSED")]);
        assert_eq!(parse_found(closed.as_bytes(), &repo()), Ok(Some(9)));
        assert_eq!(parse_found(b"[]", &repo()), Ok(None));
        assert!(parse_found(b"{}", &repo()).is_err());
        let odd = list(&[list_item(1, "ODD")]);
        assert!(parse_found(odd.as_bytes(), &repo()).is_err());
        // A list entry without the fork fields fails closed.
        assert!(parse_found(br#"[{"number":1,"state":"OPEN"}]"#, &repo()).is_err());
    }

    #[test]
    fn github_a_pull_request_from_a_fork_is_never_found_or_viewed() {
        // The fork's pull request has the higher number and is open: it is still skipped.
        let json = list(&[
            fork_item(12, "OPEN"),
            list_item(5, "OPEN"),
            fork_item(13, "OPEN"),
        ]);
        assert_eq!(parse_found(json.as_bytes(), &repo()), Ok(Some(5)));
        let only_forks = list(&[fork_item(12, "OPEN")]);
        assert_eq!(parse_found(only_forks.as_bytes(), &repo()), Ok(None));
        // Same owner name but flagged cross-repository, and a different owner not flagged.
        let flagged = list(&[list_item(5, "OPEN")]).replace("false", "true");
        assert_eq!(parse_found(flagged.as_bytes(), &repo()), Ok(None));
        let other_owner = list(&[list_item(5, "OPEN")]).replace("octo", "stranger");
        assert_eq!(parse_found(other_owner.as_bytes(), &repo()), Ok(None));
        // A deleted fork has no head owner.
        let deleted = list(&[list_item(5, "OPEN")]).replace(
            r#""headRepositoryOwner":{"login":"octo"}"#,
            r#""headRepositoryOwner":null"#,
        );
        assert_eq!(parse_found(deleted.as_bytes(), &repo()), Ok(None));

        let view = view_json("OPEN", "MERGEABLE", "null");
        let flagged = view.replace("false", "true");
        let other_owner = view.replace(r#""login":"octo""#, r#""login":"stranger""#);
        for bad in [flagged, other_owner] {
            assert_eq!(
                parse_view(bad.as_bytes(), 7, &repo()),
                Err(GitHubError::Failed("cross_repo"))
            );
        }
        assert!(!GitHubError::Failed("cross_repo").may_have_reached_host());
    }

    #[test]
    fn github_stderr_maps_to_fixed_errors_in_priority_order() {
        use GitHubError as E;
        for (text, expected) in [
            (
                "pull request #7: head branch was modified. review and try again. (http 405)",
                E::HeadMoved,
            ),
            (
                "you are not logged in to any github hosts. run gh auth login",
                E::NotSignedIn,
            ),
            ("http 401: bad credentials", E::NotSignedIn),
            ("could not resolve host: github.com", E::Failed("network")),
            (
                "graphql: could not resolve to a repository with the name 'o/n'",
                E::NotFound,
            ),
            ("http 404: not found", E::NotFound),
            ("api rate limit exceeded", E::RateLimited),
            (
                "pull request is not mergeable: the merge commit cannot be cleanly created",
                E::NotMergeable,
            ),
            (
                "required status check \"build\" is expected",
                E::NotMergeable,
            ),
            ("http 405: method not allowed", E::NotMergeable),
            ("something new and strange", E::Failed("gh_failed")),
            ("", E::Failed("gh_failed")),
        ] {
            assert_eq!(wire::classify(text), expected, "{text}");
        }
    }

    #[test]
    fn github_errors_say_whether_a_request_may_have_landed() {
        assert!(GitHubError::Timeout.may_have_reached_host());
        assert!(GitHubError::Failed("gh_failed").may_have_reached_host());
        assert!(GitHubError::Failed("network").may_have_reached_host());
        for before in [
            GitHubError::NotAvailable,
            GitHubError::NotSignedIn,
            GitHubError::NotFound,
            GitHubError::HeadMoved,
            GitHubError::NotMergeable,
            GitHubError::RateLimited,
            GitHubError::Failed("bad_argument"),
            GitHubError::Failed("spawn"),
        ] {
            assert!(!before.may_have_reached_host(), "{before:?}");
        }
    }

    #[test]
    fn github_error_text_is_fixed() {
        assert_eq!(
            GitHubError::Failed("gh_failed").to_string(),
            "github call failed: gh_failed"
        );
        assert_eq!(GitHubError::NotSignedIn.to_string(), "gh is not signed in");
    }

    #[test]
    fn github_env_policy_allows_no_token_or_host_name() {
        let policy = gh_env_policy();
        for name in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GH_HOST",
            "AGENT_ORCH_ENV_PASSTHROUGH",
            "AGENT_ORCH_GH_PROGRAM",
            "ANTHROPIC_API_KEY",
        ] {
            assert!(!policy.allows(OsStr::new(name)), "{name}");
        }
        for name in ["PATH", "HOME", "XDG_CONFIG_HOME", "HTTPS_PROXY"] {
            assert!(policy.allows(OsStr::new(name)), "{name}");
        }
        for (name, _) in wire::CHILD_ENV {
            assert!(!name.contains("TOKEN"));
        }
    }

    #[test]
    fn github_child_command_parts_carry_no_token_or_host_and_a_safe_directory() {
        let os = |text: &str| OsString::from(text);
        let parent = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/someone"),
            ("GH_TOKEN", "secret"),
            ("GITHUB_TOKEN", "secret"),
            ("GH_HOST", "example.test"),
            ("AGENT_ORCH_ENV_PASSTHROUGH", "GH_TOKEN"),
            ("ANTHROPIC_API_KEY", "secret"),
        ]
        .map(|(name, value)| (os(name), os(value)));
        let home = std::env::temp_dir();
        let parts =
            child_command_parts(&gh_env_policy(), parent.clone(), Some(home.clone().into()));
        let names: Vec<&OsString> = parts.env.iter().map(|(name, _)| name).collect();
        for kept in ["PATH", "HOME", "GH_PROMPT_DISABLED", "NO_COLOR"] {
            assert!(names.contains(&&os(kept)), "{kept}");
        }
        for dropped in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_HOST",
            "AGENT_ORCH_ENV_PASSTHROUGH",
            "ANTHROPIC_API_KEY",
        ] {
            assert!(!names.contains(&&os(dropped)), "{dropped}");
        }
        assert_eq!(parts.cwd, home);
        // A home that does not exist or is not absolute falls back to the temp directory.
        for bad in [
            Some(os("/nonexistent-dir/home")),
            Some(os("relative")),
            None,
        ] {
            let parts = child_command_parts(&gh_env_policy(), parent.clone(), bad);
            assert_eq!(parts.cwd, std::env::temp_dir());
        }
    }

    #[test]
    fn github_repo_ref_refuses_a_backslash_or_odd_character_in_the_authority() {
        for url in [
            "https://evil\\@github.com/octo/demo",
            "https://github.com\\/octo/demo",
            "https://user%40x@github.com/octo/demo",
            "https://us er@github.com/octo/demo",
            "https://user@git\u{0}hub.com/octo/demo",
            "ssh://git@github.com\\/octo/demo",
            "https://git hub.com/octo/demo",
            "https://u[1]@github.com/octo/demo",
        ] {
            assert_eq!(RepoRef::from_remote_url(url), None, "{url}");
        }
    }

    #[test]
    fn github_a_missing_program_is_not_available_and_runs_nothing() {
        let cli = GhCli::with_program(PathBuf::from("/nonexistent-dir/definitely-not-gh"));
        assert_eq!(cli.view(&repo(), 7), Err(GitHubError::NotAvailable));
        assert_eq!(
            cli.merge(&repo(), 7, SHA, MergeMethod::Squash),
            Err(GitHubError::NotAvailable)
        );
        assert_eq!(
            cli.find_by_branch(&repo(), "moa/x"),
            Err(GitHubError::NotAvailable)
        );
        // A bad argument is refused before any process is considered.
        assert_eq!(
            cli.merge(&repo(), 7, "bad", MergeMethod::Squash),
            Err(GitHubError::Failed("bad_argument"))
        );
    }

    /// A stub `gh`: a shell script in its own temp directory, removed on drop. Unix only.
    #[cfg(unix)]
    struct StubGh {
        dir: PathBuf,
        program: PathBuf,
    }

    #[cfg(unix)]
    impl StubGh {
        fn new(label: &str, script: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let dir = std::env::temp_dir().join(format!("gh-stub-{}-{label}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let program = dir.join("gh");
            std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self { dir, program }
        }
    }

    #[cfg(unix)]
    impl Drop for StubGh {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn github_a_gh_that_times_out_is_killed_and_reports_timeout() {
        use std::time::Instant;

        // `exec` so the kill lands on the sleeping process itself, not on a shell around it.
        let stub = StubGh::new("timeout", "exec sleep 30");
        let cli = GhCli::with_limits(
            stub.program.clone(),
            Duration::from_millis(200),
            Duration::from_millis(200),
            MAX_STDOUT_BYTES,
        );
        let started = Instant::now();
        assert_eq!(cli.view(&repo(), 7), Err(GitHubError::Timeout));
        assert_eq!(
            cli.merge(&repo(), 7, SHA, MergeMethod::Squash),
            Err(GitHubError::Timeout)
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn github_a_gh_that_prints_too_much_is_capped() {
        let stub = StubGh::new("flood", "exec head -c 100000 /dev/zero");
        let cli = GhCli::with_limits(
            stub.program.clone(),
            Duration::from_secs(10),
            Duration::from_secs(10),
            1024,
        );
        assert_eq!(
            cli.find_by_branch(&repo(), "moa/x"),
            Err(GitHubError::Failed("output_too_large"))
        );
    }
}
