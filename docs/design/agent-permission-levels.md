# Agent permission levels (DPES-1105)

Status: design only. Nothing here is implemented, approved, pushed or opened as a PR.

Revision 2 adds Auto as the default level for all three agents, a cross-agent argv table, per-agent Auto semantics, and the failure rules for a review that cannot answer. Corrections to revision 1 are listed below.

Basis: code at `motif-mcao/main` `cc2b17d36`. CLI facts come from help text only: `codex-cli 0.167.2`, `claude 2.1.311`, `copilot 1.0.109`. No CLI was run for a prompt or turn. Source tags are listed at the end.

## Decisions needed

Each item has a recommendation. Items 1 to 4 and 14 need an answer before any PR.

1. **Scope of the Auto default.** Recommend: Auto is the default for chats only. Tasks default to Restricted, and Auto or Extended can be chosen per task at submit. Why: Auto widens tasks on Claude and Copilot (see "Auto is wider than today"), and tasks keep their changes after the run. Risk of this choice: tasks start less capable than chats, which may confuse users. Risk of Auto for tasks too: unattended edits and shell with no review point before the run ends, and review errors compound over many steps.
2. **Auto is wider than today on chats as well.** Codex and Copilot chats are read-only today, and Claude chats have no tools. Auto adds shell and file edits to all three. Confirm this is the intent.
3. **Codex Full.** You listed `--dangerously-bypass-approvals-and-sandbox` as Full. Recommend: no sandbox or approval flag, so the user's `~/.codex/config.toml` decides, which is what the CLI does when run by hand. Keep `--disable-builtin-mcps` on Copilot Full. The bypass flag removes the sandbox and approvals entirely, and its help says it is intended for externally sandboxed environments.
4. **Copilot Auto has no allow-all and no shell or write rule.** Recommend: the judge is the only approver for shell and writes. This fails closed: if the judge is off, those requests are denied in `-p` runs. Cost: Copilot Auto does only what the judge approves. Alternative: an explicit shell list, which is deterministic and less capable. Open point: whether `-p` sends unmatched requests to the judge at all. The `--allow-all-tools` help text suggests it does, but this is unverified.
5. **Claude Auto keeps `--restricted`.** Recommend: keep. It refuses bypass, confines file tools to working directories, requires a person to approve git and settings writes, and removes Bash unless `--tools` names it. Cost: `--restricted` ignores user settings, so the user's `autoMode` rules do not apply, and Auto uses the shipped rules. MCAO could pass its own `autoMode` through `--settings`, which is unverified. Alternative: drop `--restricted`, and then user settings and hooks load.
6. **Claude Full needs `--allow-dangerously-skip-permissions`?** The help describes that flag as the opt-in that makes bypass available "without it being enabled by default". Recommend: pass both `--allow-dangerously-skip-permissions` and `--permission-mode bypassPermissions` for Full. Unverified: confirm with a run the user does themselves, not an MCAO turn.
7. **Codex Extended.** `codex exec` has no tool allowlist and no web flag. Recommend: hide Extended for Codex and show "not available for Codex". Alternative: map Extended to workspace-write without review, which depends on an approval default that is unverified.
8. **Copilot warning visibility.** The judge is "ignored, with a warning" when `AUTO_APPROVAL` is disabled or managed policy blocks it. MCAO runs with `--log-level none`. Recommend: check in the live run (PR E) whether the warning appears at a higher log level. If it does, Copilot Auto can fail with `permission_judge_unavailable`. If not, the design can promise denial but not a reason.
9. **Persistence.** Recommend: persist only Restricted, as a desktop preference, never as daemon state. Auto returns on restart. Extended and Full reset. Full is never persisted.
10. **Spend.** Recommend: never pass `--autopilot`, `--experimental` or `--allow-all-mcp-server-instructions` at any level. For Copilot tasks, set `--max-ai-credits`. The minimum is 30 and the cap is soft, so one response can exceed it. The value is your decision.
11. **Codex `--ignore-user-config` at Restricted and Auto.** Recommend: pass it, so user MCP servers and config do not load. Unverified: it may also drop the user's model and provider settings. Never pass `--ignore-rules`, which removes the user's execpolicy rules and so loosens the run.
12. **Label.** "Auto" also names Copilot's model routing (`--auto-tier`, `COPILOT_AUTO_TIER`). Recommend: the UI label "Auto" with an agent-specific subtitle. The internal value stays `auto`.
13. **Older daemon.** Recommend: an absent `permission` field means Restricted, which is today's behavior, so older clients are unchanged. Against an older daemon, the composer sends only Restricted and blocks other levels with a message.
14. **Ship the Auto default before live verification?** Recommend: no. The default flips in PR D only after PR E has run each agent's Auto argv with a signed-in CLI. Until then the default is Restricted.

