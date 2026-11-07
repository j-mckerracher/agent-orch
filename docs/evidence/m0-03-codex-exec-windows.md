# M0-03 Evidence: Codex CLI one-shot execution on Windows

Recorded: 2026-11-07T11:04:00-07:00 (America/Denver). Platform: Windows 11 Education, build 26200, x64.
Task: M0-03 "Verify Codex execution protocol" — one-shot execution and structured output.

## Verdict

PASS for Windows: `codex exec` one-shot runs, emits parseable JSONL events on stdout, writes the
last agent message to a file, and exits 0 on success / nonzero on failure. macOS: no native
evidence available on this host; blocker recorded below. No adapter code was written (out of
scope).

## Installations found

| Binary | Version | Auth | One-shot result |
|---|---|---|---|
| `C:\Users\jmckerra\AppData\Local\OpenAI\Codex\bin\14b838c1d074ff01\codex.exe` (native desktop) | codex-cli 0.167.0-alpha.3.2 | `codex login status` = "Logged in using ChatGPT" | Success, exit 0, ~5 s |
| `C:\Users\jmckerra\AppData\Roaming\npm\codex` / `codex.cmd` (PATH wrapper) | codex-cli 0.20.0 | same ChatGPT login | Fails: config-pinned model `gpt-6.1-terra` rejected by backend with HTTP 400; 5 retries, then `{"id":"1","msg":{"type":"error",...}}` |

The old wrapper reads `~\.codex\config.toml`, which pins `model = "gpt-6.1-terra"`. The observed attempt was rejected for that account and model:
`"The 'gpt-6.1-terra' model is not supported when using Codex with a ChatGPT account."` (HTTP 400,
`model_not_found`). The response identifies an account/model-coverage condition. The 0.167.0 native executable succeeded without a `-m` override; the cause of the difference was not established.

## Executed commands and results

All commands run from `C:\Users\jmckerra\Code\agent-orch-m0-03-priority2`.
`CODEX` below is the native path above (quoted on Windows).

### 1. Version and auth

```
"C:/Users/jmckerra/AppData/Local/OpenAI/Codex/bin/14b838c1d074ff01/codex.exe" --version
# codex-cli 0.167.0-alpha.3.2
"C:/Users/jmckerra/AppData/Roaming/npm/codex.cmd" --version
# codex-cli 0.20.0
"C:/Users/jmckerra/AppData/Local/OpenAI/Codex/bin/14b838c1d074ff01/codex.exe" login status
# Logged in using ChatGPT
```

### 2. One-shot, JSONL structured output (the core probe)

```
"$CODEX" exec --json --skip-git-repo-check --sandbox read-only "Reply exactly PONG"
```

Exit: `0`. Wall time: 4.77 s. Stdout (one JSON object per line, `raw/one-shot-stdout.jsonl`):

```
{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"PONG"}}
{"type":"turn.completed","usage":{"input_tokens":22223,"cached_input_tokens":13568,"cache_write_input_tokens":0,"output_tokens":6,"reasoning_output_tokens":0}}
```

Framing: NDJSON on stdout. Event envelope is flat `{type, ...}`. Observed event sequence:
`thread.started` (carries `thread_id`) -> `turn.started` -> one or more `item.completed` (each
`item` has `id` and `type`; agent messages carry `text`) -> `turn.completed` (carries `usage`
token counts). Diagnostics go to stderr only; stdout stayed pure NDJSON in every successful run.
Sanitization note: `thread_id` and token counts are recorded raw; they are session IDs, not
secrets. No credentials appear in any captured file.

### 3. Last-message file (structured result extraction)

```
"$CODEX" exec --json --skip-git-repo-check --sandbox read-only \
  --output-last-message tests/feasibility/m0-03/raw/last-message.txt "Reply exactly PONG"
```

Exit: `0`. `last-message.txt` contains exactly `PONG`. This is the simplest machine-consumable
result channel alongside the JSONL stream.

### 4. Exit behavior, failure case (invalid argument)

```
"$CODEX" exec --sandbox bogus "x"
```

Exit: `2`. Stderr: `error: invalid value 'bogus' for '--sandbox <SANDBOX_MODE>'` with possible
values listed. Argument validation fails fast before any network call.

### 5. Old wrapper failure, recorded (no model guessing)

```
"C:/Users/jmckerra/AppData/Roaming/npm/codex.cmd" exec --json --skip-git-repo-check --sandbox read-only "Reply exactly PONG"
```

Stdout (0.20.0 JSON shape, `raw/old-wrapper-stdout.jsonl` / `raw/old-wrapper2-stdout.jsonl`):

