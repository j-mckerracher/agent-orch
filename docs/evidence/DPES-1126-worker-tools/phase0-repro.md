# DPES-1126 Phase 0: root causes and probes

Base: origin/main 69e46e8a. CLIs: Claude Code 2.1.311, codex-cli 0.171.0, Copilot CLI 1.0.114.

## Root causes

1. **Tasks start at Restricted.** Nothing that builds a task contract sends a level, so `api.rs:961` (`unwrap_or_default()`) falls to the enum default, Restricted.
   - Desktop task form: `submission.ts` builds no `permission`; `TaskSubmission` has no such field.
   - `ProposeExec` (`api_actions.rs:1816`) and revise (`api_actions.rs:1704`) set `permission: None`. This covers coordinators, workstreams, Global and "act on its own".
   - `workstreams.add_task` and `orchestrator-cli/src/workflow.rs` pass the default through.
   - The chat level does not leak into tasks. It is stored in localStorage and sent only on `conversations.send`.
2. **Restricted removes the shell by design.** Claude: `--tools Read,Edit,Write --restricted`. Copilot: `--deny-tool shell`. Codex: `--sandbox workspace-write`, no network. This is the incident text.
3. **Git in a slot is outside every grant.** A slot's `.git` is a file pointing to `<main>/.git/worktrees/<name>`. `git commit` writes the slot gitdir, `<common>/objects` and `<common>/refs/heads`. No launch flag grants these.
4. **A commit by the agent fails the task.** `commit_candidate` reports "no changes" when the tree is clean even if HEAD moved. The brief says "Do not commit".
5. **Required inputs are unreachable.** The pstack plugin lives in `~/.claude/plugins/cache/...` and is never granted. A missing plugin only warns.
6. **No honesty layer.** The brief never states the level or tools. Task pages never show the level. No preflight exists.

Daemon log, 18 `agent_launched` events: 17 claude-code restricted, 1 codex restricted. No task has ever launched above Restricted.

## Live probes

### (a) Codex workspace-write and a worktree's gitdir (PASS)
Linked worktree `wt` of repo `main`, run with `codex sandbox -c sandbox_mode="workspace-write"`.

| Case | Result |
|---|---|
| `git commit`, no grants | `fatal: Unable to create '.../.git/worktrees/wt/index.lock': Operation not permitted` |
| `git commit`, writable_roots = gitdir, `objects`, `refs/heads`, `logs/refs/heads` | commit succeeds |
| With those grants, write common `config` | denied |
| With those grants, write `hooks/pre-commit` | denied |
| With those grants, write `info/exclude` | denied |
| With those grants, write `refs/tags/t` | denied |

Codex honors writable_roots for a linked worktree's gitdir. A minimal grant allows commit and keeps config, hooks, info and tags protected. Note: an earlier run placed the probe under `$TMPDIR`, which workspace-write grants by default. That run looked writable for the wrong reason, so the probe was repeated under `~/.cache`.

### (d) Claude `result.permission_denials` (CHANGES THE DESIGN)
Claude with `--restricted --tools Read,Edit,Write`, asked to run `echo hi`. The agent said it has no Bash tool. The `result` event had `permission_denials: []`.

A tool that is absent is not a denial. The "worker lacked tools" signal cannot rely on `permission_denials` at Restricted. It must come from the daemon's own knowledge of the level's toolset. `permission_denials` and `tool_result` errors stay as extra evidence at Extended and Auto.

## Not yet run
- (b) Copilot path checks on shell writes.
- (c) `codex exec resume -c ...` honoring sandbox, roots and network, and the config key behind `--approve-for-me`.
- Real launches of the "Add an empty markdown file" and docs-audit tasks. Both need a running daemon and a signed-in session. These run in the live check after implementation.
