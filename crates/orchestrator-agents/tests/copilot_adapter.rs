//! GitHub Copilot adapter (DPES-1105): argv, detection, environment,
//! sign-in, and replay of redacted Copilot CLI 1.0.109 streams through the real
//! supervisor path. Every process here is a fake shell script, never the real
//! `copilot` binary, so no test spends a premium request.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use orchestrator_agents::copilot_adapter::parse_copilot_version;
use orchestrator_agents::copilot_parser::CopilotParser;
use orchestrator_agents::copilot_sign_in::{classify_failure, CopilotSignIn};
use orchestrator_agents::copilot_usage::parse_usage;
use orchestrator_agents::workflow_agent::{WorkflowAgent, AGENT_ID};
use orchestrator_agents::{
    provider_of, AgentAdapter, AgentCatalog, AgentError, AgentEvent, AgentExit, AgentLaunchRequest,
    CatalogState, CopilotAdapter, CopilotRun, RoutableAdapter, SignInRecord, SignInState,
    TurnOutcome,
};
use orchestrator_core::permission::PermissionLevel;

const FORBIDDEN: [&str; 15] = [
    "--allow-all",
    "--allow-all-tools",
    "--allow-all-paths",
    "--allow-all-urls",
    "--yolo",
    "--allow-url",
    "--trust-folder",
    "--share",
    "--share-gist",
    "--autopilot",
    "--mode",
    "--add-dir",
    "--available-tools",
    "--resume",
    "--continue",
];

fn usage_path() -> PathBuf {
    PathBuf::from("/var/tmp/copilot-run/usage.json")
}

fn cwd_path() -> PathBuf {
    PathBuf::from("/var/tmp/copilot-run/cwd")
}

fn worktree_path() -> PathBuf {
    PathBuf::from("/var/tmp/task-worktree")
}

fn workflow_agent() -> WorkflowAgent {
    WorkflowAgent {
        plugin_dir: PathBuf::from("/p"),
        codex_instructions: String::new(),
    }
}

fn conversation(prompt: &str) -> Vec<String> {
    conversation_at(PermissionLevel::Restricted, prompt)
}

fn conversation_at(level: PermissionLevel, prompt: &str) -> Vec<String> {
    CopilotAdapter::conversation_args(level, prompt, &usage_path(), &cwd_path(), false)
        .expect("every level but Extended has a mapping")
}

fn task(objective: &str) -> Vec<String> {
    task_at(PermissionLevel::Restricted, objective)
}

fn task_at(level: PermissionLevel, objective: &str) -> Vec<String> {
    CopilotAdapter::task_args(
        objective,
        &usage_path(),
        &worktree_path(),
        level,
        false,
        None,
    )
}

fn sign_in_check() -> Vec<String> {
    CopilotAdapter::sign_in_check_args(&usage_path(), &cwd_path(), false)
}

/// The exact argv of each task level, written out in full so a change to any
/// flag shows here. Full drops the two flags every other level keeps.
#[test]
fn copilot_task_argv_per_level_is_exact() {
    let usage = usage_path().to_string_lossy().into_owned();
    let worktree = worktree_path().to_string_lossy().into_owned();
    let write_rule = format!("write({worktree}/**)");
    let objective = "do the work";
    let prompt = format!("--prompt={objective}");
    let expected = |level: PermissionLevel, tail: &[&str]| -> Vec<String> {
        let mut want = vec![prompt.clone()];
        want.extend(["--output-format", "json", "--log-level", "none"].map(str::to_owned));
        want.extend(["--no-auto-update", "--no-ask-user"].map(str::to_owned));
        if level != PermissionLevel::Full {
            want.push("--no-custom-instructions".to_owned());
        }
        want.push("--disable-builtin-mcps".to_owned());
        if level != PermissionLevel::Full {
            want.push("--disallow-temp-dir".to_owned());
        }
        want.extend(["--usage-output-file", usage.as_str()].map(str::to_owned));
        want.extend(["-C", worktree.as_str()].map(str::to_owned));
        want.extend(tail.iter().map(|flag| {
            flag.replace("<write>", &write_rule)
                .replace("<wt>", worktree.as_str())
        }));
        want
    };
    let cases = [
        (
            PermissionLevel::Restricted,
            vec![
                "--allow-tool",
                "read",
                "--allow-tool",
                "<write>",
                "--deny-tool",
                "shell",
                "--deny-tool",
                "url",
                "--deny-tool",
                "memory",
            ],
        ),
        (
            PermissionLevel::Extended,
            vec![
                "--allow-tool",
                "read",
                "--allow-tool",
                "<write>",
                "--allow-tool",
                "shell(git status)",
                "--allow-tool",
                "shell(git diff)",
                "--allow-tool",
                "shell(git log)",
                "--allow-tool",
                "shell(git show)",
                "--deny-tool",
                "url",
                "--deny-tool",
                "memory",
            ],
        ),
        (
            PermissionLevel::Auto,
            vec![
                "--assisted-approval",
                "--allow-tool",
                "read",
                "--allow-tool",
                "<write>",
                "--deny-tool",
                "url",
                "--deny-tool",
                "memory",
                "--deny-tool",
                "shell(git push)",
            ],
        ),
        (PermissionLevel::Full, vec!["--allow-all"]),
    ];
    for (level, tail) in cases {
        assert_eq!(
            task_at(level, objective),
            expected(level, &tail),
            "{level:?}"
        );
    }
}

/// Values that follow `flag` in `args`, in order.
fn values_of(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .collect()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(name)
}

