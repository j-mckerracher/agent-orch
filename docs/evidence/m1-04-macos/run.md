# M1-04 macOS E2E evidence

Recorded: 2026-11-07T22:45:42-07:00

Codex executable: `/opt/homebrew/bin/codex`

`/opt/homebrew/bin/codex --version` exited 0 and printed:

```text
codex-cli 0.167.0
```

`/opt/homebrew/bin/codex login status` exited 0 and printed:

```text
Logged in using ChatGPT
```

The fixture lived under `/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif`. Its primary checkout started at `f4e6d40985f4801fce1033a71552b5733522e2d7` with one tracked file, `README.md`.

## Initial run and correction

The first run exposed an incompatible Codex flag pair. Codex 0.167.0 rejects `--sandbox` with `--approve-for-me`. The fix removed the explicit sandbox flag because `--approve-for-me` already selects the workspace-write sandbox.

Command:

```text
/Users/mckerracher.joshua/Code/agent-orch-worktrees/tasks/m1-04-cli-workflow/target/debug/orchestrator-cli --data-dir /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data run --repo /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/repo --task-id m1-04-e2e --objective 'Append exactly one line, "M1-04 smoke test passed.", to README.md. Do not modify any other file.' --base-ref main --agent-program /opt/homebrew/bin/codex
```

Exit status: `1`.

Stdout:

```text
task: m1-04-e2e
worktree: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/worktrees/m1-04-e2e
branch: orchestrator/m1-04-e2e
Codex JSONL: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/cli/m1-04-e2e/codex.jsonl
```

Stderr:

```text
agent-orch: Codex failed for task m1-04-e2e: Codex exited without a completed turn; inspect worktree /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/worktrees/m1-04-e2e and logs /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/cli/m1-04-e2e/codex.jsonl / /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/cli/m1-04-e2e/codex.stderr
```

Codex stderr capture:

```text
error: the argument '--sandbox <SANDBOX_MODE>' cannot be used with '--approve-for-me'

Usage: codex exec [OPTIONS] [PROMPT]
       codex exec [OPTIONS] <COMMAND> [ARGS]

For more information, try '--help'.
```

The first run produced no JSONL events. It did not modify the primary checkout.

## Successful run

The fixed CLI ran Codex in a fresh task worktree with the same disposable repository and data directory. The task ID was `m1-04-e2e-success`.

Command:

```text
/Users/mckerracher.joshua/Code/agent-orch-worktrees/tasks/m1-04-cli-workflow/target/debug/orchestrator-cli --data-dir /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data run --repo /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/repo --task-id m1-04-e2e-success --objective 'Append exactly one line, "M1-04 smoke test passed.", to README.md. Do not modify any other file.' --base-ref main --agent-program /opt/homebrew/bin/codex
```

Exit status: `0`.

Stdout:

```text
task: m1-04-e2e-success
worktree: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/worktrees/m1-04-e2e-success
branch: orchestrator/m1-04-e2e-success
Codex JSONL: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/cli/m1-04-e2e-success/codex.jsonl
Codex session: 01a1b9af-3ce2-7812-9dfa-eb556e51fdd1
I’ll check the README ending and workspace changes, then append the requested line only.
Appended `M1-04 smoke test passed.` as one line to [README.md](/private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/worktrees/m1-04-e2e-success/README.md). No other file was modified.
turn: Success
candidate: 5242a3142c3ddcfb3edac8f25b0a1801677aa3b8
status: Verifying (candidate preserved for review)
exit code: 0
```

Stderr was empty. The CLI captured Codex's stderr as `Reading additional input from stdin...`.

The task record showed state `Verifying`, attempt `CandidateReady`, session `Exited`, and exit code `0`. Candidate commit: `5242a3142c3ddcfb3edac8f25b0a1801677aa3b8`.

## Inspect

Command:

```text
/Users/mckerracher.joshua/Code/agent-orch-worktrees/tasks/m1-04-cli-workflow/target/debug/orchestrator-cli --data-dir /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data inspect m1-04-e2e-success
```

Exit status: `0`. Stderr was empty.

Stdout (tab bytes are shown as `\t`):