## Corrections to revision 1

- Claude: revision 1 said "Do not use `auto`". That was wrong. `auto` is a `--permission-mode` choice (`claude --help`, lines 151-154).
- Codex: `always, never, auto` belongs to `codex exec --color`, not to approval. The approval flag `-a, --ask-for-approval` exists only at top level, with values `on-request` and `never`. `codex exec` does not accept it.
- Codex: `--search` is top-level only. `codex exec --help` does not list it.
- Only Copilot's help calls its judge "probabilistic". Claude's and Codex's help text do not.
- An older daemon rejects a `permission` field rather than ignoring it (`api_conversations.rs:90`, `deny_unknown_fields`). The safety rule below holds either way.

## Basis and limits

- `claude auto-mode defaults` prints the shipped allow, soft_deny and hard_deny rules. It was not run, because only help text was allowed. You can run `! claude auto-mode defaults` locally to see them.
- A search of the 276 MB Claude binary for auto-mode strings was started by mistake. It exceeded its time limit and was stopped with TaskStop, and no process was left running. There were no further binary reads.
- No offline Codex or Copilot docs were available beyond help text.

## Cross-agent argv by level

Chat and task differences are noted in each cell. The Copilot base flags are the same at every level, and the per-option table below shows which ones each level keeps.

| Level | Codex | Claude Code | GitHub Copilot |
|---|---|---|---|
| Restricted | chat: `exec --json --sandbox read-only --skip-git-repo-check --ephemeral` [C-today]. task: `exec --json --sandbox workspace-write`, with no approval flag. Outcome of an approval request without a reviewer: unverified. | chat: `--tools ""`. task: `--tools Read,Edit,Write --allowedTools Read,Edit,Write`. Both: `--permission-mode dontAsk --permission-prompts none --restricted --strict-mcp-config` [CL-today] | chat: `--allow-tool read --deny-tool write --deny-tool shell --deny-tool url --deny-tool memory`. task: `--allow-tool read --allow-tool 'write(<worktree>/**)' --deny-tool shell --deny-tool url --deny-tool memory` [CP-today] |
| Extended | No equivalent. `exec` has no tool allowlist and no web flag [C-exec]. | task: `--tools Read,Edit,Write,Bash --allowedTools Read,Edit,Write,"Bash(git status *)","Bash(git diff *)","Bash(git log *)","Bash(git show *)"`, with the same permission flags as Restricted. Web group adds `WebFetch` to `--tools`. Pattern syntax unverified [CL-tools]. | task: `--allow-tool read --allow-tool 'write(<worktree>/**)' --allow-tool 'shell(git status)' --allow-tool 'shell(git diff)' --allow-tool 'shell(git log)' --allow-tool 'shell(git show)'`. No `--deny-tool shell` or `url`. Web group adds `--allow-url <host>`. Memory group adds `--enable-memory`. No judge flag [CP-rules]. |
| Auto (default) | `exec --json --sandbox workspace-write --approve-for-me`, chat and task [C-approve] | `--permission-mode auto --permission-prompts none --restricted --strict-mcp-config --tools Read,Edit,Write,Bash --allowedTools Read,Edit,Write`. Bash is deliberately not pre-approved, so the classifier decides each call [CL-auto]. | `--assisted-approval --allow-tool read --deny-tool url --deny-tool memory`. No allow-all, and no shell or write rule [CP-assisted] [CP-allow]. |
| Full | No `--sandbox` and no approval flag, so the config decides. Defaults unverified [C-exec]. Not recommended: `--dangerously-bypass-approvals-and-sandbox` [C-bypass]. | `--permission-mode bypassPermissions --permission-prompts none --allow-dangerously-skip-permissions`. Drop `--restricted`, `--strict-mcp-config`, `--tools` and `--allowedTools`. Whether the allow flag is needed: unverified [CL-bypass]. | Base minus `--sandbox`, `--disallow-temp-dir` and `--no-custom-instructions`, plus `--allow-all`, which means `--allow-all-tools --allow-all-paths --allow-all-urls` [CP-allow]. |