#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A fake `copilot` with metadata commands separate from inference.
#[cfg(unix)]
fn fake_copilot(dir: &Path, name: &str, version: &str, run_body: &str) -> PathBuf {
    let body = format!(
        "if [ \"$1\" = \"--version\" ]; then\n  printf '%s\\n' 'GitHub Copilot CLI {version}.' \"Run 'copilot update' to check for updates.\"\n  exit 0\nfi\n\
         if [ \"$1\" = \"--help\" ]; then\n  printf '%s\\n' --prompt --output-format --log-level --no-auto-update --no-ask-user --no-custom-instructions --disable-builtin-mcps --disallow-temp-dir --usage-output-file -C --allow-tool --deny-tool --model --max-ai-credits\n  exit 0\nfi\n{run_body}"
    );
    script(dir, name, &body)
}

#[cfg(unix)]
fn replay_body(stream: &Path, exit_code: i32) -> String {
    format!("cat '{}'\nexit {exit_code}\n", stream.display())
}

#[test]
fn no_run_ever_passes_a_permission_widening_or_resume_flag() {
    for args in [
        conversation("hello"),
        task("fix the parser"),
        CopilotAdapter::task_args(
            "fix the parser",
            &usage_path(),
            &worktree_path(),
            PermissionLevel::Restricted,
            false,
            Some(&workflow_agent()),
        ),
        sign_in_check(),
    ] {
        for flag in FORBIDDEN {
            assert!(!args.iter().any(|arg| arg == flag), "{flag} in {args:?}");
        }
        assert!(
            !args.iter().any(|arg| arg.starts_with("--allow-all")),
            "{args:?}"
        );
        assert!(!args.iter().any(|arg| arg == "--available-tools"));
        assert!(!args
            .windows(2)
            .any(|pair| pair[0] == "--allow-tool" && pair[1] == "shell"));
        assert!(!args.iter().any(|arg| arg == "--reasoning-effort"));
        assert!(!args.iter().any(|arg| arg == "--sandbox"));
        assert!(args.iter().any(|arg| arg == "--disallow-temp-dir"));
    }
}

/// With a workflow agent, every task level appends `--plugin-dir` and `--agent`
/// after its own flags, and changes nothing else. The `None` argv is pinned
/// exactly above.
#[test]
fn copilot_task_with_workflow_agent_appends_plugin_and_agent_at_every_level() {
    for level in [
        PermissionLevel::Restricted,
        PermissionLevel::Extended,
        PermissionLevel::Auto,
        PermissionLevel::Full,
    ] {
        let with_agent = CopilotAdapter::task_args(
            "do it",
            &usage_path(),
            &worktree_path(),
            level,
            false,
            Some(&workflow_agent()),
        );
        let mut want = task_at(level, "do it");
        want.extend(["--plugin-dir", "/p", "--agent", AGENT_ID].map(str::to_owned));
        assert_eq!(with_agent, want, "{level:?}");
    }
}

/// The credit cap is off by default. When it is on, every Copilot run passes
/// exactly one `--max-ai-credits 30`: a chat, a task and the sign-in check.
#[test]
fn credit_cap_is_off_by_default_and_exactly_thirty_when_on() {
    let chat_off = conversation("hi");
    let task_off = task_at(PermissionLevel::Auto, "x");
    let check_off = sign_in_check();
    for args in [&chat_off, &task_off, &check_off] {
        assert!(
            !args.iter().any(|arg| arg == "--max-ai-credits"),
            "{args:?}"
        );
    }

    let chat_on = CopilotAdapter::conversation_args(
        PermissionLevel::Auto,
        "hi",
        &usage_path(),
        &cwd_path(),
        true,
    )
    .unwrap();
    let task_on = CopilotAdapter::task_args(
        "x",
        &usage_path(),
        &worktree_path(),
        PermissionLevel::Auto,
        true,
        None,
    );
    let check_on = CopilotAdapter::sign_in_check_args(&usage_path(), &cwd_path(), true);
    for args in [&chat_on, &task_on, &check_on] {
        assert_eq!(values_of(args, "--max-ai-credits"), ["30"], "{args:?}");
        assert_eq!(
            args.iter().filter(|arg| *arg == "--max-ai-credits").count(),
            1,
            "{args:?}"
        );
    }
}

/// The exact argv of each conversation level. Paths are spelled as the
/// platform spells them, so the expected vectors build them the same way.
#[test]
fn copilot_conversation_argv_per_level_is_exact() {
    let usage = usage_path().to_string_lossy().into_owned();
    let cwd = cwd_path().to_string_lossy().into_owned();
    let write_rule = format!("write({cwd}/**)");
    fn as_strs(args: &[String]) -> Vec<&str> {
        args.iter().map(String::as_str).collect()
    }
    let argv = |level| {
        CopilotAdapter::conversation_args(level, "hi", &usage_path(), &cwd_path(), false).unwrap()
    };

    assert_eq!(
        as_strs(&argv(PermissionLevel::Restricted)),
        [
            "--prompt=hi",
            "--output-format",
            "json",
            "--log-level",
            "none",
            "--no-auto-update",
            "--no-ask-user",
            "--no-custom-instructions",
            "--disable-builtin-mcps",
            "--disallow-temp-dir",
            "--usage-output-file",
            usage.as_str(),
            "-C",
            cwd.as_str(),
            "--allow-tool",
            "read",
            "--deny-tool",
            "write",
            "--deny-tool",
            "shell",
            "--deny-tool",
            "url",
            "--deny-tool",
            "memory",
        ]
    );
    assert_eq!(
        as_strs(&argv(PermissionLevel::Auto)),
        [
            "--prompt=hi",
            "--output-format",
            "json",
            "--log-level",
            "none",
            "--no-auto-update",
            "--no-ask-user",
            "--no-custom-instructions",
            "--disable-builtin-mcps",
            "--disallow-temp-dir",
            "--usage-output-file",
            usage.as_str(),
            "-C",
            cwd.as_str(),
            "--assisted-approval",
            "--allow-tool",
            "read",
            "--allow-tool",
            write_rule.as_str(),
            "--deny-tool",
            "url",
            "--deny-tool",
            "memory",
            "--deny-tool",
            "shell(git push)",
        ]
    );
    assert_eq!(
        as_strs(&argv(PermissionLevel::Full)),
        [
            "--prompt=hi",
            "--output-format",
            "json",
            "--log-level",
            "none",
            "--no-auto-update",
            "--no-ask-user",
            "--disable-builtin-mcps",
            "--usage-output-file",
            usage.as_str(),
            "-C",
            cwd.as_str(),
            "--allow-all",
        ]
    );
    assert!(CopilotAdapter::conversation_args(
        PermissionLevel::Extended,
        "hi",
        &usage_path(),
        &cwd_path(),
        false
    )
    .is_none());
}

