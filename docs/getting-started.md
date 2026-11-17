# Getting started

This page takes you from a fresh install to a first chat and a first task. You need MCAO [installed](install.md) and at least one signed-in agent CLI.

## 1. Open the app

MCAO opens on the Orchestrator Workspace. The app starts its local daemon on demand, so there is nothing to launch separately. If the connection strip does not show Connected, see [Troubleshooting](troubleshooting.md#the-app-does-not-connect).

## 2. Register a repository

Open Workstreams in the sidebar and click Manage projects. Enter the absolute path of a Git repository, such as `/path/to/repository`, and click Register.

## 3. Chat with an agent

In the Orchestrator Workspace, pick a target (Global, or a project) and type a message. The composer also has a model picker and a permission level control. See [Chat](chat.md).

## 4. Submit a task

Open the project, scroll to New task, write an objective, choose an agent (`codex` or `claude-code`), and click Submit. The task is queued and starts when the scheduler has capacity. See [Tasks](tasks.md).

## 5. Review the result

A finished task keeps its work in a separate worktree and branch. Review it in Review & Decisions, or in the project's task page. The primary checkout is never modified while a task runs.

## Next steps

- [Choose a permission level](permission-levels.md)
- [Set project defaults](project-defaults.md)
- [Tune machine load limits](settings.md)