Copilot base flags at every level: `--prompt=<text> --output-format json --log-level none --no-auto-update --no-ask-user --usage-output-file <file> -C <dir>`.

Per-option rules. "Keep" means the level passes the flag.

| Copilot option | Restricted | Extended | Auto | Full | Reason |
|---|---|---|---|---|---|
| `--sandbox` | keep | keep | keep | drop | Sandboxing is disabled by default on Copilot [CP-sandbox]. |
| `--disallow-temp-dir` | keep | keep | keep | drop | Removes temp access. An MCAO restriction. |
| `--no-custom-instructions` | keep | keep | keep | drop | `AGENTS.md` is repository content, which is the main injection path. |
| `--disable-builtin-mcps` | keep | keep | keep | keep | Built-in servers use the signed-in credential. Recommended at Full too (decision 3). |
| `--no-ask-user` | keep | keep | keep | keep | Nobody can answer. |
| `--assisted-approval` | - | - | keep | - | The judge. |
| `--allow-all` | - | - | - | keep | Full only. |
| `--enable-memory` | - | memory group only | - | - | Off by default in prompt mode [CP-memory]. |
| `--autopilot`, `--experimental`, `--allow-all-mcp-server-instructions` | never | never | never | never | Cost and unbounded continuation. See Spend. |

| Codex option | Restricted | Auto | Full | Reason |
|---|---|---|---|---|
| `--ignore-user-config` | keep (decision 11) | keep (decision 11) | drop | Skips `$CODEX_HOME/config.toml`. Auth still uses `CODEX_HOME` [C-exec]. |
| `--ignore-rules` | never | never | never | Removes the user's execpolicy rules, which loosens the run. |
| `--dangerously-bypass-hook-trust` | never | never | never | Runs hooks without trust review. MCAO does not vet hook sources. |
| `--add-dir` | never | never | never | Adds writable roots. |

| Claude option | Restricted | Extended | Auto | Full | Reason |
|---|---|---|---|---|---|
| `--restricted` | keep | keep | keep | drop | Refuses bypass and ignores settings files [CL3]. |
| `--strict-mcp-config` | keep | keep | keep | drop | No MCP servers unless named. |
| `--permission-prompts none` | keep | keep | keep | keep | Prompts become denials, not waits [CL2]. |
| `--bare` | never | never | never | never | Breaks login [CL-bare]. |

## What "Auto" means, per agent, and who decides