/// Only Full may widen. Every level, Full included, keeps the hard limits: no
/// autopilot, experimental mode, sandbox, YOLO or MCP instruction widening.
#[test]
fn copilot_only_full_widens_and_no_level_takes_a_hard_limit_flag() {
    for level in [
        PermissionLevel::Restricted,
        PermissionLevel::Auto,
        PermissionLevel::Full,
    ] {
        let args = conversation_at(level, "hi");
        for flag in [
            "--autopilot",
            "--experimental",
            "--allow-all-mcp-server-instructions",
            "--sandbox",
            "--yolo",
            "--available-tools",
            "--add-dir",
        ] {
            assert!(!args.iter().any(|arg| arg == flag), "{level:?}: {flag}");
        }
        let widens = args.iter().any(|arg| arg.starts_with("--allow-all"));
        assert_eq!(widens, level == PermissionLevel::Full, "{level:?}");
    }
}

#[test]
fn a_leading_dash_prompt_stays_one_argument_after_prompt_equals() {
    let args = conversation("--leading-dash and more");
    assert_eq!(args[0], "--prompt=--leading-dash and more");
    assert_eq!(
        args.iter()
            .filter(|arg| arg.contains("leading-dash"))
            .count(),
        1
    );
    let args = task("--dangerous objective");
    assert_eq!(args[0], "--prompt=--dangerous objective");
}

#[test]
fn the_conversation_reads_its_directory_and_can_do_nothing_else() {
    let args = conversation("hi");
    assert_eq!(values_of(&args, "--allow-tool"), ["read"]);
    let denied = values_of(&args, "--deny-tool");
    for tool in ["write", "shell", "url", "memory"] {
        assert!(denied.contains(&tool.to_owned()), "{tool} not denied");
    }
    assert_eq!(values_of(&args, "-C"), [cwd_path().display().to_string()]);
}

#[test]
fn a_task_writes_only_under_its_worktree_and_runs_no_shell() {
    let args = task("do the work");
    let allowed = values_of(&args, "--allow-tool");
    assert_eq!(
        allowed,
        [
            "read".to_owned(),
            format!("write({}/**)", worktree_path().display())
        ]
    );
    let denied = values_of(&args, "--deny-tool");
    for tool in ["shell", "url", "memory"] {
        assert!(denied.contains(&tool.to_owned()), "{tool} not denied");
    }
    assert_eq!(
        values_of(&args, "-C"),
        [worktree_path().display().to_string()]
    );
}

#[test]
fn only_the_sign_in_check_and_a_chosen_selection_name_a_model() {
    let check = sign_in_check();
    assert_eq!(values_of(&check, "--model"), ["claude-haiku-4.5"]);
    assert!(!conversation("hi").iter().any(|arg| arg == "--model"));
    assert!(!task("x").iter().any(|arg| arg == "--model"));

    assert!(CopilotAdapter::selection_args(None, None).is_empty());
    assert_eq!(
        CopilotAdapter::selection_args(Some("gpt-a"), Some("high")),
        ["--model", "gpt-a", "--reasoning-effort", "high"]
    );
    assert_eq!(
        CopilotAdapter::selection_args(None, Some("low")),
        ["--reasoning-effort", "low"]
    );
}

/// A fake `copilot --acp` that answers the four calls the model read makes. `auto` is listed
/// twice, and only `gpt-a` has reasoning levels.
#[cfg(unix)]
pub const FAKE_ACP: &str = r#"if [ "$1" = "--version" ]; then printf '%s\n' 'GitHub Copilot CLI 1.0.114.'; exit 0; fi
if [ "$1" != "--acp" ]; then exit 1; fi
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1}}\n' "$id" ;;
    *'"session/new"'*) printf '{"jsonrpc":"2.0","method":"session/update","params":{}}\n{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"s1","models":{"availableModels":[{"modelId":"auto","name":"Auto"},{"modelId":"auto","name":"Auto"},{"modelId":"gpt-a","name":"GPT A"},{"modelId":"mini","name":"Mini"}]}}}\n' "$id" ;;
    *'"value":"gpt-a"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"configOptions":[{"id":"reasoning_effort","currentValue":"high","options":[{"value":"low"},{"value":"high"}]}]}}\n' "$id" ;;
    *'"session/set_config_option"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"configOptions":[]}}\n' "$id" ;;
    *'"session/close"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"; exit 0 ;;
  esac
done"#;

#[cfg(unix)]
#[tokio::test]
async fn the_catalog_lists_the_models_and_levels_the_cli_reports() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "acp", FAKE_ACP);
    let signed_in = SignInRecord {
        state: SignInState::Ready,
        ..SignInRecord::default()
    };
    let catalog = CopilotAdapter::new()
        .with_program(&program)
        .catalog(Duration::from_secs(10), &signed_in)
        .await;
    assert_eq!(catalog.models.state, CatalogState::Available, "{catalog:?}");
    let listed: Vec<(&str, &[String], Option<&str>)> = catalog
        .models
        .items
        .iter()
        .map(|m| (m.id.as_str(), m.efforts.as_slice(), m.default_effort.as_deref()))
        .collect();
    let levels = ["low".to_owned(), "high".to_owned()];
    assert_eq!(
        listed,
        [("auto", &[][..], None), ("gpt-a", &levels[..], Some("high")), ("mini", &[][..], None)]
    );
    // Levels are per model, so the adapter-wide list says so and offers none.
    assert_eq!(catalog.efforts.state, CatalogState::Unsupported);
}

