# Agent permission levels (DPES-1105)

Status: design only. Nothing here is implemented, approved, pushed or opened as a PR.

Revision 3. Four levels: Restricted, Extended, Auto, Full. Auto is the default for all three agents (user decision). Open items are listed first with recommendations.

Basis: code at `motif-mcao/main` `cc2b17d36`. CLI facts come from help text only: `codex-cli 0.167.2`, `claude 2.1.311`, `copilot 1.0.109`. No CLI was run for a prompt or turn. Source tags are at the end.

## Decisions needed

Each item states the options, my recommendation, and the cost of each answer. Items 1, 2, 5 and 9 need an answer before PR A.

1. **Does the Auto default cover tasks? (open).** Auto is the default for chats (decided). Options: (a) tasks default to Auto too; (b) tasks default to Restricted, and Auto or Extended is chosen per task at submit. Recommend (b). Cost of (a): Claude and Copilot tasks gain shell with no review point before the run ends, and review errors compound across steps. Cost of (b): one agent has two defaults, which users may read as a bug. The chip shows the level, so the difference is visible.
2. **Does the Copilot sandbox gate apply when a level allows shell? (open, your call).** You said you do not care about the Copilot sandbox. Options A, B and C are in "Copilot sandbox gate". Recommend B: pass `--sandbox` only at Extended and Auto. Cost of A (every non-Full level): Copilot is unusable on hosts without a sandbox except at Full, which pushes users to the least restricted level. Cost of B: Extended and Auto refuse on those hosts, the Copilot default resolves to Restricted there with a visible notice, and the catalog must report availability per level. Cost of C (the current instruction to copilot-drop-sandbox-gate, which stops passing `--sandbox`): shell runs with your full account access on every host.
3. **Confirm that Auto is wider than today on chats.** Codex and Copilot chats are read-only today, and Claude chats have no tools. Auto adds shell and file edits to all three. Recommend: accept, since Auto is the decided default, and say so in the chip wording. Cost of not accepting: the default would have to be Restricted, which contradicts the decision.
4. **Should a review denial fail the turn?** Recommend: yes. "Ask" and denial give `permission_review_denied`, errors give `permission_review_unavailable`, and the reply text is kept. Cost: a chat that finished its answer despite one blocked action shows as failed. Alternative: a new turn state, "completed with blocked actions", which needs a migration to the turn CHECK.
5. **Live check before the default flips (PR E).** Recommend: yes. Each agent's Auto argv is run with a signed-in CLI, and the run checks whether the review engages, whether denials are visible in output, and for Copilot whether `-p` consults the judge. Cost: Auto is not the shipped default until PR E passes, which delays the decided default. If PR E shows an agent gives no denial or error signal, do not offer Auto for that agent (recommend) until the signal exists.
6. **Persistence.** Recommend: only Restricted persists, as a desktop preference, never as daemon state. Auto, Extended and Full reset to Auto on restart. Full is never persisted. Cost: Extended users pick it again each launch. Alternative: persist any choice except Full, so Extended's web access could survive a restart without a new choice.
7. **Codex Full.** Recommend: no `--dangerously-bypass-approvals-and-sandbox`. Full adds no sandbox or approval flag, so the user's `~/.codex/config.toml` decides, as the CLI does when run by hand. Cost: Full's real reach depends on a config MCAO does not read. The bypass flag removes the sandbox and approvals entirely, which is more than the CLI's default.
8. **Claude Full and the allow flag (unverified).** Recommend: pass `--allow-dangerously-skip-permissions` with `--permission-mode bypassPermissions`. The help describes the allow flag as the opt-in that makes bypass available "without it being enabled by default". Cost of passing it when it is not needed: an extra opt-in the user did not need.
9. **Claude Auto keeps `--restricted`.** Recommend: keep. It refuses bypass, confines file tools to working directories, holds git and settings writes for a person, and removes Bash unless `--tools` names it. Cost: the user's `autoMode` settings are ignored for MCAO runs, so the shipped rules apply. Alternative: drop it, and user settings and hooks load.
10. **Spend.** Recommend: never pass `--autopilot`, `--experimental` or `--allow-all-mcp-server-instructions` at any level. Set `--max-ai-credits` on Copilot tasks. The value is yours. The minimum is 30, and the cap is soft, so one response can exceed it.
11. **Codex `--ignore-user-config` at Restricted and Auto.** Recommend: pass it, so user MCP servers and config do not load. Cost: it may drop the user's model and provider settings (unverified). Never pass `--ignore-rules`, which removes the user's execpolicy rules and so loosens the run.
12. **Label.** Recommend: the UI says "Auto" with an agent-specific subtitle. Cost: Copilot's model routing (`--auto-tier`) also uses the word, which may confuse users in the model picker.
13. **Older daemon with the Auto default.** Recommend: the chip switches to Restricted with a visible notice, and the send goes without the field. Restricted is tighter than Auto, and the notice says why. Alternative: block sends until the user picks Restricted or updates the daemon. Cost: the blocking option stops every chat on an older daemon until the user acts.

