//! GitHub Copilot CLI adapter (DPES-1105): one `copilot -p` process per run,
//! translated from the CLI's JSONL stream into the canonical event vocabulary.
//!
//! Verified against Copilot CLI [`PINNED_VERSION`]. Every run:
//! - passes its prompt as one argument (`--prompt=<text>`) and never through a
//!   shell;
//! - leaves the CLI's command sandbox off (no `--sandbox`), as the CLI does by
//!   default. The adapter neither probes for that sandbox nor gates on it;
//! - starts from the shared environment baseline, with no provider variable,
//!   so no GitHub token reaches the child;
//! - passes `--max-ai-credits` (a soft cap, see [`AI_CREDIT_CAP`]);
//! - passes a widening flag only at the Full level, which the user chose, and
//!   never passes a model or effort on a normal run (see
//!   [`CopilotAdapter::sign_in_check_args`] for the one exception).
//!
//! The worktree is source isolation, not a sandbox. Built-in file edits are not
//! OS-sandboxed (Copilot's own sandbox help says so), so the write rule is
//! enforced by the CLI's permission layer on a best-effort basis.
//!
//! Task policy and result interpretation stay in the engine. A run completes
//! only on the provider's own `result`, and sign-in facts are kept by
//! [`crate::copilot_sign_in`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use orchestrator_core::permission::PermissionLevel;
use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent, SupervisorError};
use uuid::Uuid;

use crate::contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result,
};
use crate::copilot_parser::CopilotParser;
use crate::copilot_sign_in::{record_for_exit, SignInRecord};
use crate::copilot_usage::read_usage;
use crate::routing::RoutableAdapter;

/// The wire id of this adapter family, as the daemon and the catalog name it.
pub const COPILOT_AGENT: &str = "github-copilot";
/// The one CLI version this adapter has been verified against.
pub const PINNED_VERSION: &str = "1.0.109";
/// The cheapest documented model, used only by the explicit sign-in check.
pub const SIGN_IN_MODEL: &str = "claude-haiku-4.5";
const SIGN_IN_PROMPT: &str = "Reply with the single word OK.";
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
/// Every run's `--max-ai-credits`, in credits. 30 is the minimum the CLI accepts.
/// The cap is soft: one response can exceed it. This is a working default, not a
/// value the user chose (DPES-1105 O2).
pub const AI_CREDIT_CAP: &str = "30";

/// The version on the first line of `copilot --version`
/// (`GitHub Copilot CLI 1.0.109.`), without the trailing period.
pub fn parse_copilot_version(first_line: &str) -> Option<String> {
    let token = first_line
        .split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))?
        .trim_end_matches('.');
    let well_formed = token.len() <= 32
        && token.starts_with(|c: char| c.is_ascii_digit())
        && token.chars().all(|c| c.is_ascii_digit() || c == '.');
    well_formed.then(|| token.to_owned())
}

/// Runs `copilot --version` under the supervisor and returns its version.
/// Bounded by `limit`; a probe that is still running is cancelled.
pub(crate) async fn probe_version(
    program: &Path,
    limit: Duration,
) -> std::result::Result<String, String> {
    let mut supervisor = ChildSupervisor::launch(&LaunchRequest {
        program: program.to_path_buf(),
        args: vec!["--version".to_owned()],
        working_dir: std::env::temp_dir(),
        env_names: Vec::new(),
    })
    .map_err(|error| error.to_string())?;
    let mut first_line: Option<String> = None;
    let finished = tokio::time::timeout(limit, async {
        loop {
            match supervisor.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    first_line.get_or_insert(line);
                }
                Some(SupervisionEvent::StderrLine(_)) => {}
                Some(SupervisionEvent::Exited { code, .. }) => return code,
                None => return None,
            }
        }
    })
    .await;
    match finished {
        Ok(Some(0)) => first_line
            .as_deref()
            .and_then(parse_copilot_version)
            .ok_or_else(|| "copilot --version printed no version".to_owned()),
        Ok(code) => Err(format!(
            "copilot --version exited with code {}",
            code.map_or_else(|| "none".to_owned(), |code| code.to_string())
        )),
        Err(_) => {
            supervisor.cancel().await;
            Err("copilot --version timed out".to_owned())
        }
    }
}

/// One run's private directory: an empty working directory for runs that must
/// not see a repository, and the usage file the CLI writes at session end. The
/// whole directory is removed when the run's handle drops.
#[derive(Debug)]
pub struct CopilotRun {
    root: PathBuf,
}