#[cfg(unix)]
#[tokio::test]
async fn a_server_that_does_not_answer_leaves_the_models_unknown_not_empty() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(dir.path(), "silent", "1.0.114", "exit 1\n");
    let catalog = CopilotAdapter::new()
        .with_program(&program)
        .catalog(Duration::from_secs(10), &SignInRecord::default())
        .await;
    assert_eq!(catalog.models.state, CatalogState::Unknown, "{catalog:?}");
    assert!(catalog.models.items.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn a_signed_out_account_is_not_asked_for_models() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("acp-started");
    let program = script(
        dir.path(),
        "guarded",
        &format!("if [ \"$1\" = \"--acp\" ]; then touch '{}'; exit 1; fi\nprintf '%s\\n' 'GitHub Copilot CLI 1.0.114.'", marker.display()),
    );
    let signed_out = SignInRecord {
        state: SignInState::NotSignedIn,
        ..SignInRecord::default()
    };
    let catalog = CopilotAdapter::new()
        .with_program(&program)
        .catalog(Duration::from_secs(10), &signed_out)
        .await;
    assert_eq!(catalog.state, CatalogState::AuthenticationFailed);
    assert_eq!(catalog.models.state, CatalogState::Unknown);
    assert!(!marker.exists(), "the model read started for a signed-out account");
}

#[test]
fn every_run_writes_usage_outside_its_working_directory() {
    for args in [conversation("hi"), task("x"), sign_in_check()] {
        let usage = values_of(&args, "--usage-output-file");
        assert_eq!(usage.len(), 1);
        assert!(!usage[0].starts_with(&worktree_path().display().to_string()));
    }
}

// --- detection, pins, catalog ----------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn a_missing_binary_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let error = CopilotAdapter::new()
        .with_program(dir.path().join("no-such-copilot"))
        .detect()
        .await
        .unwrap_err();
    assert!(matches!(error, AgentError::NotFound(_)), "{error}");
}

#[cfg(unix)]
#[tokio::test]
async fn detection_is_independent_of_the_verified_version() {
    let dir = tempfile::tempdir().unwrap();
    let pinned = fake_copilot(dir.path(), "pinned", "1.0.109", "exit 0\n");
    let caps = CopilotAdapter::new()
        .with_program(&pinned)
        .detect()
        .await
        .expect("1.0.109 is pinned");
    assert_eq!(caps, CopilotAdapter::new().capabilities());
    assert!(caps.model_selection && caps.streaming && caps.cancellation);

    let newer = fake_copilot(dir.path(), "newer", "1.2.3", "exit 0\n");
    assert_eq!(
        CopilotAdapter::new()
            .with_program(&newer)
            .detect()
            .await
            .unwrap(),
        caps
    );
}

#[test]
fn the_version_comes_from_the_first_line_only() {
    assert_eq!(
        parse_copilot_version("GitHub Copilot CLI 1.0.109."),
        Some("1.0.109".to_owned())
    );
    assert_eq!(parse_copilot_version("Run 'copilot update' to check"), None);
}

#[test]
fn copilot_belongs_to_the_github_vendor_family() {
    assert_eq!(provider_of("github-copilot"), Some("github"));
}

#[tokio::test]
async fn a_missing_copilot_catalog_is_unavailable_not_empty() {
    let dir = tempfile::tempdir().unwrap();
    let catalog: AgentCatalog = CopilotAdapter::new()
        .with_program(dir.path().join("absent"))
        .catalog(Duration::from_secs(1), &SignInRecord::default())
        .await;
    assert_eq!(catalog.state, CatalogState::Unavailable);
    assert!(catalog.models.items.is_empty());
}

/// DPES-1105: Windows is not refused. With no CLI installed, the sign-in check
/// says the executable is missing, the same as on every other host.
#[cfg(windows)]
#[tokio::test]
async fn windows_is_not_refused_and_a_missing_cli_is_reported_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let record = CopilotAdapter::new()
        .with_program(dir.path().join("absent.exe"))
        .sign_in_check(dir.path(), Duration::from_secs(5), false)
        .await;
    assert_eq!(record.state, SignInState::NotChecked);
    assert!(record
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("not found"));
}

#[cfg(unix)]
#[tokio::test]
async fn the_catalog_states_follow_the_sign_in_record() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(dir.path(), "copilot", "1.0.109", "exit 0\n");
    let adapter = CopilotAdapter::new().with_program(&program);
    let limit = Duration::from_secs(5);
    let record = |state| SignInRecord {
        state,
        reason: Some("reason text".to_owned()),
        ..SignInRecord::default()
    };

    let not_checked = adapter.catalog(limit, &SignInRecord::default()).await;
    assert_eq!(not_checked.state, CatalogState::Available);
    assert!(not_checked.detail.starts_with("Not confirmed yet"));
    assert_eq!(not_checked.provider.as_deref(), Some("github"));
    assert_eq!(not_checked.models.state, CatalogState::Unsupported);
    assert_eq!(not_checked.efforts.state, CatalogState::Unsupported);

    let signed_out = adapter
        .catalog(limit, &record(SignInState::NotSignedIn))
        .await;
    assert_eq!(signed_out.state, CatalogState::AuthenticationFailed);
    assert!(signed_out.detail.contains("copilot login"));

    let no_seat = adapter.catalog(limit, &record(SignInState::NoSeat)).await;
    assert_eq!(no_seat.state, CatalogState::Unavailable);
    assert_eq!(
        no_seat.detail,
        "This GitHub account has no GitHub Copilot subscription."
    );

    let errored = adapter.catalog(limit, &record(SignInState::Error)).await;
    assert_eq!(errored.state, CatalogState::Unknown);
    assert_eq!(errored.detail, "reason text");
}