```
{"sandbox":"read-only","provider":"openai","approval":"never","workdir":"...","model":"gpt-6.1-terra"}
{"prompt":"Reply exactly PONG"}
{"id":"1","msg":{"type":"task_started"}}
{"id":"1","msg":{"type":"background_event","message":"stream error: unexpected status 400 Bad Request: ...retrying 1/5 in 204ms…"}}
... (5 background_event retries) ...
{"id":"1","msg":{"type":"error","message":"unexpected status 400 Bad Request: {\"detail\":\"The 'gpt-6.1-terra' model is not supported when using Codex with a ChatGPT account.\",...}"}}
```

Exit code: NOT OBTAINED. Both runs hung. Timeline from the re-run (17:55:21 start): the final
`{"id":"1","msg":{"type":"error",...}}` line landed at ~17:55:26 (about 5 s in), after which the
process produced no further output and did not exit within 80 s. `timeout 80` killed it, giving
shell exit 124 (the timeout's code, not Codex's). The first capture shows the same pattern with
an external `^C`. Observed defect in 0.20.0: after emitting the terminal error event on this
400-retry path the process does not terminate; a supervisor needs a bounded timeout and explicit protocol handling; an error event alone does not establish process exit. Exit 124 is therefore a timeout artifact and
must not be cited as the CLI's own failure code.

Note: 0.20.0 uses a *different* JSONL envelope (`{"id","msg":{...}}` with nested `msg.type`) than
0.167.0 (`{"type": ...}` flat). An adapter must not assume one framing across versions.

### 6. Self-contained runner (rerun on any host)

```
powershell -NoProfile -ExecutionPolicy Bypass -File tests/feasibility/m0-03/run-codex-exec.ps1 -CodexPath "<path to codex>"
```

Run here with `-CodexPath` = native 0.167.0 exe: exit 0, produced `run-stdout.jsonl`
(`thread.started`, `turn.started`, `item.completed` with `text: PONG`, `turn.completed`),
`run-exit.txt` containing `0`, and `run-last-message.txt` containing `PONG`.
macOS equivalent:

```
codex exec --json --skip-git-repo-check --sandbox read-only --output-last-message last-message.txt "Reply exactly PONG"; echo "EXIT=$?"
```

## Raw artifacts

`tests/feasibility/m0-03/raw/`: `one-shot-stdout.jsonl`, `one-shot-stderr.txt`,
`lastmsg-stdout.jsonl`, `lastmsg-stderr.txt`, `last-message.txt`, `badsandbox-stderr.txt`,
`old-wrapper-stdout.jsonl`, `old-wrapper-stderr.txt`, `old-wrapper2-stdout.jsonl`,
`old-wrapper2-stderr.txt`, `run-stdout.jsonl`, `run-stderr.txt`, `run-exit.txt`,
`run-last-message.txt`.

## macOS blocker

No macOS host was available to this worker (Windows-only workstation, no macOS VM or remote).
Per task rules, macOS capability is not claimed. Reproduction on a native macOS machine:

1. Install Codex CLI (native build), `codex login` with the same account type.
2. Run the section 2 command; expect exit 0 and the four-event JSONL sequence.
3. Run section 3 and 4; expect `PONG` in the file and exit 2 for the invalid flag.
4. Record whether stderr-only diagnostics and stdout NDJSON framing match; record the exact
   binary version (`codex --version`) because framing differs between CLI versions (0.20.0 vs
   0.167.0 shapes above).

## Limitations

- Windows evidence only; macOS unverified (blocker above).
- Single account (ChatGPT login), single model (native CLI default); account-eligible model
  lists vary by plan, and the same account fails on the 0.20.0 wrapper due to the stale pinned
  `model = "gpt-6.1-terra"` in `~\.codex\config.toml`.
- `codex exec` without `--json` prints a human banner (workdir/model/sandbox header) that must
  be filtered if reused; with `--json` stdout is pure NDJSON.
- `--sandbox read-only` was used; workspace-write/danger-full-access behavior is untested here
  and out of scope.
- Stderr carried non-fatal plugin/skills warnings on both versions; they did not affect exit
  status or stdout framing.
- 0.20.0 wrapper exit code on the 400 path: not obtainable; the process hangs after the
  terminal error event (see section 5). The native 0.167.0 executable exits promptly (0 on
  success, 2 on argument error) — the hang is version-specific on the observed error path.
- Cancellation, session interruption, and approval flows (other conformance-suite items from
  the plan) were not exercised; M0-03 scope is one-shot execution and structured output.