impl CopilotRun {
    /// Creates a fresh run directory under `parent`.
    pub fn create(parent: &Path) -> std::io::Result<Self> {
        let root = parent.join(format!("run-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(root.join("cwd"))?;
        Ok(Self { root })
    }

    /// The empty working directory. It is never a user's repository.
    pub fn cwd(&self) -> PathBuf {
        self.root.join("cwd")
    }

    /// Where the CLI writes its usage JSON. Outside the working directory.
    pub fn usage_file(&self) -> PathBuf {
        self.root.join("usage.json")
    }
}

impl Drop for CopilotRun {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The Copilot CLI adapter.
#[derive(Debug, Clone, Default)]
pub struct CopilotAdapter {
    /// Executable to launch. `None` means `copilot`, resolved against PATH.
    program: Option<PathBuf>,
}

impl CopilotAdapter {
    /// Launch `copilot`, resolved against PATH.
    pub fn new() -> Self {
        Self { program: None }
    }

    /// Use an explicit executable path.
    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    fn program(&self) -> PathBuf {
        self.program
            .clone()
            .unwrap_or_else(|| PathBuf::from("copilot"))
    }

    /// Locate the executable: an explicit path is checked as-is; a bare name
    /// is resolved against PATH (`.exe`, then `.cmd`, on Windows).
    pub fn locate(&self) -> Option<PathBuf> {
        crate::locate::locate_program(&self.program())
    }

    /// Arguments for a text-only conversation turn at `level`, or `None` when
    /// this adapter has no mapping for it (Extended, in PR A). The run reads
    /// only its own empty directory, `scratch`.
    ///
    /// Restricted can neither write, run a command, fetch a URL nor use memory.
    /// Auto can write under `scratch`, and a shell request goes to the judge
    /// (`--assisted-approval`). Git push is denied by rule. Full passes
    /// `--allow-all`, the user's per-turn choice.
    pub fn conversation_args(
        level: PermissionLevel,
        prompt: &str,
        usage_file: &Path,
        scratch: &Path,
    ) -> Option<Vec<String>> {
        let flags = match level {
            PermissionLevel::Restricted => restricted_flags(),
            PermissionLevel::Auto => auto_flags(scratch),
            PermissionLevel::Full => vec!["--allow-all".to_owned()],
            PermissionLevel::Extended => return None,
        };
        let mut args = base_args(prompt, usage_file, scratch, level);
        args.extend(flags);
        Some(args)
    }

    /// Arguments for a task run in `worktree`. It reads what the CLI allows and
    /// writes only under the worktree. It has no shell, URL or memory tool.
    pub fn task_args(objective: &str, usage_file: &Path, worktree: &Path) -> Vec<String> {
        let mut args = base_args(objective, usage_file, worktree, PermissionLevel::Restricted);
        let write_rule = format!("write({}/**)", path_text(worktree));
        args.extend(pairs(&[
            ("--allow-tool", "read"),
            ("--allow-tool", write_rule.as_str()),
        ]));
        args.extend(pairs(&[
            ("--deny-tool", "shell"),
            ("--deny-tool", "url"),
            ("--deny-tool", "memory"),
        ]));
        args
    }

    /// The Restricted conversation arguments plus the one model this adapter
    /// ever names: the explicit sign-in check. Normal runs pass no `--model`.
    pub fn sign_in_check_args(usage_file: &Path, scratch: &Path) -> Vec<String> {
        let mut args = base_args(
            SIGN_IN_PROMPT,
            usage_file,
            scratch,
            PermissionLevel::Restricted,
        );
        args.extend(restricted_flags());
        args.extend(["--model".to_owned(), SIGN_IN_MODEL.to_owned()]);
        args
    }

    /// Launches `request` through the supervisor. `run` (when given) lives as
    /// long as the handle.
    pub async fn spawn_run(
        &self,
        request: AgentLaunchRequest,
        run: Option<CopilotRun>,
    ) -> Result<Box<dyn AgentHandle>> {
        Ok(Box::new(spawn_copilot_handle(request, run)?))
    }

    /// Runs the one explicit sign-in check: one call on [`SIGN_IN_MODEL`] in a
    /// fresh run directory under `parent`, bounded by `limit`. The result says
    /// what the call established, plus the usage it reported. A check that
    /// cannot run returns an unrecorded [`SignInRecord::refused`].
    pub async fn sign_in_check(&self, parent: &Path, limit: Duration) -> SignInRecord {
        let Some(program) = self.locate() else {
            return SignInRecord::refused(format!("the {COPILOT_AGENT} executable was not found"));
        };
        if let Err(error) = self.detect().await {
            return SignInRecord::refused(error.to_string());
        }
        let run = match CopilotRun::create(parent) {
            Ok(run) => run,
            Err(error) => {
                return SignInRecord::refused(format!(
                    "the check's run directory could not be created: {error}"
                ))
            }
        };
        let usage_file = run.usage_file();
        let working_dir = run.cwd();
        let request = AgentLaunchRequest {
            program,
            args: Self::sign_in_check_args(&usage_file, &working_dir),
            working_dir,
        };
        let mut handle = match spawn_copilot_handle(request, Some(run)) {
            Ok(handle) => handle,
            Err(error) => return SignInRecord::refused(error.to_string()),
        };
        let now = crate::copilot_sign_in::now_ms();
        let waited = tokio::time::timeout(limit, handle.wait()).await;
        let warning = handle.warning();
        let mut record = match waited {
            Ok(Ok(exit)) => record_for_exit(&exit, warning, now),
            Ok(Err(error)) => SignInRecord::failed(error.to_string()),
            Err(_) => {
                let _ = handle.cancel().await;
                SignInRecord::failed("the sign-in check timed out".to_owned())
            }
        };
        let usage = read_usage(&usage_file);
        record.premium_requests = usage.as_ref().and_then(|usage| usage.premium_requests);
        record.ai_credits = usage.as_ref().and_then(|usage| usage.ai_credits);
        record
    }
}

/// Launches `request` through the supervisor. `run` (when given) lives as long
/// as the handle.
fn spawn_copilot_handle(
    request: AgentLaunchRequest,
    run: Option<CopilotRun>,
) -> Result<CopilotHandle> {
    let sup = ChildSupervisor::launch(&LaunchRequest {
        program: request.program,
        args: request.args,
        working_dir: request.working_dir,
        // The shared baseline only: Copilot takes no provider variable.
        env_names: Vec::new(),
    })?;
    Ok(CopilotHandle {
        sup,
        parser: CopilotParser::new(),
        cancelled: false,
        code: None,
        exited: false,
        _run: run,
    })
}

/// Argument pairs in order: each flag followed by its value.
fn pairs(items: &[(&str, &str)]) -> Vec<String> {
    items
        .iter()
        .flat_map(|(flag, value)| [(*flag).to_owned(), (*value).to_owned()])
        .collect()
}

/// The Restricted flags for a conversation turn: read only, no write, shell,
/// URL or memory tool.
fn restricted_flags() -> Vec<String> {
    pairs(&[
        ("--allow-tool", "read"),
        ("--deny-tool", "write"),
        ("--deny-tool", "shell"),
        ("--deny-tool", "url"),
        ("--deny-tool", "memory"),
    ])
}

/// The Auto flags for a conversation turn. Reads, and writes only under
/// `scratch`. A shell request with no allow rule goes to the judge. A
/// `git push` shell command is denied by rule.
fn auto_flags(scratch: &Path) -> Vec<String> {
    let write_rule = format!("write({}/**)", path_text(scratch));
    let mut args = vec!["--assisted-approval".to_owned()];
    args.extend(pairs(&[
        ("--allow-tool", "read"),
        ("--allow-tool", write_rule.as_str()),
        ("--deny-tool", "url"),
        ("--deny-tool", "memory"),
        ("--deny-tool", "shell(git push)"),
    ]));
    args
}

/// The arguments every run shares. The prompt is one argument. Each
/// variadic `--allow-tool` and `--deny-tool` value list is followed by a flag,
/// so no tool name can be read as a positional argument.
///
/// Full drops `--no-custom-instructions` and `--disallow-temp-dir`, because
/// its user chose to load the user's own instructions and temp dir. Every other
/// level keeps both.
///
/// User hooks: the CLI also runs the user's own Copilot hooks from
/// `~/.copilot` during every run, as it does when the user runs Copilot by
/// hand. They are the user's commands under the user's account. The daemon
/// does not disable them, because the CLI has no flag for that and moving
/// `COPILOT_HOME` would break the stored login. A hook error shows as a warning
/// (see `copilot_parser`). Decision recorded in the PR.
fn base_args(prompt: &str, usage_file: &Path, cwd: &Path, level: PermissionLevel) -> Vec<String> {
    let keeps_restrictions = level != PermissionLevel::Full;
    let mut args = vec![format!("--prompt={prompt}")];
    args.extend(["--output-format", "json", "--log-level", "none"].map(str::to_owned));
    args.extend(["--no-auto-update", "--no-ask-user"].map(str::to_owned));
    if keeps_restrictions {
        args.push("--no-custom-instructions".to_owned());
    }
    args.push("--disable-builtin-mcps".to_owned());
    if keeps_restrictions {
        args.push("--disallow-temp-dir".to_owned());
    }
    args.push("--usage-output-file".to_owned());
    args.push(path_text(usage_file));
    args.extend(["-C".to_owned(), path_text(cwd)]);
    args.extend(["--max-ai-credits".to_owned(), AI_CREDIT_CAP.to_owned()]);
    args
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

struct CopilotHandle {
    sup: ChildSupervisor,
    parser: CopilotParser,
    cancelled: bool,
    /// Last observed process exit code, if any.
    code: Option<i32>,
    /// True once `SessionExited` has been emitted.
    exited: bool,
    /// Declared last, so the child is stopped before its directory is removed.
    _run: Option<CopilotRun>,
}

#[async_trait]
impl AgentHandle for CopilotHandle {
    fn pid(&self) -> u32 {
        self.sup.pid()
    }

    fn try_exited(&self) -> std::io::Result<bool> {
        self.sup.try_exited()
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        loop {
            match self.sup.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    if let Some(event) = self.parser.feed(&line) {
                        return Ok(Some(event));
                    }
                }
                // Stderr is diagnostic; it does not fail the protocol.
                Some(SupervisionEvent::StderrLine(_)) => {}
                Some(SupervisionEvent::Exited { code, .. }) => {
                    self.code = code;
                    self.exited = true;
                    return Ok(Some(AgentEvent::SessionExited { exit_code: code }));
                }
                None => return Ok(None),
            }
        }
    }

    async fn cancel(&mut self) -> Result<()> {
        // A natural exit already observed is terminal; do not relabel it.
        if self.cancelled || self.exited {
            return Ok(());
        }
        self.cancelled = true;
        self.sup.cancel().await;
        Ok(())
    }

    async fn wait(&mut self) -> Result<AgentExit> {
        if self.exited {
            return Ok(self.outcome());
        }
        while let Some(event) = self.next_event().await? {
            if matches!(event, AgentEvent::SessionExited { .. }) {
                return Ok(self.outcome());
            }
        }
        Err(SupervisorError::NoExit.into())
    }
}

impl CopilotHandle {
    fn outcome(&self) -> AgentExit {
        terminal_outcome(self.cancelled, self.code, &self.parser)
    }
}

impl CopilotHandle {
    /// The sanitized error a hook reported during the run, if any. Shown as a
    /// warning when the run completed.
    fn warning(&self) -> Option<String> {
        self.parser.warning().map(str::to_owned)
    }
}

fn terminal_outcome(cancelled: bool, code: Option<i32>, parser: &CopilotParser) -> AgentExit {
    let last_message = parser.last_message().map(str::to_owned);
    if cancelled {
        return AgentExit::Cancelled { code, last_message };
    }
    if let Some(exit_code) = code.filter(|code| *code != 0) {
        let reason = match parser.failure() {
            Some(failure) => format!("agent exited with code {exit_code}; {failure}"),
            None => format!("agent exited with code {exit_code}"),
        };
        return AgentExit::Failed {
            code: Some(exit_code),
            reason: Some(reason),
        };
    }
    if parser.completed() {
        return AgentExit::Completed { code, last_message };
    }
    AgentExit::Failed {
        code,
        reason: Some(
            parser
                .failure()
                .unwrap_or_else(|| "the Copilot run ended without a result".to_owned()),
        ),
    }
}

#[async_trait]
impl AgentAdapter for CopilotAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::GithubCopilot
    }