- **Codex.** `--approve-for-me` routes approval requests through "automatic review using the workspace-write sandbox" [C-approve]. The help does not say who or what does the review, and does not call it probabilistic. Whether the review is a model is unverified. The workspace-write sandbox still bounds what runs. It works under `exec`, which lists the flag. Unverified: the review's outcome and what happens when the reviewer fails.
- **Claude.** A classifier decides each tool request, configured by an `autoMode` section in settings with `environment`, `allow`, `soft_deny` and `hard_deny` rules [CL-auto-sub]. Who decides: the classifier, bounded by those rule lists. The help does not say whether the classifier is a model. The `critique` subcommand gives "AI feedback" on the rules and takes a `--model` option, which suggests one. Unverified: whether auto mode needs a setting or account feature, and whether it works with `--print`. The help does not say it fails under `--print`, and `--permission-prompts` is documented for `--print`.
- **Copilot.** An "assisted-approval safety judge", which the help calls "probabilistic and not a security boundary" [CP-assisted]. The settings text says it is "a probabilistic LLM review" that auto-approves requests it judges safe, asks otherwise, and "may approve dangerous or unintended actions" [CP-config]. Who decides: an LLM. It works in `-p` runs: the environment help says it is "supported only in non-interactive prompt mode" [CP-env]. It needs `AUTO_APPROVAL` enabled and no managed-policy block, otherwise it is ignored with a warning [CP-assisted]. Unverified: whether unmatched requests reach the judge in `-p`, and whether deny rules win over the judge.

Copilot's "Auto" model routing is unrelated to this level.

Autopilot is not used. Autopilot makes the agent answer its own questions and continue up to five times by default. Each continuation is billed as a premium request [CP-billing].

## Auto's defaults, surface by surface

| Surface | Codex Auto | Claude Auto | Copilot Auto |
|---|---|---|---|
| Shell | On, inside workspace-write. Approval requests go to automatic review. | On, as Bash. The classifier decides each call. | On only when the judge approves. The macOS sandbox gate applies. Copilot is refused on Windows today. |
| File writes | Inside workspace roots. Exact roots unverified. | Edit and Write are pre-approved. `--restricted` keeps them in working directories. | The judge decides writes in the working directory. Writes outside it fail the path check. Temp is removed by `--disallow-temp-dir`. |
| Web (url) | Off. No exec flag [C3]. | Off. WebFetch is not in `--tools`. | Off. `--deny-tool url`. URLs are not covered by `--allow-all-tools` [CP-permissions]. |
| Memory | Unverified. No exec flag. | Unverified. Not named in `--restricted` help. | Off. `--deny-tool memory`. |
| MCP servers | User servers in `config.toml`: unverified. `--ignore-user-config` would skip them. | Off. `--strict-mcp-config`. | User servers in `~/.copilot/mcp-config.json` still load [CP-mcp]. Their calls go to the judge. Built-ins are off. |
| Custom instructions (AGENTS.md, CLAUDE.md) | Unverified. | Unverified. `--restricted` does not name CLAUDE.md. | Off. `--no-custom-instructions`. |
| User hooks | `--dangerously-bypass-hook-trust` is not passed. Trust behavior unverified. | Off under `--restricted`, which ignores settings files. | On. They run on every Copilot run, and no help flag disables them (`copilot_adapter.rs` base comment). |
| Provider keys in child env | `OPENAI_API_KEY` and related names are forwarded (`codex_adapter.rs:21-27`), so shell can read them. | `ANTHROPIC_API_KEY` is forwarded (`claude_code_adapter.rs:36`), so shell can read it. | None forwarded (shared baseline). |

### Auto is wider than today

- **Chats.** Codex and Copilot chats are read-only today, and Claude chats have no tools. Auto adds shell and file edits to all three (decision 2).
- **Tasks.** Claude tasks have no Bash today. Copilot tasks have no shell and no url today. Auto adds shell that is judged or classifier-decided. Codex tasks pass `--approve-for-me` with no sandbox pinned (`executor.rs:696`). Auto pins `workspace-write`, which tightens the run if the config default is wider.

## Hard limits that stay under Auto

These are argv-level, so they hold even when the review fails.