## Corrections to earlier revisions

- Claude: revision 1 said "Do not use `auto`". That was wrong. `auto` is a `--permission-mode` choice (`claude --help`, lines 151-154).
- Codex: `always, never, auto` belongs to `codex exec --color`, not to approval. The approval flag `-a` exists only at top level, with values `on-request` and `never`, and `codex exec` does not accept it.
- Codex: `--search` is top-level only. `codex exec --help` does not list it.
- Only Copilot's help calls its judge "probabilistic". Claude's and Codex's help do not.
- An older daemon rejects a `permission` field rather than ignoring it (`api_conversations.rs:90`, `deny_unknown_fields`). Nothing runs either way.
- The lines quoted from revision 1 ("Default: Restricted", "Default is Restricted") are in commit `9b7a29327`. Commit `6f5f67f53` already replaced them. This revision replaces the remaining Restricted-default text.

## Basis and limits

- `claude auto-mode defaults` prints the shipped allow, soft_deny and hard_deny rules. It was not run, because only help text was allowed. You can run `! claude auto-mode defaults` locally to see them.
- A search of the 276 MB Claude binary was started by mistake and stopped with TaskStop. No process was left running, and there were no further binary reads.
- No offline Codex or Copilot docs were available beyond help text.

## What Auto-as-default changes

Auto is the default for all three agents. Today's behavior differs by agent and by surface.

| | Codex | Claude Code | GitHub Copilot |
|---|---|---|---|
| Chat today | `--sandbox read-only`, no review. Auto adds workspace-write and automatic review, in a shared scratch directory. | `--tools ""`. Auto adds Bash, Edit and Write. | Read-only, no shell. Auto adds judge-decided shell, and writes in a per-turn run directory. |
| Task today | `--approve-for-me` with no sandbox pinned (`executor.rs:696`). Auto is nearly the same and pins `workspace-write`. Restricted for a task is tighter than today. | Read, Edit and Write, no Bash. Auto adds Bash, decided by the classifier. | No shell, url or memory. Writes are allowed by a worktree rule. Auto adds judge-decided shell. Url and memory stay denied. |
| Net | Tasks: nearly unchanged. Chats: wider. | Wider for chats and tasks. | Wider for chats and tasks, through shell. |

Codex tasks already run `--approve-for-me` today, so Auto-as-default changes almost nothing for them. Codex Restricted tasks are the one new tightening.

## Cross-agent argv by level

Run directory: the worktree for tasks, and the per-turn empty directory for Copilot chats. Codex and Claude chats use one shared scratch directory today (see hard limit 2).