    /// Checks that the executable exists and reports [`PINNED_VERSION`]. Any
    /// other version is refused, because its stream has not been verified.
    async fn detect(&self) -> Result<AgentCapabilities> {
        let program = self
            .locate()
            .ok_or_else(|| AgentError::NotFound(self.program().to_string_lossy().into_owned()))?;
        let version = probe_version(&program, VERSION_TIMEOUT)
            .await
            .map_err(AgentError::Unavailable)?;
        if version != PINNED_VERSION {
            return Err(AgentError::Unavailable(format!(
                "GitHub Copilot CLI {version} is not the verified version {PINNED_VERSION}"
            )));
        }
        Ok(self.capabilities())
    }

    async fn spawn(&self, request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>> {
        self.spawn_run(request, None).await
    }
}

impl RoutableAdapter for CopilotAdapter {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true, // CopilotHandle::next_event feeds stdout to CopilotParser::feed.
            interactive_input: false, // AgentHandle exposes no input method.
            structured_output: true, // `--output-format json` is JSONL (copilot_parser.rs).
            cancellation: true, // CopilotHandle::cancel calls ChildSupervisor::cancel.
            session_resumption: false, // `--resume` and `--continue` are never passed.
            approvals: false, // Prompts are denied with `--no-ask-user` and deny rules.
            model_selection: false, // Normal runs never pass `--model` or `--reasoning-effort`.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_of_the_version_output_is_the_version() {
        assert_eq!(
            parse_copilot_version("GitHub Copilot CLI 1.0.109."),
            Some("1.0.109".to_owned())
        );
        assert_eq!(
            parse_copilot_version("Copilot 1.0.108"),
            Some("1.0.108".to_owned())
        );
        assert_eq!(parse_copilot_version("no digits here"), None);
        assert_eq!(parse_copilot_version("GitHub Copilot CLI 1.0.1x9."), None);
    }
}