1. **No web access.** Copilot `--deny-tool url`. Claude has no WebFetch in `--tools`. Codex has no web flag on exec. Why: a network path the review cannot police.
2. **No memory.** Copilot `--deny-tool memory`. Why: memory carries instructions across sessions.
3. **No MCP built-ins, and no Claude MCP servers.** `--disable-builtin-mcps` and `--strict-mcp-config`. Why: external credentials. Copilot user MCP servers are a known gap (see Not verified).
4. **Writes only in the worktree or the scratch directory.** Copilot has no `--allow-all-paths` and passes `--disallow-temp-dir`. Claude uses `--restricted`. Codex uses workspace-write. Why: the worktree is the unit a task delivers.
5. **No repository-supplied instructions.** `--no-custom-instructions` on Copilot. Why: repository content is untrusted input.
6. **No bypass.** No allow-all, no bypass mode, no bypass-hook-trust and no bypass sandbox. Why: those belong to Full.
7. **No human in the loop.** `--permission-prompts none` and `--no-ask-user`. Why: a prompt becomes a denial instead of a wait.
8. **Git and settings writes need a person.** Claude `--restricted` holds these. Why: `.git` hooks and tool settings are code that runs later.

## When the review cannot answer

The safe outcome on every agent is deny. Never retry at another level. Never fall back silently to Restricted or Full.

| Condition | Codex | Claude | Copilot |
|---|---|---|---|
| Review says ask or deny | Automatic review refuses, and the command fails. What the agent sees is unverified. | Denied automatically by `--permission-prompts none` [CL2]. | Denied. `-p` runs deny requests that would prompt [CP-permissions]. |
| Review errors or times out | Unverified. The sandbox still bounds the run, and a nonzero exit fails the turn (`codex_adapter.rs` header). | Unverified whether it reports a denial or an error. | Unverified. The request is denied unless another approval applies. |
| Review disabled or blocked by policy | Not documented in help. | Not documented in help. | The flag is ignored with a warning. Requests without an allow rule are denied. The warning is visible only if the log level shows it (decision 8). |

Failure reason: `permission_judge_unavailable`, only when the CLI output shows the review could not answer. Otherwise the turn finishes and records the denials it can see. The design does not promise a reason. Do not reuse the existing `permission_denied` code, which means the daemon refused access (`m7-08` code list).

## Threat notes

- **Auto versus Full.** Claude Auto keeps `--restricted`, which refuses bypass and holds git and settings writes for a person. Full drops it and loads the user's settings and hooks. Copilot Auto keeps the deny rules, the path check and the macOS sandbox. Full has none of them. Codex Auto keeps workspace-write. Codex Full has no sandbox if the bypass flag is used, and otherwise whatever the config sets.
- **Prompt injection.** The review reads the same untrusted repository content as the agent. Copilot's help says the judge "may approve dangerous or unintended actions". Auto is therefore not a boundary. The enforced limits are the argv-level hard limits above.
- **Secrets.** Auto's shell can read the environment, including provider keys (see the surface table). Copilot's `--secret-env-vars` removes named variables from shell and MCP environments [CP-help-run]. It is a candidate for Auto, unverified against the baseline. Codex and Claude need their keys to run, and removing them may break login (unverified).
- **Spend.** Copilot bills AI credits per model call. Whether judge calls are billed is not stated. `--max-ai-credits` is a soft cap with a minimum of 30 [CP-billing]. Denied actions can make the agent retry, and each retry is a paid call. Claude and Codex spend controls were not checked.
- **Writes outside the worktree.** Under Auto: Copilot's path check, Claude's `--restricted`, and Codex workspace-write roots (unverified). Under Full: anywhere the account can write.
- **What MCAO can promise.** The argv each level passes. The level recorded on a turn or task equals the flags passed. The argv-level hard limits above hold as far as each CLI enforces its documented flags.
- **What MCAO cannot promise.** The review's decisions. That the review engaged. Enforcement beyond the help text. That a person chose Full, because the daemon has no caller identity (`0033` comment). That user config adds nothing at Full. That shell cannot read secrets.

