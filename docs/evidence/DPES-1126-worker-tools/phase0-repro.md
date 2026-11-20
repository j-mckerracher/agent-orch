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

### (b) Copilot shell writes: path-checked against `--add-dir`, not `write(...)`
Copilot CLI 1.0.114, `--model claude-sonnet-5.5`. The default `claude-opus-4.8` is not available on this account. Each row runs `touch <outside>/<file>` through `shell(touch)`.

| Grant | Outside file written |
|---|---|
| `--add-dir <outside>` only | yes |
| `--add-dir <outside>` + `write(<outside>/**)` (the status shape) | yes |
| `write(<outside>/**)` only | no, denied (no interactive response) |
| `write(<allowed>/**)` only, target outside | no, denied |
| `shell(python3)`, `python3 -c open(...)`, no `--add-dir` | yes, written |
| Auto shape (`--assisted-approval`, read, `write(<wt>/**)`, denies) | no, the judge refused |

So a `write(...)` rule does not constrain a shell write. `--add-dir` does, by a check on the command text. An interpreter gets past that check. Copilot command sandboxing is off by default (`copilot help sandbox`), so Copilot Auto's boundary is the judge plus rules, not the OS. Recorded as a risk.

### (c) Codex resume and `--approve-for-me`: OPEN, offline evidence only, Codex not run live
Status: open. The live Codex account is blocked, so this item stays open. The PR states "Codex not run live".
- A live `codex exec` was refused: "You hit your spend cap set by the owner of your workspace." Live resume needs the cap raised.
- `--approve-for-me` expands to `approvals_reviewer="auto_review"`, `approval_policy="on-request"`, `sandbox_mode="workspace-write"`. The Codex 0.171.0 binary contains that exact literal.
- Offline, `codex debug prompt-input` with an empty `CODEX_HOME` renders `read-only` with user-approved escalation for no overrides. The three keys above render `workspace-write` with "`approvals_reviewer` is `auto_review`". `approval_policy` does not render as text, so that key rests on the literal.
- `sandbox_workspace_write.writable_roots=[...]` and `sandbox_workspace_write.network_access=true` also render: "Network access is enabled", and the roots appear.
- **Not conclusive, so Auto resume stays refused.** The offline render shows the keys are the right ones at launch. It does not show that `exec resume -c ...` applies them to a resumed session. Auto returns to the fail-closed refusal in `resume_refusal`, with the message "its `-c` equivalent is not verified live yet". Lifting it needs one live `exec` plus `exec resume` run with the keys.

### Deny syntax (live)
- Claude Auto `--disallowedTools`: `Bash(git push *)`, `Bash(git remote *)`, `Bash(git config *)` and `Bash(gh *)` are each refused. The same `git remote -v` runs with no deny list, so the refusal came from the rule. `permission_denials` lists the refused commands, which supports (d).
- Copilot Auto `--deny-tool` (with `--allow-tool shell(git)`): `shell(git remote)`, `shell(git config)`, `shell(git credential)` and `shell(git push)` each refuse. `shell(gh)` does not refuse `gh --version` or `gh pr list`, because a bare pattern does not match there. `shell(gh:*)` refuses both, so Copilot uses `shell(gh:*)`.

### Codex's cmux shim injects hook trust bypass
The `codex` on this PATH is a cmux shim. It adds `--dangerously-bypass-hook-trust` to Codex invocations, and the probe's `exec` logged "`--dangerously-bypass-hook-trust` is enabled". A daemon that resolves `codex` through a cmux PATH inherits this. Not changed here; see open risks.

## Per-agent flag table (after Lane B)
`G` = the write directories: the status directory, then the slot's four git directories from `SlotDirs.grants` (its gitdir, `objects`, `refs/heads/<prefix>`, `logs/refs/heads/<prefix>`). The daemon's `RunChannel::write_dirs` builds the list, and every level gets it.

| Agent | Level | First launch | Resume |
|---|---|---|---|
| Claude | Restricted | `--tools Read,Edit,Write --restricted --permission-mode dontAsk`; `--add-dir G` | same, `--resume <id>` |
| Claude | Extended | `--tools …,Bash` with allow-list `Bash(git status/diff/log/show *)`; `--add-dir G` | same |
| Claude | Auto | `--permission-mode auto --restricted --tools …,Bash`; `--disallowedTools` `Bash(git push *)`, `Bash(git * push *)`, `Bash(git remote *)`, `Bash(git credential *)`, `Bash(git config *)`, `Bash(gh *)`; `--add-dir G` | same, `--resume <id>` |
| Claude | Full | `--permission-mode bypassPermissions`; `--add-dir G` | same |
| Codex | Restricted | `--sandbox workspace-write --ignore-user-config`; `--add-dir G` | `-c sandbox_mode`, `-c sandbox_workspace_write.writable_roots=[G]` |
| Codex | Auto | `--sandbox workspace-write --approve-for-me --ignore-user-config -c sandbox_workspace_write.network_access=true`; `--add-dir G` | refused (`resume_refusal`), until a live resume confirms the `-c` keys |
| Codex | Full | no sandbox flag; `--add-dir G` | `-c sandbox_workspace_write.writable_roots=[G]` (restores the status grant; replaces the user's own roots) |
| Codex | Extended | no mapping, refused | refused |
| Copilot | Restricted | `--deny-tool shell --deny-tool url --deny-tool memory`; per dir `--add-dir d --allow-tool write(d/**)` for G | same, `--resume=<id>` |
| Copilot | Extended | shell limited to `git status/diff/log/show`; per dir grants | same |
| Copilot | Auto | `--assisted-approval`; `--deny-tool shell(git push)`, `shell(git remote)`, `shell(git credential)`, `shell(git config)`, `shell(gh:*)`; per dir grants | same, `--resume=<id>` |
| Copilot | Full | `--allow-all`; per dir `--add-dir d` only | same |

## Still not run
- Real launches of the "Add an empty markdown file" and docs-audit tasks. Both need a running daemon and a signed-in session. These run in the live check after implementation.
- Live Codex launch and resume (blocked by the spend cap). This keeps probe (c) open.

## Open risks
- **Codex Auto network plus readable credentials.** `workspace-write` restricts writes, not reads. An Auto Codex worker with network on can read `~/.ssh` or `~/.codex/auth.json` and send them out. This is the accepted widening, and it needs an explicit decision. Not live-verified.
- **Copilot shell is not an OS boundary.** Sandboxing is off by default, and the path check is heuristic (see (b)). Auto relies on the judge and the deny rules.
- **Commit identity: resolved.** Auto denies `git config`, so the agent cannot set an identity. The task guard sets `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME` and `GIT_COMMITTER_EMAIL` to `orchestrator_git::AGENT_NAME` and `AGENT_EMAIL`. These are the same constants `commit_candidate` commits with, so agent and daemon commits match.
- **Full Codex resume** replaces the user's own `writable_roots` with the task's roots.
- **Env guard: resolved, task-only.** `orchestrator_git::TASK_GIT_GUARD_ENV` (`GIT_TERMINAL_PROMPT=0`, `credential.helper` empty, `core.hooksPath=/dev/null`, and the identity above) is set by `ChildSupervisor::launch` only when `LaunchRequest::git_guard` is true. The daemon sets `AgentLaunchRequest::git_guard` to `Job::git_guard()`, which is true only for a task below Full. Chat's launches pass `false`, and so does every probe. A Full task gets none either. The four adapter `spawn` sites that serve tasks pass the request's flag through.
- **Codex's `--add-dir` for git dirs at launch** uses the same mechanism as the status directory. It is not live-verified.
