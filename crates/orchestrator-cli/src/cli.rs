//! Command grammar and dispatch for the standalone CLI. Boundary arguments
//! are parsed here; workflow invariants stay in `workflow`. Updated: 2026-11-07T22:31:40-07:00.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::workflow;

#[derive(Debug, Parser)]
#[command(
    name = "agent-orch",
    version,
    about = "Run and inspect isolated agent tasks"
)]
pub struct Cli {
    /// Store and task-worktree root. Defaults to the user's local data directory.
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run one Codex task in an isolated Git worktree.
    Run {
        /// Git repository to work in.
        #[arg(long)]
        repo: PathBuf,
        /// Stable task identifier. A UUID is generated when omitted.
        #[arg(long)]
        task_id: Option<String>,
        /// Explicit work request passed to Codex.
        #[arg(long)]
        objective: String,
        /// Commit or branch from which to create the candidate worktree.
        #[arg(long, default_value = "main")]
        base_ref: String,
        /// Codex executable path or name resolved through PATH.
        #[arg(long, default_value = "codex")]
        agent_program: PathBuf,
    },
    /// List stored task IDs and states.
    Tasks,
    /// Show one task's attempts, sessions, worktree, and result.
    Inspect {
        /// Task identifier.
        task_id: String,
    },
    /// Request owner-routed cancellation of a running task's agent.
    Cancel {
        /// Task identifier.
        task_id: String,
    },
    /// Manually release one unresolved owner after confirming its process tree exited.
    RecoverOwner {
        /// Task identifier.
        task_id: String,
        /// Exact attempt identifier shown by `inspect`.
        #[arg(long)]
        attempt_id: String,
        /// Exact session identifier shown by `inspect`.
        #[arg(long)]
        session_id: String,
        /// Exact owner generation shown by `inspect`.
        #[arg(long)]
        generation: String,
        /// Confirm that both the run owner and its managed process tree have exited.
        #[arg(long)]
        confirm_processes_exited: bool,
        /// Reason for the operator's process-exit confirmation.
        #[arg(long)]
        reason: String,
    },
    /// Show candidate changes relative to the task's base commit.
    Diff {
        /// Task identifier.
        task_id: String,
    },
    /// Remove a clean worktree only after its commit is reachable from base.
    Cleanup {
        /// Task identifier.
        task_id: String,
    },
}

pub async fn execute(cli: Cli) -> Result<i32, String> {
    let data_dir = workflow::data_dir(cli.data_dir)?;
    match cli.command {
        Commands::Run {
            repo,
            task_id,
            objective,
            base_ref,
            agent_program,
        } => {
            workflow::run_task(
                &data_dir,
                &repo,
                task_id.as_deref(),
                &objective,
                &base_ref,
                &agent_program,
            )
            .await
        }
        Commands::Tasks => {
            workflow::list_tasks(&data_dir)?;
            Ok(0)
        }
        Commands::Inspect { task_id } => {
            workflow::inspect_task(&data_dir, &task_id)?;
            Ok(0)
        }
        Commands::Cancel { task_id } => {
            workflow::cancel_task(&data_dir, &task_id)?;
            Ok(0)
        }
        Commands::RecoverOwner {
            task_id,
            attempt_id,
            session_id,
            generation,
            confirm_processes_exited,
            reason,
        } => {
            workflow::recover_cancellation_owner(
                &data_dir,
                &task_id,
                &attempt_id,
                &session_id,
                &generation,
                confirm_processes_exited,
                &reason,
            )?;
            Ok(0)
        }
        Commands::Diff { task_id } => {
            workflow::show_diff(&data_dir, &task_id)?;
            Ok(0)
        }
        Commands::Cleanup { task_id } => {
            workflow::cleanup_task(&data_dir, &task_id)?;
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_command_and_global_data_directory() {
        let common = ["agent-orch", "--data-dir", "/tmp/agent-orch-data"];
        let commands = [
            vec!["tasks"],
            vec!["inspect", "task-1"],
            vec!["cancel", "task-1"],
            vec![
                "recover-owner",
                "task-1",
                "--attempt-id",
                "attempt-1",
                "--session-id",
                "session-1",
                "--generation",
                "0123456789abcdef0123456789abcdef",
                "--confirm-processes-exited",
                "--reason",
                "verified process tree exited",
            ],
            vec!["diff", "task-1"],
            vec!["cleanup", "task-1"],
        ];
        for command in commands {
            let args = common.into_iter().chain(command).collect::<Vec<_>>();
            assert!(Cli::try_parse_from(args).is_ok());
        }
        assert!(Cli::try_parse_from([
            "agent-orch",
            "run",
            "--repo",
            "/tmp/repo",
            "--task-id",
            "task-1",
            "--objective",
            "edit file",
        ])
        .is_ok());
    }

    #[test]
    fn rejects_run_without_repository_or_objective() {
        assert!(Cli::try_parse_from(["agent-orch", "run"]).is_err());
        assert!(Cli::try_parse_from(["agent-orch", "run", "--repo", "/tmp/repo",]).is_err());
    }
}