Copilot's own guidance says enforceable protection needs managed restrictions, sandboxing, least-privilege credentials and egress controls [CP-config]. Auto's wording below repeats that point.

## Default, scope, persistence, indicator and confirmation

- **Default.** Auto for chats (decision 1). Tasks default to Restricted, and Auto or Extended can be chosen per task at submit.
- **Persistence.** Restricted persists as a desktop preference (decision 9). The daemon stores no level. Auto, Extended and Full reset on restart. Full is never persisted.
- **Auto.** No confirmation. An always-visible chip beside the composer shows the level, and the chip has a one-click "Restricted" action.
- **Full.** Confirmation each time Full is selected, and for tasks each time a task is submitted with Full. Focus starts on Cancel. Wording:
  - Codex: "Codex runs with your own ~/.codex settings. MCAO adds no sandbox or approval flags, so the sandbox and approval settings in that config decide what runs."
  - Claude: "Claude Code skips its permission checks, and uses your own Claude settings and hooks. It can run any tool your account allows."
  - Copilot: "Copilot can run any shell command, edit any file your account can, reach any URL, and use your own Copilot settings and hooks. No OS sandbox is applied. Copilot cannot ask you anything during the run."
  - Tasks add: "This task runs unattended after you submit. Objective: <text>. Worktree: <path>."
- **Auto wording, all agents.** "Auto lets the agent run commands and edit files in its workspace without asking you. A review step decides each risky action, and it can be wrong. Web access, memory, MCP servers and custom instruction files stay off. You can switch to Restricted at any time."
- **Copilot Auto adds.** "Copilot's review is a probabilistic LLM judge. Copilot's documentation says it is not a security boundary and may approve dangerous or unintended actions."
- **Displayed level.** The chip shows the level the next message will use. Each turn and task shows the level the daemon reports it ran with (`permission_level`, from `conversations.get`). If the reported level differs from the one requested, the UI shows an error.

## Wire contract and older daemon

- `conversations.send {target, text, idempotency_key, selection?, permission?, extended_groups?}`.
- `permission` is `restricted`, `extended`, `auto` or `full`. An absent field means `restricted`, which is today's behavior, so older clients and their idempotency fingerprints are unchanged.
- `extended_groups` is a subset of `git-read`, `web`, `shell-named` and `memory`. An unknown value is `InvalidParams` and records no turn, the same rule `selection` follows.
- Capability `permission_levels`, advertised once the daemon maps all four levels.
- Turn records from `conversations.get`, and the send receipt, carry `permission_level` and `permission_profile` (for example `copilot:assisted:v1`). The desktop displays these.
- **Older daemon.** The desktop never sends `permission` to it. For Restricted, it sends the request without the field, and the older daemon runs read-only, which is Restricted. For any other level, it blocks the send with: "This daemon predates permission levels and runs chats read-only. Update the daemon, or choose Restricted."
- **Why the rule holds.** The current code rejects unknown fields (`api_conversations.rs:90`). A `permission` field sent to an older daemon therefore fails with `InvalidParams`, and nothing runs. The composer never sends a Restricted-looking request that runs as Full, and the daemon never upgrades a request.
- **Failure codes on `turn_failed`.** `permission_level_unsupported` (for example Codex Extended). `permission_sandbox_unavailable` (Copilot without a sandbox host for a level that needs one). `permission_judge_unavailable` (see above).
- **Idempotency.** The send key includes `permission` when present. Restricted with no field keeps today's key.
- **Fixture mode.** No `conversations.send` call is made, and the SIMULATED notice applies as it does today.

## Audit and migration

