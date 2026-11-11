# Claude Code CLI verification

Recorded 2026-11-07T22:12:57-07:00 on macOS.

## Installation and commands

The installed executable is `/opt/homebrew/bin/claude`. `/opt/homebrew/bin/claude --version` reported `2.1.311 (Claude Code)`.

`/opt/homebrew/bin/claude --help` reported these relevant options:

- `-p` or `--print` runs non-interactively and accepts the prompt as a positional argument.
- `--output-format` accepts `text`, `json`, or `stream-json` in print mode.
- `--include-partial-messages` is available with print mode and `stream-json`.
- `--permission-mode` accepts `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, and `plan`.
- `--permission-prompts` accepts `host` or `none`. `none` denies actions that would prompt.
- `--tools` selects the available built-in tools. An empty string disables all tools.
- `--allowedTools` and `--disallowedTools` allow or deny named tools or tool patterns.
- `--restricted` removes code-running tools and `WebFetch` unless named, ignores user, project, and local settings, confines file tools to working directories, and refuses `bypassPermissions`.
- `--strict-mcp-config` ignores MCP servers not explicitly supplied with `--mcp-config`.
- `--verbose` overrides the configured verbosity.

The help output did not say that `stream-json` requires `--verbose`. A first print-mode attempt without `--verbose` exited 1 before a model request and wrote this error to stderr:

```text
When using --print, --output-format=stream-json requires --verbose
```

The successful command included `--verbose`.

## Permission behavior

The CLI help listed the installed mode values. The [Claude Code permission-mode reference](https://code.claude.com/docs/en/permission-modes) describes their behavior:

| CLI value | Behavior |
| --- | --- |
| `manual` | Reads run without asking. Actions that need approval prompt. In print mode, calls that need approval are denied unless an allow rule or permission handler approves them. The configuration value `default` is an alias. |
| `acceptEdits` | Reads, file edits, and common filesystem commands run without prompting. Other commands can still prompt. |
| `plan` | Claude reads and explores, then proposes a plan. Edits stay blocked until a person approves the plan. |
| `auto` | A safety classifier reviews actions. Actions run without routine prompts only when the checks allow them. |
| `dontAsk` | Reads in working directories and pre-approved tools run. Actions that would prompt are denied, so the session does not wait for input. |
| `bypassPermissions` | Disables permission checks. The help describes `--dangerously-skip-permissions` as bypassing all permission checks. This mode is not used. |

The chosen mode is `dontAsk`. It is more restrictive than `acceptEdits` because it permits only pre-approved tools and denies every other action that would prompt. The adapter pre-approves only `Read`, `Edit`, and `Write`, and exposes only those built-in tools. `--restricted` confines file tools to the working directory. `--strict-mcp-config` is supplied without `--mcp-config`, so the run uses no MCP servers. `--permission-prompts none` prevents an unavailable host prompt from blocking the process.

The live init event confirmed `permissionMode` was `dontAsk`, the available tools were `Edit`, `Read`, and `Write`, and `mcp_servers` was empty. The prompt requested no tool use. This probe did not exercise a tool approval or denial.

The init record also listed installed plugins and skills. `--restricted` did not
prove that customizations were absent. The `tools` and `mcp_servers` fields
confirm the exposed tool set and the absence of MCP servers in this run. Plugin
hooks and other plugin side effects were not exercised.

## Input and output protocol

The prompt is the positional argument following `-p`. It is passed as one argument in the process argument array, not interpolated into a shell command. The runtime supervisor sets child stdin to null, so this integration does not use `--input-format stream-json`.

The successful `stream-json` run emitted one JSON object per stdout line. Observed event shapes included:

- `{"type":"system","subtype":"init","session_id":"...","tools":[...],"mcp_servers":[...],"permissionMode":"dontAsk"}` for startup metadata.
- `{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"..."},...},"session_id":"..."}` for streamed text.
- `{"type":"assistant","message":{"content":[{"type":"text","text":"..."},...]},"session_id":"..."}` for the completed assistant message.
- `{"type":"result","subtype":"success","is_error":false,"result":"...","session_id":"...",...}` for the terminal turn result.

`--include-partial-messages` produced the nested `stream_event` text deltas. `result.subtype` and `result.is_error` describe the turn protocol; the child process exit remains a separate transport fact.

## Verified run

The successful probe ran in unique temporary working directory `/var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T/tmp.lmpekeCQ09` with the one-word, tool-free prompt `OK`:

```text
/opt/homebrew/bin/claude -p "OK" --verbose --output-format stream-json --include-partial-messages --permission-mode dontAsk --permission-prompts none --restricted --strict-mcp-config --tools "Read,Edit,Write" --allowedTools "Read,Edit,Write"
```

The command exited 0. Raw evidence:

- `docs/evidence/m4-02-macos/20261107T215309-0700.stdout.jsonl`
- `docs/evidence/m4-02-macos/20261107T215309-0700.stderr.txt` (empty)
- `docs/evidence/m4-02-macos/20261107T215309-0700.exit-code.txt` (`0`)

## Flags not used

- `--dangerously-skip-permissions`, `--allow-dangerously-skip-permissions`, and `--permission-mode bypassPermissions` were not used because they enable or select permission bypass.
- `--tools default` was not used because it would expose the default tool set instead of the verified `Read`, `Edit`, and `Write` allowlist. No `Bash` tool was named.
- `--input-format stream-json` was not used because the supervisor provides no stdin and the prompt fits in one argument.
- `--output-format text` and `--output-format json` were not used because they do not provide the observed incremental event stream.
- `--resume`, `--continue`, and `--session-id` were not used because this adapter runs one turn and does not implement session resumption.
- `--safe-mode` was not used because it disables `CLAUDE.md` and skills that can carry repository instructions. The explicit tool allowlist still limits built-in tools, but plugin hooks remain unverified.
- `--model` was not used. Model selection is not exposed by this adapter.