| Level | Codex | Claude Code | GitHub Copilot |
|---|---|---|---|
| Restricted | chat: `exec --json --sandbox read-only --skip-git-repo-check --ephemeral` [C-today]. task: `exec --json --sandbox workspace-write`, no approval flag. Outcome of an approval request with no reviewer: unverified. | chat: `--tools ""`. task: `--tools Read,Edit,Write --allowedTools Read,Edit,Write`. Both: `--permission-mode dontAsk --permission-prompts none --restricted --strict-mcp-config` [CL-today]. | chat: `--allow-tool read --deny-tool write --deny-tool shell --deny-tool url --deny-tool memory`. task: `--allow-tool read --allow-tool 'write(<worktree>/**)' --deny-tool shell --deny-tool url --deny-tool memory`. Copilot base flags without `--sandbox` (decision 2, B) [CP-today]. |
| Extended | No equivalent. `exec` has no tool allowlist and no web flag [C-exec]. Recommend hiding Extended for Codex. | task: `--tools Read,Edit,Write,Bash --allowedTools Read,Edit,Write,"Bash(git status *)","Bash(git diff *)","Bash(git log *)","Bash(git show *)"`, with Restricted's permission flags. Web group adds `WebFetch` to `--tools`. Pattern syntax unverified [CL-tools]. | task: `--allow-tool read --allow-tool 'write(<worktree>/**)' --allow-tool 'shell(git status)' --allow-tool 'shell(git diff)' --allow-tool 'shell(git log)' --allow-tool 'shell(git show)'`, with `--sandbox` (decision 2, B). No `--deny-tool shell` or `url`. Web group adds `--allow-url <host>`. Memory group adds `--enable-memory`. No judge flag [CP-rules]. |
| Auto (default) | chat and task: `exec --json --sandbox workspace-write --approve-for-me`. Chat adds `--skip-git-repo-check --ephemeral` [C-approve]. | chat and task: `--permission-mode auto --permission-prompts none --restricted --strict-mcp-config --tools Read,Edit,Write,Bash --allowedTools Read,Edit,Write`. Bash is deliberately not pre-approved, so the classifier decides each call [CL-auto]. Chat adds `--no-session-persistence`, as today. | `--assisted-approval --allow-tool read --allow-tool 'write(<run-dir>/**)' --deny-tool url --deny-tool memory`, with `--sandbox` (decision 2, B) [CP-assisted] [CP-allow]. No allow-all. No shell rule. |
| Full | No `--sandbox` and no approval flag, so the config decides. Defaults unverified [C-exec]. Not recommended: `--dangerously-bypass-approvals-and-sandbox` [C-bypass]. | `--permission-mode bypassPermissions --permission-prompts none --allow-dangerously-skip-permissions`. Drop `--restricted`, `--strict-mcp-config`, `--tools` and `--allowedTools`. Whether the allow flag is needed is unverified (decision 8) [CL-bypass]. | Copilot base flags minus `--sandbox`, `--disallow-temp-dir` and `--no-custom-instructions`, plus `--allow-all` (`--allow-all-tools --allow-all-paths --allow-all-urls`) [CP-allow]. |

Copilot base flags at every level: `--prompt=<text> --output-format json --log-level none --no-auto-update --no-ask-user --usage-output-file <file> -C <dir>`.

Copilot options by level. "Keep" means the level passes the flag.

| Copilot option | Restricted | Extended | Auto | Full | Reason |
|---|---|---|---|---|---|
| `--sandbox` | drop (B) | keep (B) | keep (B) | drop | Shell is the only thing it covers. See "Copilot sandbox gate". |
| `--disallow-temp-dir` | keep | keep | keep | drop | Removes temp access. An MCAO restriction. |
| `--no-custom-instructions` | keep | keep | keep | drop | `AGENTS.md` is repository content, the main injection path. |
| `--disable-builtin-mcps` | keep | keep | keep | keep | Built-in servers use the signed-in credential. Kept at Full too, which is my recommendation. |
| `--no-ask-user` | keep | keep | keep | keep | Nobody can answer. |
| `--assisted-approval` | - | - | keep | - | The judge. |
| `--allow-all` | - | - | - | keep | Full only. |
| `--enable-memory` | - | memory group only | - | - | Off by default in prompt mode [CP-memory]. |
| `--autopilot`, `--experimental`, `--allow-all-mcp-server-instructions` | never | never | never | never | Spend, unbounded continuation, and MCP instructions in the system prompt. |

| Codex option | Restricted | Auto | Full | Reason |
|---|---|---|---|---|
| `--ignore-user-config` | keep (decision 11) | keep (decision 11) | drop | Skips `$CODEX_HOME/config.toml`. Auth still uses `CODEX_HOME` [C-exec]. |
| `--ignore-rules` | never | never | never | Removes the user's execpolicy rules, which loosens the run. |
| `--dangerously-bypass-hook-trust` | never | never | never | Runs hooks without trust review. MCAO does not vet hook sources. |
| `--add-dir` | never | never | never | Adds writable roots beyond the run directory. |

| Claude option | Restricted | Extended | Auto | Full | Reason |
|---|---|---|---|---|---|
| `--restricted` | keep | keep | keep | drop | Refuses bypass, ignores settings files, holds git and settings writes for a person [CL3]. |
| `--strict-mcp-config` | keep | keep | keep | drop | No MCP servers unless named. |
| `--permission-prompts none` | keep | keep | keep | keep | Prompts become denials, not waits [CL2]. |
| `--add-dir` | never | never | never | never | Adds writable directories beyond the run directory. |
| `--bare` | never | never | never | never | Breaks login [CL-bare]. |

## What "Auto" means, per agent, and who decides

- **Codex.** `--approve-for-me` routes approval requests through "automatic review using the workspace-write sandbox" [C-approve]. The help does not say who or what does the review, and does not call it probabilistic. Whether it is a model is unverified. The workspace-write sandbox still bounds what runs. The flag works under `exec`, which lists it.
- **Claude.** A classifier decides each tool request. It is configured by an `autoMode` section in settings with `environment`, `allow`, `soft_deny` and `hard_deny` rules [CL-auto-sub]. Who decides: the classifier, bounded by those rule lists. The help does not say whether the classifier is a model. `critique` gives "AI feedback" on the rules and takes a `--model` option, which suggests one. Unverified: whether auto mode needs a setting or account feature, and whether it works with `--print`.
- **Copilot.** An "assisted-approval safety judge", which the help calls "probabilistic and not a security boundary" [CP-assisted]. The settings text says it is "a probabilistic LLM review" that auto-approves requests it judges safe, asks otherwise, and "may approve dangerous or unintended actions" [CP-config]. Who decides: an LLM. It is "supported only in non-interactive prompt mode" [CP-env], so it applies to `-p`. It is ignored with a warning when `AUTO_APPROVAL` is disabled or managed policy blocks it [CP-assisted]. Unverified: whether requests no rule covers reach the judge in `-p`, and whether deny rules win over the judge. The judge only ever approves; a request it does not approve is denied in `-p`.

Copilot's "Auto" model routing (`--auto-tier`) is unrelated to this level.

Autopilot is not used. It makes the agent answer its own questions and continue up to five times by default, and each continuation is billed as a premium request [CP-billing].

## Auto's defaults, surface by surface

| Surface | Codex Auto | Claude Auto | Copilot Auto |
|---|---|---|---|
| Shell | On, inside workspace-write. Approval requests go to automatic review. | On, as Bash. The classifier decides each call. | On only when the judge approves, and only with `--sandbox` (decision 2, B). |
| File writes | Inside the workspace roots. Exact roots unverified. | Edit and Write are pre-approved. `--restricted` keeps them in working directories. | Writes under the run directory are allowed by rule. Anything else fails the path check. Temp is removed by `--disallow-temp-dir`. |
| Web (url) | Off. No exec flag [C3]. | Off. WebFetch is not in `--tools`. | Off. `--deny-tool url`. URLs are not covered by `--allow-all-tools` [CP-permissions]. |
| Memory | Unverified. No exec flag. | Unverified. Not named in `--restricted` help. | Off. `--deny-tool memory`. |
| MCP servers | User servers in `config.toml`: unverified. `--ignore-user-config` would skip them. | Off. `--strict-mcp-config`. | User servers in `~/.copilot/mcp-config.json` still load [CP-mcp]. Their calls need the judge. Built-ins are off. |
| Custom instructions (AGENTS.md, CLAUDE.md) | Unverified. | Unverified. `--restricted` does not name CLAUDE.md. | Off. `--no-custom-instructions`. |
| User hooks | `--dangerously-bypass-hook-trust` is never passed. Trust behavior unverified. | Off under `--restricted`, which ignores settings files. | On. They run on every Copilot run, and no help flag disables them (`copilot_adapter.rs` base comment). |
| Provider keys in child env | `OPENAI_API_KEY` and related names are forwarded (`codex_adapter.rs:21-27`), so shell can read them. | `ANTHROPIC_API_KEY` is forwarded (`claude_code_adapter.rs:36`), so shell can read it. | None. Copilot starts from the shared baseline, which has no COPILOT entry (`env_policy.rs`). |

## Hard limits that stay under Auto

These are argv-level or environment-level, so they hold even when the review fails. Each applies at Auto, and at Restricted and Extended unless noted.

1. **Writes only under the run directory.** Copilot: the `write(<run-dir>/**)` rule, no `--allow-all-paths`, and `--disallow-temp-dir`. Claude: `--restricted` confines file tools to working directories. Codex: the workspace-write roots. `--add-dir` and `--allow-all-paths` are never passed. Copilot's worktree rule does not cover a linked worktree's shared git directory, which lies outside the worktree. Writes there fail the path check or need a rule (unverified).
2. **A per-turn run directory for every chat.** Codex and Claude chats share one scratch directory today (`api_conversation_agent.rs`, `working_dir: self.config.scratch`). Under Auto, writes would persist across turns. Recommend a per-turn directory, as Copilot already has (`CopilotRun`).
3. **No web access.** Copilot `--deny-tool url`. Claude has no WebFetch in `--tools`. Codex exec has no web flag. Why: a network path the review cannot police.
4. **No memory.** Copilot `--deny-tool memory`. Why: memory carries instructions across sessions. Claude and Codex unverified.
5. **No MCP built-ins and no Claude MCP servers.** `--disable-builtin-mcps` and `--strict-mcp-config`. Why: external credentials. Copilot user MCP servers are a known gap.
6. **No repository instructions.** `--no-custom-instructions` on Copilot. Why: repository content is untrusted input. Claude and Codex unverified.
7. **Never forward `COPILOT_ALLOW_ALL=true`.** MCAO's environment policy has no COPILOT entry, so Copilot starts without it. Help: `true` "trusts the working directory without prompting, which loads that directory's skills, plugins, MCP servers, and hooks, including hooks that run shell commands" [CP-env]. Keep a test that no `COPILOT_*` variable is forwarded at any level.
8. **No bypass at Restricted, Extended or Auto.** No allow-all, bypass mode, bypass-hook-trust, bypass sandbox, or `--ignore-rules`.
9. **No human in the loop.** `--permission-prompts none`, `--no-ask-user` and `--no-auto-update`. Why: a prompt becomes a denial, not a wait.
10. **Git and settings writes need a person.** Claude `--restricted`. Why: `.git` hooks and tool settings run later as code.
11. **No autopilot or experimental features** at any level. Spend (decision 10).

## When the review cannot answer

The safe outcome on every agent is deny. The design never retries at another level and never falls back to a looser one. The argv is fixed for the turn, and the daemon records the level it used.

| Outcome | Action | Turn result | Code |
|---|---|---|---|
| Review says "ask" (needs a human) | Denied. MCAO has no human, so "ask" is a denial. | Failed, reply text kept | `permission_review_denied` |
| Review denies | Denied. | Failed, reply text kept | `permission_review_denied` |
| Review errors, times out or cannot be reached | Denied. | Failed, reply text kept | `permission_review_unavailable` |
| Copilot judge disabled or blocked by policy | Requests that need review are denied. Writes under the run directory still run, because the rule allows them. | Failed, if the warning is visible in output. Otherwise no reason (decision 5). | `permission_review_unavailable` |
| Rule denial (deny rule, path check, no allow rule, `--restricted`) | Denied. | Not a failure. This is normal Restricted behavior. | none |

Rules:
- A reason is set only when the CLI output shows a review denial or error. The signal is unverified for all three agents, and decision 5 covers the case where it never appears.
- The chip says a review step decides risky actions. If the review is off, requests are denied, so a wrong claim produces a denial, not a loosening.
- Do not reuse the existing `permission_denied` code, which means the daemon refused access (`m7-08` code list).

## Copilot sandbox gate: facts, options and what changes later

Facts:
- `--sandbox` runs shell inside an OS sandbox: Seatbelt on macOS, bubblewrap on Linux, and MXC ProcessContainer on Windows, on supported versions only. Without it, shell runs with your full account access. Built-in file edits are not OS-sandboxed in either case, so the write rule is the only control over edits [CP-sandbox].
- An explicit `--sandbox` on a host that cannot run it makes sandboxed shell, MCP/LSP servers and extensions fail, and Copilot prints a warning [CP-sandbox].
- MCAO today: the gate is in `copilot_adapter.rs:51-170`. Windows is refused with `WINDOWS_REASON` (line 51). The catalog checks the host (`executor.rs:137`), so Copilot is listed unavailable without it. Launch checks it again (`executor.rs:755`, `spawn_with_host` at `copilot_adapter.rs:370`). Gate tests are `tests/copilot_adapter.rs:176-229`.
- The gate is per agent today, not per level.

Options for shell at Extended and Auto (decision 2):
- **A. `--sandbox` at every non-Full level.** Cost: Copilot is unusable on hosts without a sandbox except at Full, which pushes users to the least restricted level. Restricted runs no shell, so the sandbox adds nothing there.
- **B. `--sandbox` at Extended and Auto only (recommended).** Restricted needs none, because it denies shell, and Full runs without one by design. Cost: Extended and Auto refuse on hosts without a sandbox, with `permission_sandbox_unavailable`. The catalog must report availability per level. The Copilot default resolves to Restricted on those hosts, with a visible notice. On Windows, Restricted runs and Extended and Auto refuse until the sandbox is verified there, which matches the `WINDOWS_REASON` text.
- **C. No `--sandbox` anywhere (the current instruction to copilot-drop-sandbox-gate).** Cost: judge-approved and named shell runs with your full account access on every host. Copilot's help describes this state as the same access as your user account, read, write and delete anywhere, with no restriction on credentials [CP-sandbox]. The only bounds left are the deny rules, the run-directory write rule, and the judge.

What changes if you later want the sandbox back:
- **C to B:** add `--sandbox` to the Extended and Auto argv (base flags, `copilot_adapter.rs` around line 494). Restore the per-level host check on the launch path (`spawn_with_host`, `copilot_adapter.rs:370`, called at `executor.rs:755`) and the catalog check (`executor.rs:137`). Restore the gate tests if they were deleted. This is a flag change if the gate functions stay in the tree, and a rewrite if they were deleted. Recommend keeping `sandbox_host_status` and its tests under C, so that restoring is cheap.
- **B to A:** add `--sandbox` to Restricted too. Copilot then needs a sandbox host for every non-Full level.
- The copilot-drop-sandbox-gate branch needs the answer to decision 2 before it lands. Its current instruction matches option C.

## Threat notes

- **Auto versus Full.** Claude Auto keeps `--restricted`, which refuses bypass and holds git and settings writes for a person. Full drops it and loads the user's settings and hooks. Copilot Auto keeps the deny rules, the write rule, the path check and, under B, the sandbox for shell. Full keeps none of them. Codex Auto keeps workspace-write. Codex Full has no sandbox if the bypass flag is used, and otherwise whatever the config sets.
- **Prompt injection.** The review reads the same untrusted repository content as the agent. Copilot's help says the judge "may approve dangerous or unintended actions". Auto is therefore not a boundary. The enforced limits are the argv and environment limits above.
- **Secrets.** Auto's shell can read the environment, including provider keys for Codex and Claude. Copilot's `--secret-env-vars` removes named variables from shell and MCP environments [CP-help-run]. It is a candidate for Auto, unverified against the baseline. Codex and Claude need their keys to run, and removing them may break login (unverified).
- **Spend.** Copilot bills AI credits per model call, and whether judge calls are billed is not stated. `--max-ai-credits` is a soft cap with a minimum of 30 [CP-billing]. Denied actions can make the agent retry, and each retry is a paid call. Claude and Codex spend controls were not checked.
- **Writes outside the run directory.** Under Auto: Copilot's path check, Claude's `--restricted`, and Codex workspace-write roots (unverified). Under Full: anywhere the account can write.
- **What MCAO can promise.** The argv each level passes. The level recorded on a turn or task equals the level whose flags were passed. The hard limits above hold as far as each CLI enforces its documented flags.
- **What MCAO cannot promise.** The review's decisions. That the review engaged. Enforcement beyond the help text. That a person chose Full, because the daemon has no caller identity (`0033` comment). That user config adds nothing at Full. That shell cannot read secrets.

## Default, scope, persistence, indicator and confirmation

- **Default.** Auto for every agent and chat (decided). Tasks follow decision 1.
- **Persistence.** Restricted persists as a desktop preference (decision 6). The daemon stores no level. Auto, Extended and Full reset to Auto on restart. Full is never persisted.
- **Indicator.** A chip beside the composer is always visible. It shows the level the next message will use, and the daemon-reported level of the last turn. It has a one-click "Restricted" action. Tasks show their level in the task list.
- **Auto.** No confirmation. The chip says: "Auto: the agent runs commands and edits files in its workspace without asking you. A review step decides each risky action, and it can be wrong. Web access, memory, MCP servers and custom instruction files stay off. You can switch to Restricted at any time."
- **Copilot Auto adds:** "Copilot's review is a probabilistic LLM judge. Copilot's documentation says it is not a security boundary and may approve dangerous or unintended actions."
- **Full.** Confirmation each time Full is selected, and for tasks each time a task is submitted with Full. Focus starts on Cancel. Wording:
  - Codex: "Codex runs with your own ~/.codex settings. MCAO adds no sandbox or approval flags, so the sandbox and approval settings in that config decide what runs."
  - Claude: "Claude Code skips its permission checks, and uses your own Claude settings and hooks. It can run any tool your account allows."
  - Copilot: "Copilot can run any shell command, edit any file your account can, reach any URL, and use your own Copilot settings and hooks. No OS sandbox is applied. Copilot cannot ask you anything during the run."
  - Tasks add: "This task runs unattended after you submit. Objective: <text>. Worktree: <path>."
- **Displayed level.** Each turn and task shows the level the daemon reports it ran with (`permission_level`, from `conversations.get` and the task record), not the level the user chose. If the two differ, the UI shows an error.

## Wire contract and older daemon

- `conversations.send {target, text, idempotency_key, selection?, permission?, extended_groups?}`.
- `permission` is `restricted`, `extended`, `auto` or `full`. An absent field means `restricted`, which is today's behavior, so older clients and their idempotency keys are unchanged.
- `extended_groups` is a subset of `git-read`, `web`, `shell-named` and `memory`. An unknown value is `InvalidParams` and records no turn, the same rule `selection` follows.
- Capability `permission_levels`, advertised once the daemon maps all four levels.
- Turn records from `conversations.get`, and the send receipt, carry `permission_level` and `permission_profile` (for example `copilot:assisted:v1`).
- **Older daemon.** The desktop never sends `permission` to it. If the chip is Auto, the chip switches to Restricted with a visible notice, and the send goes without the field, which the older daemon runs as read-only. Any other chosen level is blocked with: "This daemon predates permission levels. Update the daemon, or choose Restricted." Decision 13 sets this behavior.
- **Why the rule holds.** The current code rejects unknown fields (`api_conversations.rs:90`), so a `permission` field sent to an older daemon fails with `InvalidParams`. Nothing runs. The composer never sends a Restricted-looking request that runs as Full, and the daemon never upgrades a request.
- **Failure codes on `turn_failed`.** `permission_review_denied`, `permission_review_unavailable`, `permission_level_unsupported` (for example Codex Extended), and `permission_sandbox_unavailable` (decision 2, option B).
- **Idempotency.** The send key includes `permission` when present. Restricted with no field keeps today's key.
- **Fixture mode.** No `conversations.send` call is made, and the SIMULATED notice applies as it does today.

## Audit and migration

- **Migration `0035_conversation_permission.sql`.** `0034` is the latest migration on `motif-mcao/main`, and no local worktree has a `0035`. The migration adds nullable `permission_level` (CHECK on the four values) and `permission_profile` to `conversation_turns`. NULL means a turn from before this change. Both are written at turn creation, next to `runtime_name`.
- **Tasks.** The contract gets `permission_level`, with serde default `restricted`, because old contracts ran with today's arguments. The launch event carries `permission_level` and `permission_profile` (`executor.rs:592`, `event_fields`). Whether a migration is also needed depends on how contracts are stored, which I did not verify.
- **`conversation_audit` is unchanged.** Its CHECK allows only `clear`, `remove` and `restore` (`0033:9`). Choosing Full is recorded on the turn, not as an audit row.
- **Auto as default.** A new desktop always sends an explicit `permission`, so a turn recorded as `auto` was chosen by the default or by the user. Turns recorded before this change show NULL, never `auto`.
- **Blocked-action counts are not recorded.** How the CLIs report denials is unverified (decision 5).
- **Activity view.** Shows the level per turn and per task.

## Tests

- **Argv tables.** One test per agent, level and surface, with exact vectors. Auto is the default, so these are the first tests to write. Assert the Auto vectors above, including Copilot's write rule scoped to the run directory and no `--allow-all*`.
- **Hard limits, all levels.** No `--allow-all-paths`, `--add-dir`, `--dangerously-*`, `--ignore-rules`, `--autopilot`, `--experimental` or `--allow-all-mcp-server-instructions` at Restricted, Extended or Auto. No `COPILOT_*` name in any forwarded environment.
- **Run directory.** Codex and Claude chats each get a per-turn directory, and writes do not persist across turns.
- **Review outcomes, with fake output.** A fake agent that prints a review denial yields `permission_review_denied`. One that prints an error yields `permission_review_unavailable`. In both cases the fake is launched exactly once. A rule denial (for example Restricted attempting shell) is not a failure. These fake outputs test the parser only, not real CLI output.
- **Wire, in-process.** Absent means `restricted`. An unknown `permission` value is rejected with no turn. Capability gating. Codex Extended is refused with `permission_level_unsupported`. Copilot Extended and Auto on a host without a sandbox are refused with `permission_sandbox_unavailable` (option B).
- **Store.** Migration `0035` is nullable, the CHECK holds, and old rows read NULL. An old contract with no level reads as `restricted`.
- **Desktop Vitest.** Chats default to Auto, tasks follow decision 1, and the chip is always rendered with a one-click Restricted action. Only Restricted is written to the preference. Full needs confirmation each time. The displayed level comes from the daemon, and a mismatch shows an error. Fixture mode never sends. On an older daemon, the chip switches to Restricted with a notice.
- **Desktop e2e, real daemon and fake agent.** A default chat sends `permission: auto`, and the fake's argv shows the Auto flags. Switching to Restricted changes the argv. Choosing Full, confirming, and sending shows the Full flags. The indicator shows the reported level.
- **Not testable without a signed-in CLI and paid calls.** Whether `-p` sends unmatched Copilot requests to the judge. Whether the judge engages. Copilot's warning visibility. Codex automatic review. Claude auto mode under `--print`, and its classifier. Whether `bypassPermissions` needs the allow flag. The denial and error formats for all three. Codex exec approval defaults. These need the live run in PR E, with your approval, since it costs money.

## Rollout

0. This document. Optional docs-only PR, not opened.
1. Answer decision 2 (sandbox). Update the copilot-drop-sandbox-gate instruction to match.
2. PR A: Copilot argv per level, with the run-directory write rule, the judge, and the sandbox per decision 2. Size M.
3. PR B: Claude and Codex argv per level. Codex tasks pin `workspace-write`, and Codex Restricted tasks drop the approval flag. Per-turn run directories for Codex and Claude chats. `--ignore-user-config` per decision 11. Size M.
4. PR C: wire field, capability, failure codes, migration `0035`, turn and task fields. Size M to L.
5. PR D: desktop chip, Auto default (scope per decision 1), Full confirmation, persistence rule, older-daemon behavior, displayed level. Size M. The default is gated by decision 5.
6. PR E: live check with a signed-in CLI, with your approval because it costs money. Its results settle decisions 4 and 5. Size S.

Every PR: run the Trivy scan (`trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .`), use the PR template from `mc-innersource/.github`, and do not push without approval.

## Not verified

- **Copilot.** Whether `-p` sends unmatched requests to the judge. Whether deny rules win over the judge. Judge billing. Warning visibility at other log levels. The managed-policy case. Whether user MCP servers load under Auto in `-p`. The `--secret-env-vars` baseline. What `--sandbox` covers on each host beyond the help text.
- **Claude.** Whether auto mode needs a setting or account feature, and whether it works under `--print` in practice. The classifier's model and rules (`defaults` was not run). Whether `bypassPermissions` needs `--allow-dangerously-skip-permissions`. Whether CLAUDE.md and auto-memory load under `--restricted`. The `--allowedTools` pattern syntax. How denials appear in stream-json.
- **Codex.** What `--approve-for-me` reviews, and whether it is a model. The exec default approval policy. Read scope, writable roots and network under workspace-write. Config keys for exec, including approval policy and web search. Side effects of `--ignore-user-config`. Whether `AGENTS.md` loads. The Windows backend.
- **Not run.** Any CLI prompt or turn. `codex features list`. `claude auto-mode defaults`, `config`, `critique` and `reset`.

## Source tags

Help text is `<cli> --help` unless noted.

- **C-today.** `crates/orchestrator-agents/src/catalog.rs:850-867`, Codex chat argv. The Codex task argv is `executor.rs:696`.
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
- **CP-allow.** `copilot --help`, `--allow-all-tools`: "denies tool requests that no allow rule (or `--assisted-approval`) approves".
- **CP-assisted.** `copilot --help`, `--assisted-approval`.
- **CP-config.** `copilot help config`, `defaultPermissionMode`: "assisted" is "a probabilistic LLM review". The value is ignored outside interactive runs, so it does not apply to `-p`.
- **CP-env.** `copilot help environment`: `COPILOT_ASSISTED_APPROVAL` ("supported only in non-interactive prompt mode"), `COPILOT_ALLOW_ALL`, and `COPILOT_AUTO_TIER`.
- **CP-permissions.** `copilot help permissions`: "In autopilot and -p runs, a tool permission request that would prompt is denied."
- **CP-sandbox.** `copilot help sandbox`: disabled by default, the effects of enabled and disabled, and the host support list.
- **CP-billing.** `copilot help billing`, `copilot help limits` (`--max-ai-credits`, minimum 30, soft cap), and `copilot --help` (`--max-autopilot-continues`, default 5).
- **CP-mcp.** `copilot help mcp`: user servers in `~/.copilot/mcp-config.json`.
- **CP-memory.** `copilot --help`, `--enable-memory`: "disabled by default" in prompt mode.
- **CP-help-run.** `copilot --help`, `--secret-env-vars`.
- **Source files.** `copilot_adapter.rs:51-170` (gate, Windows reason), `:370` (`spawn_with_host`), `:494` (`--sandbox` in base argv). `executor.rs:137` (catalog host check), `:696` (Codex task argv), `:755` (Copilot launch). `api_conversations.rs:90` (`deny_unknown_fields`). `api_conversation_agent.rs` (chat working directories). `env_policy.rs` (no COPILOT entry).