- **Migration `0035_conversation_permission.sql`.** `0034` is the latest migration on `motif-mcao/main`, and no local worktree has a `0035`. The migration adds nullable `permission_level` (CHECK on the four values) and `permission_profile` to `conversation_turns`. NULL means a turn from before this change. The values are written at turn creation, next to `runtime_name`.
- **Tasks.** The contract gets `permission_level` with serde default `restricted`, because old contracts ran with today's arguments. The launch event carries `permission_level` and `permission_profile` (`executor.rs:592`, `event_fields`). Whether a migration is also needed depends on how contracts are stored, which I did not verify.
- **`conversation_audit` is unchanged.** Its CHECK allows only `clear`, `remove` and `restore` (`0033:9`). Choosing Full is recorded on the turn, not as an audit row.
- **Blocked-action counts are not recorded.** How the CLIs report denials is unverified.
- **Activity view.** Shows the level per turn and per task.

## Tests

- **Argv tables.** One test per agent, level and surface, with exact vectors. Assert that Restricted, Extended and Auto never contain `--allow-all`, `--dangerously-*`, `bypassPermissions`, `--autopilot`, `--experimental` or `--ignore-rules`. Assert that Full drops the sandbox and restriction flags listed above. Restricted vectors are unchanged except the Codex task.
- **In-process API.** Absent means `restricted`. An unknown `permission` value is rejected. Codex Extended is refused with `permission_level_unsupported`. Capability gating. Turn record fields.
- **Process level, fake agent** (`examples/fake_agent.rs`). The argv reaches the fake. A fake that prints a review error yields `permission_judge_unavailable`. That tests the parser against fake output, not real behavior.
- **Store.** Migration `0035` is nullable, the CHECK holds, and old rows read NULL.
- **Desktop Vitest.** Chats default to Auto and tasks to Restricted. Full needs confirmation. Only Restricted is written to a preference. The chip has a one-click Restricted action. An older daemon blocks non-Restricted sends. Fixture mode never sends. The displayed level comes from the daemon.
- **Desktop e2e, real daemon and fake agent.** Choose Full, confirm, send. The launch log shows the Full flags, and the indicator shows the reported level.
- **Not testable without a signed-in CLI and paid calls.** Whether `-p` sends unmatched Copilot requests to the judge. Whether the judge engages. Copilot's warning visibility. Codex automatic review. Claude auto mode with `--print`, and its classifier. Whether `bypassPermissions` needs the allow flag. Codex exec approval defaults. These need a live run that you approve (PR E).

## Rollout

0. This document. Optional docs-only PR, not opened.
1. Coordinate with `copilot-drop-sandbox-gate` so the sandbox gate becomes level-dependent. Size S.
2. PR A: Copilot per-level argv, including the judge and Full. Size M.
3. PR B: Claude and Codex per-level argv. The Codex task pins `workspace-write`. Apply the `--ignore-user-config` decision. Size M.
4. PR C: wire field, capability, `permission_level` on turns and tasks, migration `0035`, unknown-field tests. Size M to L.
5. PR D: desktop default, chip, Full confirmation, older-daemon block, displayed level. Size M. The Auto default flips here only after PR E passes (decision 14).
6. PR E: opt-in live verification with a signed-in CLI, with your approval because it costs money. Size S.

Every PR: run the Trivy scan (`trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .`), use the PR template from `mc-innersource/.github`, and do not push without approval.

## Not verified

- **Copilot.** Whether `-p` sends unmatched requests to the judge. Whether deny rules win over the judge. Judge billing. Warning visibility at other log levels. The managed-policy case. Whether user MCP servers load under Auto in `-p`. The `--secret-env-vars` baseline.
- **Claude.** Whether auto mode needs a setting or account feature. Whether it works under `--print` in practice. The classifier's model and rules (`defaults` was not run). Whether `bypassPermissions` needs `--allow-dangerously-skip-permissions`. Whether CLAUDE.md and auto-memory load under `--restricted`. The `--allowedTools` pattern syntax. How denied actions appear in stream-json.
- **Codex.** What `--approve-for-me` reviews. The exec default approval policy. Read scope, writable roots and network under workspace-write. Config keys for exec, including approval policy and web search. Side effects of `--ignore-user-config`. Whether `AGENTS.md` loads. The Windows backend.
- **Not run.** Any CLI prompt or turn. `codex features list`. `claude auto-mode defaults`, `config`, `critique` and `reset`.