#[cfg(unix)]
#[tokio::test]
async fn a_catalog_for_an_unseen_version_keeps_provider_and_model_support_separate() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(dir.path(), "copilot", "1.0.108", "exit 0\n");
    let catalog = CopilotAdapter::new()
        .with_program(&program)
        .catalog(Duration::from_secs(5), &SignInRecord::default())
        .await;
    assert_eq!(catalog.state, CatalogState::Available);
    assert_eq!(catalog.models.state, CatalogState::Unsupported);
    assert_eq!(catalog.efforts.state, CatalogState::Unsupported);
}

// --- environment and argument transport through the real supervisor ---------

#[cfg(unix)]
#[tokio::test]
async fn the_child_gets_no_github_token_and_keeps_path_and_home() {
    // COPILOT_ALLOW_ALL=true would trust the working directory's hooks and MCP
    // servers (copilot help environment), so no run may ever receive it.
    for name in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "COPILOT_GITHUB_TOKEN",
        "COPILOT_ALLOW_ALL",
    ] {
        std::env::set_var(name, "must-not-reach-the-child");
    }
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "copilot", "env > \"$PWD/env.txt\"\nexit 0\n");
    let run = CopilotRun::create(dir.path()).unwrap();
    let cwd = run.cwd();
    let request = AgentLaunchRequest {
        program,
        args: CopilotAdapter::conversation_args(
            PermissionLevel::Restricted,
            "hi",
            &run.usage_file(),
            &run.cwd(),
            false,
        )
        .unwrap(),
        working_dir: run.cwd(),
    };
    let mut handle = CopilotAdapter::new()
        .spawn_run(request, Some(run))
        .await
        .unwrap();
    handle.wait().await.unwrap();
    let env = std::fs::read_to_string(cwd.join("env.txt")).unwrap();
    for name in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "COPILOT_GITHUB_TOKEN",
        "COPILOT_ALLOW_ALL",
    ] {
        assert!(
            !env.lines()
                .any(|line| line.starts_with(&format!("{name}="))),
            "{name} reached the child"
        );
    }
    assert!(
        env.lines().any(|line| line.starts_with("PATH=")),
        "PATH missing"
    );
    assert!(
        env.lines().any(|line| line.starts_with("HOME=")),
        "HOME missing"
    );
    for name in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "COPILOT_GITHUB_TOKEN",
        "COPILOT_ALLOW_ALL",
    ] {
        std::env::remove_var(name);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_leading_dash_prompt_reaches_the_child_as_one_argument() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(
        dir.path(),
        "copilot",
        "for a in \"$@\"; do printf '%s\\0' \"$a\" >> \"$PWD/argv.bin\"; done\nexit 0\n",
    );
    let run = CopilotRun::create(dir.path()).unwrap();
    let cwd = run.cwd();
    let args = CopilotAdapter::conversation_args(
        PermissionLevel::Restricted,
        "--leading-dash",
        &run.usage_file(),
        &run.cwd(),
        false,
    )
    .unwrap();
    let request = AgentLaunchRequest {
        program,
        args: args.clone(),
        working_dir: run.cwd(),
    };
    let mut handle = CopilotAdapter::new()
        .spawn_run(request, Some(run))
        .await
        .unwrap();
    handle.wait().await.unwrap();
    let received = std::fs::read(cwd.join("argv.bin")).unwrap();
    let elements: Vec<String> = received
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8(part.to_vec()).unwrap())
        .collect();
    assert_eq!(elements, args);
    assert_eq!(elements[0], "--prompt=--leading-dash");
}

// --- parser on redacted fixtures and inline streams ---------------------------

#[test]
fn the_bogus_model_fixture_fails_with_the_providers_own_words() {
    let text = std::fs::read_to_string(fixture("bogus_model.jsonl")).unwrap();
    let mut parser = CopilotParser::new();
    let mut outcome = None;
    for line in text.lines() {
        if let Some(AgentEvent::TurnFinished { outcome: value }) = parser.feed(line) {
            outcome = Some(value);
        }
    }
    assert_eq!(outcome, Some(TurnOutcome::Failure));
    assert!(!parser.completed());
    let failure = parser.failure().expect("the session error is reported");
    assert!(failure.contains("is not available"), "{failure}");
}

/// A successful run in the verified event layout (field names and nesting as
/// in the Copilot CLI 1.0.109 capture). Inline on purpose: the redacted capture
/// file is pending, so this test does not claim to be a recorded run.
const COMPLETED_STREAM: [&str; 3] = [
    r#"{"type":"session.start","data":{"sessionId":"11111111-2222-4333-8444-555555555555","copilotVersion":"1.0.109","selectedModel":"claude-opus-4.8"}}"#,
    r#"{"type":"assistant.message","data":{"content":"PONG","toolRequests":[],"providerId":"copilot"}}"#,
    r#"{"type":"result","timestamp":"2026-11-15T01:51:04.132Z","sessionId":"11111111-2222-4333-8444-555555555555","exitCode":0,"usage":{"premiumRequests":15}}"#,
];