```text
task: m1-04-e2e-success
status: Verifying
agent: codex
repository: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/repo
objective: Append exactly one line, "M1-04 smoke test passed.", to README.md. Do not modify any other file.
attempts:
  1\tm1-04-e2e-success-attempt-1\tCandidateReady
sessions:
  m1-04-e2e-success-session-1\tcodex\tExited
worktree: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/worktrees/m1-04-e2e-success
branch: orchestrator/m1-04-e2e-success
base: main (f4e6d40985f4801fce1033a71552b5733522e2d7)
agent PID: 76784
exit code: 0
agent outcome: completed
candidate commit: 5242a3142c3ddcfb3edac8f25b0a1801677aa3b8
Codex JSONL: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/cli/m1-04-e2e-success/codex.jsonl
Codex stderr: /private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/cli/m1-04-e2e-success/codex.stderr
```

## Diff

Command:

```text
/Users/mckerracher.joshua/Code/agent-orch-worktrees/tasks/m1-04-cli-workflow/target/debug/orchestrator-cli --data-dir /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data diff m1-04-e2e-success
```

Exit status: `0`. Stderr was empty.

Stdout:

```diff
diff --git a/README.md b/README.md
index e1cbf33..d65fe15 100644
--- a/README.md
+++ b/README.md
@@ -1 +1,2 @@
 M1-04 disposable repository.
+M1-04 smoke test passed.
```

## Raw Codex JSONL events

```jsonl
{"type":"thread.started","thread_id":"01a1b9af-3ce2-7812-9dfa-eb556e51fdd1"}
{"type":"item.completed","item":{"id":"item_0","type":"error","message":"`[features].codex_hooks` is deprecated. Use `[features].hooks` instead. (Enable it with `--enable hooks` or `[features].hooks` in config.toml. See https://developers.openai.com/codex/config-basic#feature-flags for details.)"}}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"I’ll check the README ending and workspace changes, then append the requested line only."}}
{"type":"item.started","item":{"id":"item_2","type":"command_execution","command":"/bin/zsh -lc 'rtk git status --short && rtk read README.md --tail-lines 5'","aggregated_output":"","exit_code":null,"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_2","type":"command_execution","command":"/bin/zsh -lc 'rtk git status --short && rtk read README.md --tail-lines 5'","aggregated_output":"M1-04 disposable repository.\n","exit_code":0,"status":"completed"}}
{"type":"item.started","item":{"id":"item_3","type":"command_execution","command":"/bin/zsh -lc 'tail -c 1 README.md | od -An -t x1'","aggregated_output":"","exit_code":null,"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_3","type":"command_execution","command":"/bin/zsh -lc 'tail -c 1 README.md | od -An -t x1'","aggregated_output":"           0a                                                            \n\n","exit_code":0,"status":"completed"}}
{"type":"item.started","item":{"id":"item_4","type":"command_execution","command":"/bin/zsh -lc \"printf '%s\\\\n' 'M1-04 smoke test passed.' >> README.md && git diff -- README.md && git status --short\"","aggregated_output":"","exit_code":null,"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_4","type":"command_execution","command":"/bin/zsh -lc \"printf '%s\\\\n' 'M1-04 smoke test passed.' >> README.md && git diff -- README.md && git status --short\"","aggregated_output":"diff --git a/README.md b/README.md\nindex e1cbf33..d65fe15 100644\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n M1-04 disposable repository.\n+M1-04 smoke test passed.\n M README.md\n","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"item_5","type":"agent_message","text":"Appended `M1-04 smoke test passed.` as one line to [README.md](/private/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/agent-orch-m1-04-macos.fL7kif/data/worktrees/m1-04-e2e-success/README.md). No other file was modified."}}
{"type":"turn.completed","usage":{"input_tokens":96936,"cached_input_tokens":72192,"cache_write_input_tokens":0,"output_tokens":471,"reasoning_output_tokens":203}}
```

The JSONL includes Codex's deprecation warning for `features.codex_hooks`. It did not affect the completed turn.

## Checkout invariants

Before the successful run, the disposable primary checkout was at `f4e6d40985f4801fce1033a71552b5733522e2d7` and `git status --porcelain` was empty. After the run, the checkout stayed at that commit, its status remained empty, and `README.md` still contained only `M1-04 disposable repository.`.

The Agent Orchestrator worktree had empty `git status --short` before the successful run and after it. The task branch and candidate commit remained in the disposable repository's linked worktree.
