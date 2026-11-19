# Tasks

A task is one unit of agent work that runs in its own Git worktree and branch. Your primary checkout is not changed while it runs.

## Submit a task

Open a project and use the New task form. Fill in Objective, pick the Agent (`codex` or `claude-code`), and click Submit. The task is queued and starts when the scheduler has capacity.

## Form fields

| Field | Options | Meaning |
| --- | --- | --- |
| Kind | Implement, Investigate | Implement writes code in the worktree. Investigate is read-only research that produces a report. |
| Verification | None, Commands | With Commands, one command per line runs against the finished work. |
| Delivery | ReportOnly, LocalReview, PullRequest | ReportOnly never merges code. LocalReview waits for your authorization. PullRequest pushes a branch and opens a pull request. |
| Autonomy | WorkspaceWrites, ApprovalRequired | WorkspaceWrites lets the agent write inside the worktree. ApprovalRequired needs approval for consequential actions. |
| Max retries, Max repairs | numbers | Bounds on re-running a failed task and repairing a failed verification. |

Parent task and Dependencies are optional task IDs. A task waits until its dependencies finish.

## Tasks from the global orchestrator

A task the global orchestrator proposes joins the "Global tasks" workstream, which is made the first time it is needed. It joins another workstream only when you name that workstream. If it is not clear which project a task belongs to, the task runs in the [home directory](settings.md#home-directory) when you have set one. A task in the home directory runs in that folder itself, not in a worktree, and only reports.

## Task states

A task moves through Created, Queued, Running, and Verifying. It ends as Succeeded, Failed, VerificationFailed, or Canceled. Verifying runs the checks on the candidate and settles to Succeeded or VerificationFailed. Canceling a Verifying task ends it as Canceled at once.

A running task can also go to Waiting (shown as "Waiting for a reply"). Its agent stopped to ask for a decision only you can make, or reported that it is blocked. No agent runs while a task waits, so it uses no tokens and takes no worker slot, but its worktree stays reserved. A decision appears under Needs your decision in Review & Decisions. When you answer it, or when the orchestrator sends the task a message that you approve, the agent continues in the same session, in the same worktree, at the same permission level. Canceling a Waiting task ends it as Canceled at once. A Waiting task cannot be archived.

The agent reports these events by adding lines to a status file that MCAO keeps outside the worktree. A line that says the work is done is only a claim: MCAO still commits the changes and runs the checks. A line that says the agent failed ends the attempt as Failed. A Codex task at the Auto level cannot be continued at the same level, so when it asks for a decision it ends as Failed with that reason instead of waiting. Retry it with the answer added.

## Machine load

New tasks wait while the machine is busy, and running tasks are never stopped. At most two tasks run at once, and at most two per project. Change the load limits in [Settings](settings.md#machine-load-limits).

## Archive finished tasks

A task that Succeeded, Failed, VerificationFailed, or Canceled can be archived. Archiving hides it from the task lists and from its workstream, and keeps its attempts, sessions, and timeline. Archive one task from its page or from the workstream table, or archive every finished task at once from the Workstreams view. Archived tasks are not restored from the desktop app. A task that is still queued or running cannot be archived.

## Review and retry

Open the task to see attempts, sessions, and the timeline. Click Review candidate when the task has a pending approval. A Failed or VerificationFailed task can be retried from its page.

## Workflow agent

Every task attempt runs as the pstack `workflow-agent` (Codex, Claude Code and GitHub Copilot, at every permission level). MCAO finds the plugin under `~/.claude/plugins`. If it is missing, the attempt records a warning and runs without it.

## Where the work lives

Task worktrees are in `worktrees/` inside the [data directory](configuration.md#data-directory). Task branches start with a prefix, `moa/` by default. See [Project defaults](project-defaults.md#branch-prefix).

## Run a task from the terminal

The [command-line client](cli.md) runs one Codex task without the desktop app.