## Source tags

Help text is `<cli> --help` unless noted.

- **C-today.** `crates/orchestrator-agents/src/catalog.rs:850-867`, Codex chat argv.
- **C-exec.** `codex exec --help`: `--sandbox`, `--approve-for-me`, `--ignore-user-config`, `--ignore-rules`, `--ephemeral`, `--add-dir`, `--color`. No `-a` and no `--search`.
- **C-approve.** `codex exec --help`, `--approve-for-me`: "Route approval requests through automatic review using the workspace-write sandbox".
- **C-bypass.** `codex exec --help`, `--dangerously-bypass-approvals-and-sandbox`.
- **C3.** `codex --help`, `-a, --ask-for-approval` (values `on-request` and `never`), and `--search` (top level only).
- **CL-today.** `catalog.rs:914-948` and `claude_code_adapter.rs:69-89`.
- **CL-tools.** `claude --help`, `--tools` and `--allowedTools`.
- **CL-auto.** `claude --help`, `--permission-mode` choices: acceptEdits, auto, bypassPermissions, manual, dontAsk, plan.
- **CL2.** `claude --help`, `--permission-prompts`: "none" means anything that would prompt is denied.
- **CL3.** `claude --help`, `--restricted`: removes Bash, PowerShell and WebFetch unless `--tools` names them, ignores user, project and local settings, refuses bypass, and holds writes to settings, git and tool-configuration files for a person.
- **CL-bypass.** `claude --help`, `--allow-dangerously-skip-permissions` ("without it being enabled by default") and `--dangerously-skip-permissions`.
- **CL-bare.** `claude --help`, `--bare`: OAuth and keychain are never read.
- **CL-auto-sub.** `claude auto-mode --help`, and the `config`, `defaults`, `critique` and `reset` subcommand help.
- **CP-today.** `copilot_adapter.rs:330-358` (conversation and task argv) and `:481-503` (base argv).
- **CP-rules.** `copilot help permissions`, rule grammar: `shell(git status)`, `write(path)`, `read(path)`, `memory`, `url(domain)`. Deny rules win over every allow.
- **CP-allow.** `copilot --help`, `--allow-all` and `--allow-all-tools`: "denies tool requests that no allow rule (or `--assisted-approval`) approves".
- **CP-assisted.** `copilot --help`, `--assisted-approval`.
- **CP-config.** `copilot help config`, `defaultPermissionMode`: "assisted" is "a probabilistic LLM review". The value is ignored outside interactive runs, so it does not apply to `-p`.
- **CP-env.** `copilot help environment`: `COPILOT_ASSISTED_APPROVAL` ("supported only in non-interactive prompt mode"), `COPILOT_ALLOW_ALL` (`true` trusts the directory and loads its hooks) and `COPILOT_AUTO_TIER`.
- **CP-permissions.** `copilot help permissions`: "In autopilot and -p runs, a tool permission request that would prompt is denied."
- **CP-sandbox.** `copilot help sandbox`: disabled by default.
- **CP-billing.** `copilot help billing`, `copilot help limits` (`--max-ai-credits`, minimum 30, soft cap), and `copilot --help` (`--max-autopilot-continues`, default 5).
- **CP-mcp.** `copilot help mcp`: user servers in `~/.copilot/mcp-config.json`.
- **CP-memory.** `copilot --help`, `--enable-memory`: "disabled by default" in prompt mode.
- **CP-help-run.** `copilot --help`, `--secret-env-vars`.