#[test]
fn a_reply_with_a_zero_result_completes() {
    let mut parser = CopilotParser::new();
    let mut outcome = None;
    for line in COMPLETED_STREAM {
        if let Some(AgentEvent::TurnFinished { outcome: value }) = parser.feed(line) {
            outcome = Some(value);
        }
    }
    assert_eq!(outcome, Some(TurnOutcome::Success));
    assert!(parser.completed());
    assert_eq!(parser.last_message(), Some("PONG"));
    assert!(parser.failure().is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn a_replayed_completed_run_exits_completed_and_marks_sign_in_ready() {
    let dir = tempfile::tempdir().unwrap();
    let stream = dir.path().join("completed.jsonl");
    std::fs::write(&stream, COMPLETED_STREAM.join("\n") + "\n").unwrap();
    let program = fake_copilot(dir.path(), "copilot", "1.0.109", &replay_body(&stream, 0));
    let run = CopilotRun::create(dir.path()).unwrap();
    let request = AgentLaunchRequest {
        program,
        args: CopilotAdapter::conversation_args(
            PermissionLevel::Restricted,
            "hi",
            &run.usage_file(),
            &run.cwd(),
            false,
        )
        .unwrap(),
        working_dir: run.cwd(),
    };
    let mut handle = CopilotAdapter::new()
        .spawn_run(request, Some(run))
        .await
        .unwrap();
    assert_eq!(
        handle.wait().await.unwrap(),
        AgentExit::Completed {
            code: Some(0),
            last_message: Some("PONG".to_owned()),
        }
    );
    let cache = CopilotSignIn::default();
    cache.record_run(&AgentExit::Completed {
        code: Some(0),
        last_message: Some("PONG".to_owned()),
    });
    assert_eq!(cache.snapshot().state, SignInState::Ready);
}

#[test]
fn unknown_event_types_and_non_json_lines_change_nothing() {
    let mut parser = CopilotParser::new();
    assert_eq!(parser.feed("warning: something went sideways"), None);
    assert_eq!(
        parser.feed(r#"{"type":"future.event","data":{"x":1}}"#),
        None
    );
    assert_eq!(
        parser.feed(r#"{"type":"session.skills_loaded","data":{}}"#),
        None
    );
    assert_eq!(
        parser.feed(r#"{"type":"assistant.message_delta","data":{"deltaContent":"O"}}"#),
        None
    );
    let reply =
        parser.feed(r#"{"type":"assistant.message","data":{"content":"OK","toolRequests":[]}}"#);
    assert_eq!(
        reply,
        Some(AgentEvent::MessageDelta {
            text: "OK".to_owned()
        })
    );
    assert_eq!(parser.last_message(), Some("OK"));
}

/// One `assistant.message` line yields its reasoning, then its reply.
#[test]
fn reasoning_text_yields_a_thinking_event_before_the_message() {
    let stream = std::fs::read_to_string(fixture("success_clean.jsonl")).unwrap();
    let mut parser = CopilotParser::new();
    let mut events = Vec::new();
    for line in stream.lines() {
        events.extend(parser.feed(line));
        events.extend(parser.take_pending());
    }
    let thinking = events
        .iter()
        .position(|e| matches!(e, AgentEvent::ThinkingDelta { text } if text.starts_with("The user is asking")))
        .expect("thinking event");
    assert!(matches!(
        &events[thinking + 1],
        AgentEvent::MessageDelta { text } if text == "OK"
    ));
    assert_eq!(parser.last_message(), Some("OK"));
    assert_eq!(parser.take_pending(), None);

    let mut parser = CopilotParser::new();
    let only_thinking =
        r#"{"type":"assistant.message","data":{"content":"","reasoningText":"hm"}}"#;
    assert_eq!(
        parser.feed(only_thinking),
        Some(AgentEvent::ThinkingDelta {
            text: "hm".to_owned()
        })
    );
    assert_eq!(parser.take_pending(), None);
    assert_eq!(parser.last_message(), None);
}

#[test]
fn a_truncated_stream_never_completes() {
    let mut parser = CopilotParser::new();
    parser.feed(
        r#"{"type":"session.start","data":{"sessionId":"11111111-2222-4333-8444-555555555555"}}"#,
    );
    parser.feed(r#"{"type":"assistant.message","data":{"content":"PONG"}}"#);
    assert!(
        !parser.completed(),
        "a reply without a result is not a completed run"
    );
    assert_eq!(parser.failure(), None);
}

/// A hook error beside a completed turn is a warning. The turn is complete.
#[test]
fn a_session_error_beside_a_completed_turn_is_a_warning() {
    let mut parser = CopilotParser::new();
    parser.feed(r#"{"type":"assistant.message","data":{"content":"PONG"}}"#);
    parser.feed(r#"{"type":"session.error","data":{"errorType":"query","message":"boom"}}"#);
    parser.feed(r#"{"type":"result","exitCode":0}"#);
    assert!(parser.completed());
    assert_eq!(parser.warning(), Some("boom"));
    assert_eq!(parser.failure(), None);
}

/// A nonzero exit still fails, and the reported error is its detail.
#[test]
fn a_nonzero_result_fails_with_the_reported_error_as_detail() {
    let mut parser = CopilotParser::new();
    parser.feed(r#"{"type":"assistant.message","data":{"content":"PONG"}}"#);
    parser.feed(r#"{"type":"session.error","data":{"errorType":"query","message":"boom"}}"#);
    parser.feed(r#"{"type":"result","exitCode":1}"#);
    assert!(!parser.completed());
    let failure = parser.failure().expect("a nonzero exit is a failure");
    assert!(failure.contains("exit code 1"), "{failure}");
    assert!(failure.ends_with("boom"), "{failure}");
}

/// Exit 0 without a reply is not a completed turn.
#[test]
fn an_exit_zero_without_a_reply_still_fails() {
    let mut parser = CopilotParser::new();
    parser.feed(r#"{"type":"result","exitCode":0}"#);
    assert!(!parser.completed());
    assert_eq!(
        parser.failure().as_deref(),
        Some("Copilot finished without a reply")
    );
}

#[test]
fn provider_text_is_masked_and_capped() {
    let mut parser = CopilotParser::new();
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    parser.feed(&format!(
        r#"{{"type":"session.error","data":{{"errorType":"x","message":"token {secret} {}"}}}}"#,
        "x".repeat(400)
    ));
    let failure = parser.failure().unwrap();
    assert!(!failure.contains(secret), "{failure}");
    assert!(failure.contains("[REDACTED]"));
    assert!(failure.chars().count() <= 200);
}

// --- usage ------------------------------------------------------------------

#[test]
fn a_missing_or_unparseable_usage_file_is_not_reported_never_zero() {
    assert_eq!(
        orchestrator_agents::copilot_usage::read_usage(Path::new("/no/such/usage.json")),
        None
    );
    assert_eq!(parse_usage("not json"), None);
    assert_eq!(parse_usage("{}"), None);
    assert_eq!(parse_usage(r#"{"totalPremiumRequestCost":"lots"}"#), None);
}

#[test]
fn usage_keeps_premium_requests_and_credits_in_their_own_units() {
    let usage = parse_usage(
        r#"{"totalPremiumRequestCost":0.33,"totalNanoAiu":1500000000,
            "providerModelMetrics":[{"provider":{"id":"copilot"},"modelId":"claude-haiku-4.5",
            "metrics":{"usage":{"inputTokens":10,"outputTokens":2,"cacheReadTokens":0,"cacheWriteTokens":7}}}]}"#,
    )
    .unwrap();
    assert_eq!(usage.premium_requests, Some(0.33));
    assert_eq!(usage.ai_credits, Some(1.5));
    assert_eq!(usage.models[0].model_id, "claude-haiku-4.5");
    assert_eq!(usage.models[0].input_tokens, Some(10));
    assert_eq!(usage.models[0].cache_write_tokens, Some(7));
}

// --- explicit sign-in check and session state -----------------------------------

#[tokio::test]
async fn a_check_that_cannot_run_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let record = CopilotAdapter::new()
        .with_program(dir.path().join("absent"))
        .sign_in_check(dir.path(), Duration::from_secs(5), false)
        .await;
    assert_eq!(record.state, SignInState::NotChecked);
    assert!(record.reason.is_some());
    assert_eq!(record.checked_at_ms, None);
}

#[cfg(unix)]
#[tokio::test]
async fn a_check_that_meets_the_bogus_model_reports_a_model_error() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(
        dir.path(),
        "copilot",
        "1.0.109",
        &replay_body(&fixture("bogus_model.jsonl"), 1),
    );
    let record = CopilotAdapter::new()
        .with_program(&program)
        .sign_in_check(dir.path(), Duration::from_secs(10), false)
        .await;
    assert_eq!(record.state, SignInState::Error);
    assert!(record.reason.unwrap().starts_with("model unavailable"));
    assert!(record.checked_at_ms.is_some());
}

/// The not-signed-in check, cached for the session. Synchronous, because the
/// daemon's `CopilotSignIn::check` takes a blocking closure; that closure runs
/// its own runtime, so this test must not run inside one.
#[cfg(unix)]
#[test]
fn a_check_that_is_not_signed_in_says_so_and_is_cached() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("stream.txt");
    std::fs::write(
        &text,
        "{\"type\":\"session.error\",\"data\":{\"errorType\":\"auth\",\"message\":\"Not logged in. Please run copilot login.\"}}\n{\"type\":\"result\",\"exitCode\":1}\n",
    )
    .unwrap();
    let program = fake_copilot(dir.path(), "copilot", "1.0.109", &replay_body(&text, 1));
    let adapter = CopilotAdapter::new().with_program(&program);
    let cache = Arc::new(CopilotSignIn::default());
    let parent = dir.path().to_path_buf();
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    let started = cache.start(false, move || {
        counted.fetch_add(1, Ordering::SeqCst);
        futures_block(adapter.sign_in_check(&parent, Duration::from_secs(10), false))
    });
    assert_eq!(
        started.state,
        SignInState::Checking,
        "the check runs in the background"
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while cache.snapshot().state == SignInState::Checking {
        assert!(
            Instant::now() < deadline,
            "the background check never settled"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(cache.snapshot().state, SignInState::NotSignedIn);
    let again = cache.start(false, || {
        panic!("a settled answer is served from the session")
    });
    assert_eq!(again.state, SignInState::NotSignedIn);
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "the second plain check is served from the session"
    );
}

/// A completed turn, replayed through the real adapter path, records sign-in as
/// ready. The event shapes are those the Copilot CLI wrote in a real run: the
/// `result` carries `exitCode` at the top level.
#[cfg(unix)]
#[test]
fn a_check_whose_turn_completes_records_ready() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("stream.txt");
    std::fs::write(
        &text,
        concat!(
            "{\"type\":\"session.start\",\"data\":{\"sessionId\":\"00000000-0000-4000-8000-000000000001\",\"copilotVersion\":\"1.0.109\",\"selectedModel\":\"claude-haiku-4.5\"}}\n",
            "{\"type\":\"assistant.message\",\"data\":{\"content\":\"OK\",\"model\":\"claude-haiku-4.5\",\"providerId\":\"copilot\",\"toolRequests\":[]}}\n",
            "{\"type\":\"assistant.turn_end\",\"data\":{\"turnId\":\"0\"}}\n",
            "{\"type\":\"result\",\"exitCode\":0,\"usage\":{\"premiumRequests\":0.33,\"totalApiDurationMs\":1791}}\n",
        ),
    )
    .unwrap();
    let program = fake_copilot(dir.path(), "copilot", "1.0.109", &replay_body(&text, 0));
    let adapter = CopilotAdapter::new().with_program(&program);
    let record = futures_block(adapter.sign_in_check(dir.path(), Duration::from_secs(10), false));
    assert_eq!(
        record.state,
        SignInState::Ready,
        "a completed turn is a sign-in fact"
    );
    assert!(record.checked_at_ms.is_some());
}

/// A real captured stream, redacted (see `tests/fixtures/copilot/` in the PR).
fn captured(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Runs the sign-in check against a fake CLI that replays `stream` and exits
/// with `exit_code`. The real adapter path runs; only the process is a fake.
#[cfg(unix)]
fn sign_in_over(stream: &str, exit_code: i32) -> SignInRecord {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("stream.txt");
    std::fs::write(&text, stream).unwrap();
    let program = fake_copilot(
        dir.path(),
        "copilot",
        "1.0.109",
        &replay_body(&text, exit_code),
    );
    let adapter = CopilotAdapter::new().with_program(&program);
    futures_block(adapter.sign_in_check(dir.path(), Duration::from_secs(10), false))
}

/// The captured run: the agentStop hook failed in the sandbox, the turn still
/// replied `OK`, and the result exited 0. That is ready, with the hook as a warning.
#[cfg(unix)]
#[test]
fn a_real_turn_with_a_hook_error_is_ready_with_the_hook_as_a_warning() {
    let record = sign_in_over(&captured("hook_error_then_success.jsonl"), 0);
    assert_eq!(record.state, SignInState::Ready);
    let warning = record
        .warning
        .expect("the hook error is shown as a warning");
    assert!(warning.starts_with("agentStop hook from"), "{warning}");
    assert!(warning.chars().count() <= 200, "the warning is capped");
    assert!(
        !warning.contains("mckerracher"),
        "no personal path reaches the warning"
    );
}

#[cfg(unix)]
#[test]
fn a_clean_real_turn_is_ready_without_a_warning() {
    let record = sign_in_over(&captured("success_clean.jsonl"), 0);
    assert_eq!(record.state, SignInState::Ready);
    assert_eq!(record.warning, None);
}

/// The same real stream with the result's exit code changed to 1 fails, and
/// the CLI's exit code is the reason. A hook error is never a success by itself.
#[cfg(unix)]
#[test]
fn a_nonzero_exit_after_a_hook_error_is_an_error_with_the_cli_reason() {
    let record = sign_in_over(&captured("nonzero_exit_after_hook_error.jsonl"), 1);
    assert_eq!(record.state, SignInState::Error);
    let reason = record.reason.expect("a reason");
    assert!(reason.contains("exit code 1"), "{reason}");
}

#[test]
fn the_parser_reads_the_captured_hook_error_as_a_warning_on_a_completed_turn() {
    let mut parser = CopilotParser::new();
    for line in captured("hook_error_then_success.jsonl").lines() {
        parser.feed(line);
    }
    assert!(parser.completed());
    assert!(parser
        .warning()
        .expect("a warning")
        .starts_with("agentStop hook"));
    assert_eq!(parser.failure(), None);
}

#[cfg(unix)]
#[tokio::test]
async fn a_check_that_runs_too_long_is_cancelled_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(dir.path(), "copilot", "1.0.109", "sleep 30\n");
    let started = Instant::now();
    let record = CopilotAdapter::new()
        .with_program(&program)
        .sign_in_check(dir.path(), Duration::from_millis(400), false)
        .await;
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(record.state, SignInState::Error);
    assert!(record.reason.unwrap().contains("timed out"));
}

#[test]
fn an_unclassified_failure_is_an_error_with_its_text() {
    let record = classify_failure("socket hang up", 9);
    assert_eq!(record.state, SignInState::Error);
    assert_eq!(record.reason.as_deref(), Some("socket hang up"));
    assert_eq!(record.checked_at_ms, Some(9));
}

/// Runs a future to completion on a fresh runtime, for the synchronous
/// `CopilotSignIn::check` closure.
fn futures_block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

// --- replay through the supervisor -------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn a_replayed_bogus_model_run_exits_failed_with_the_message() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(
        dir.path(),
        "copilot",
        "1.0.109",
        &replay_body(&fixture("bogus_model.jsonl"), 1),
    );
    let run = CopilotRun::create(dir.path()).unwrap();
    let cwd = run.cwd();
    let request = AgentLaunchRequest {
        program,
        args: CopilotAdapter::conversation_args(
            PermissionLevel::Restricted,
            "hi",
            &run.usage_file(),
            &run.cwd(),
            false,
        )
        .unwrap(),
        working_dir: cwd,
    };
    let mut handle = CopilotAdapter::new()
        .spawn_run(request, Some(run))
        .await
        .unwrap();
    match handle.wait().await.unwrap() {
        AgentExit::Failed { code, reason } => {
            assert_eq!(code, Some(1));
            assert!(reason.unwrap().contains("is not available"));
        }
        other => panic!("expected a failed exit, got {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_a_running_turn_reports_cancelled_not_completed() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_copilot(
        dir.path(),
        "copilot",
        "1.0.109",
        "printf '%s\\n' '{\"type\":\"session.start\",\"data\":{\"sessionId\":\"11111111-2222-4333-8444-555555555555\"}}'\nsleep 30\n",
    );
    let run = CopilotRun::create(dir.path()).unwrap();
    let request = AgentLaunchRequest {
        program,
        args: CopilotAdapter::conversation_args(
            PermissionLevel::Restricted,
            "hi",
            &run.usage_file(),
            &run.cwd(),
            false,
        )
        .unwrap(),
        working_dir: run.cwd(),
    };
    let mut handle = CopilotAdapter::new()
        .spawn_run(request, Some(run))
        .await
        .unwrap();
    assert!(matches!(
        handle.next_event().await.unwrap(),
        Some(AgentEvent::SessionStarted { .. })
    ));
    handle.cancel().await.unwrap();
    assert!(matches!(
        handle.wait().await.unwrap(),
        AgentExit::Cancelled { .. }
    ));
}
