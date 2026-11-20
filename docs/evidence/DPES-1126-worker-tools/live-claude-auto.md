# DPES-1126 live check: Claude Code at Auto

Claude Code 2.1.311, `--model haiku`. This runs the exact Auto flag set the daemon builds (`claude_task_argv_per_level_is_exact` in `executor_tests.rs`) in a real linked worktree of a scratch repo. It does NOT go through the daemon, so it proves the flag set and the git grants, not the launch path, staging or preflight.

Setup: branch `mcao/live`, `--add-dir` for a status directory, the slot gitdir, `<common>/objects`, `<common>/refs/heads/mcao` and `<common>/logs/refs/heads/mcao`. Environment: the task git guard (no credential helper, hooks off, identity `Agent Orchestrator <agent-orch@localhost>`).

| Step | Result |
|---|---|
| Create `hello.md` | done |
| Run a shell command (`test -f hello.md && echo tests-pass`) | `tests-pass` |
| `git add` and `git commit` | commit `031f613` landed on `mcao/live` |
| `git log` | shows the commit on top of base |
| `git remote -v` | denied |
| `git push origin mcao/live` | denied |
| `gh --version` | denied |
| `touch ../outside-the-worktree.txt` | **succeeded** |

`permission_denials` in the `result` event listed the three denied commands. It is populated when a tool is denied. It is empty only when a tool is absent (Restricted), as recorded in `phase0-repro.md` (d).

## What this shows
- At Auto, Claude can read, run shell commands, and commit in its slot. Push, remote and `gh` are refused by the deny rules.
- **Writes outside the worktree are not blocked at Claude Auto.** `--restricted` confines the file tools, and Bash is judged only by the classifier. This is the documented classifier-only boundary, and the design keeps it. It is now observed, not assumed.
- `permission_denials` is usable for a denial notice at Auto. That notice is not implemented here.

## Not run live
- Codex: blocked by the workspace spend cap.
- Copilot: denial rules and path checks were probed live (see `phase0-repro.md`), but the create-file, test and commit flow was not run.
- The daemon launch path (staging, preflight, task card) was not run against a signed-in agent.
