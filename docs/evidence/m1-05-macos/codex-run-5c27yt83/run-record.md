# Real Codex isolation run

Updated: 2026-11-07T23:20:21-07:00.

## Result

The CLI ran Codex 0.167.0 against a disposable dirty primary repository. Codex changed only `sample.txt` in the task worktree to `codex candidate` with no trailing newline. The CLI committed the candidate and exited with code 0.

The primary checkout HEAD stayed `6355f092d021e5d46bdbf16716929c15491e5585`. Its `git status --porcelain` output and both file hashes match before and after. See `primary-before.txt` and `primary-after.txt`.

## Command and outputs

`command.txt` contains the raw `orchestrator-cli run` invocation. The command used `/opt/homebrew/bin/codex` and task ID `m105-codex-isolation`.

`codex-version.txt` records `/opt/homebrew/bin/codex --version`. `stdout.txt`, `stderr.txt`, and `exit-status.txt` contain the raw CLI outputs and status. `data/cli/m105-codex-isolation/codex.jsonl` contains Codex JSONL events.

The run used the CLI's existing `--approve-for-me` mode. Codex help states that this mode uses the `workspace-write` sandbox. No full-access or bypass-approval flag was used.

Codex JSONL includes a warning that `[features].codex_hooks` is deprecated. The run still completed successfully.

## Scope

`repo/` is the disposable primary checkout. `data/worktrees/m105-codex-isolation/` is the task worktree. The run directory was created with `tempfile.mkdtemp` under `docs/evidence/m1-05-macos/`.
