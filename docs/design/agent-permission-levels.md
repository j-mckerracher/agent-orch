# Agent permission levels (DPES-1105)

Status: design, revision 4. Chats are implemented (PR A, #80). Tasks are implemented by PR C (DPES-1105). The live check with signed-in CLIs has not run; it is a follow-up, not a gate.

Levels: Restricted, Extended, Auto (default), Full. Agents: Codex, Claude Code, GitHub Copilot.

Basis: `motif-mcao/main` at `cc2b17d36`. PR #75 (removes the Copilot sandbox gate) and PR #76 (test counts) are in flight. CLI facts come from help text only: `codex-cli 0.167.2`, `claude 2.1.311`, `copilot 1.0.109`. No CLI was run for a prompt or turn.

## Decisions

### Decided by the user, applied in this revision

- **D1.** Auto is the default for chats and tasks, for all three agents. A task can still pick Restricted, Extended or Full at submit.
- **D2.** Copilot runs with no `--sandbox` at any level. No sandbox gate and no per-level host check. Restoring the sandbox later is a new design, not a flag switch.
- **D3.** The Auto default ships now. No live check gates it. The live check is a follow-up for you to run (see "Follow-up: live check").

### Open. Each has a working default that applies until you decide

- **O1. Codex Full.** Working default: no `--dangerously-bypass-approvals-and-sandbox`. Full adds no sandbox or approval flag, so `~/.codex/config.toml` decides. Cost: what Full can do depends on a config MCAO does not read.
- **O2. `--max-ai-credits` for Copilot runs.** Working default: the cap is off. It sits behind one switch, `AGENT_ORCH_COPILOT_CREDIT_CAP=1`, read when the daemon starts. With the switch on, every Copilot run (chat, task and the sign-in check) passes `--max-ai-credits 30`, the minimum the CLI accepts. The user has not chosen the value 30. Cost when on: a long run can stop partway. The cap is soft, so one response can exceed it (help).
- **O3. Review denial fails the turn.** Working default: yes. A denial or an "ask" gives `permission_review_denied`, and the reply text is kept. Cost: a chat that answered well but had one blocked action shows as failed.
- **O4. Persistence.** Working default: a Restricted choice persists per surface (chat or task). Auto, Extended and Full reset to Auto on restart. Full never persists. Cost: Extended is chosen again on each launch.
- **O5. Label.** Working default: "Auto", with an agent-specific subtitle. Cost: Copilot's `--auto-tier` routing uses the same word.
- **O6. Older daemon.** Working default: chats fall back to Restricted with a notice. Tasks are blocked with a message. See "Wire contract and older daemon".
- **O7. Claude spend cap on tasks (new).** `--max-budget-usd` works only with `--print` (help), which MCAO uses. Working default: no cap. No Claude run passes the flag, and no switch for it exists yet. Whether to add one is open. Codex `exec` help lists no spend flag, so MCAO sets no Codex cap.
- **O8. Delivery by the agent (new).** Working default: task runs deny `git push` and `gh`, where the CLI can match a rule. Otherwise a task agent could publish a branch outside MCAO's delivery path. Copilot: `--deny-tool 'shell(git push)'` (rule form from help). Claude: `--disallowedTools "Bash(git push *)" "Bash(gh *)"` (pattern syntax unverified). Codex: no per-command deny exists, and network under `workspace-write` is unverified.

### Settled in this design. Reviewable, no user input needed

- Claude keeps `--restricted` at Auto. Settings and git writes then need a person, and file tools stay in the working directories.
- `--ignore-user-config` on Codex at Restricted, Auto and tasks. Never `--ignore-rules`, `--autopilot`, `--experimental` or `--allow-all-mcp-server-instructions`.
- Codex tasks pin `--sandbox workspace-write` at Auto. Without a pin, Auto would mean whatever the user's config says. Today Codex tasks run `--approve-for-me` with no sandbox pin (`executor.rs:696`), so the pin tightens that.
- A request with no `permission` field means `restricted`, on the wire and in stored contracts. The desktop chooses Auto by sending it explicitly, so an old client never gains Auto.
- Claude and Codex chats run in a per-turn directory, not the shared scratch directory (`api_conversation_agent.rs`, `self.config.scratch`). Auto allows writes, and writes must not carry over between turns.

## Corrections and changes since revision 3

- Revision 3 proposed keeping `sandbox_host_status` and its tests for a later restore. D2 drops that. PR #75 already removes the gate and adds `assert!(!args.iter().any(|arg| arg == "--sandbox"))`.
- Revision 3 treated the Copilot sandbox as an optional bound on shell. Under D2, Copilot shell has no OS boundary. See "Copilot without a sandbox".
- `tasks.submit` denies unknown fields (`api.rs:78-79`), as `conversations.send` does (`api_conversations.rs:90`). An older daemon rejects a `permission` task field and runs nothing.
- Copilot is not a chat agent in the picker. Copilot chat exists only when `AGENT_ORCH_CONVERSATION_AGENT=github-copilot` (PR 74). The Copilot chat rows below apply only in that configuration.
- The desktop submits tasks today (`ProjectDetail.tsx`, `daemon/api.ts`). The older-daemon task rule is a real path.

## Basis and limits

- `claude auto-mode defaults` prints the shipped allow, soft_deny and hard_deny rules. It was not run. You can run `! claude auto-mode defaults` locally.
- Revision 2 started a grep over the 276 MB Claude binary by mistake. It was stopped with TaskStop, and no process was left running.
- There are no offline Codex or Copilot docs beyond help text.

## What an unattended Auto task can do

"Enforced" means the CLI's own mechanism. "Judged" means a model or classifier decides each call.

- **Codex.** Edits files and runs shell commands in `workspace-write`. Approval requests go to automatic review. Enforced: the sandbox confines shell and file edits to the workspace roots (roots and network unverified). Judged: approval requests. Not allowed by argv: web search (no exec flag), MCP servers, user config (`--ignore-user-config`).
- **Claude.** Reads, edits and writes files in the worktree (`--restricted`). Runs Bash, where the classifier judges each call. Enforced: file tools stay in working directories, settings and git writes need a person, no WebFetch, no MCP servers. Not enforced: Bash is not confined to the worktree. A judged Bash call can write anywhere the account can.
- **Copilot, no sandbox.** Reads and writes under the worktree by rule. Runs shell only when the judge approves. Enforced: `write(<worktree>/**)`, `--disallow-temp-dir`, the path check on file tools, `--deny-tool url`, `--deny-tool memory`. Not enforced: shell. The write rule does not apply to shell commands (help: "A rule with a path does not apply to shell commands"). With no sandbox, a judged shell command runs with the account's full access and can write outside the worktree.

**Worktree-only writes, stated plainly.** They hold for Codex, where the sandbox covers shell (roots unverified). They hold for the file tools of Claude and Copilot. They do not hold for Claude Bash or Copilot shell. For those two, the only check on a shell write outside the worktree is the review.

**Auto defaults by surface** (chats and tasks, unless marked)

| Surface | Codex | Claude | Copilot |
|---|---|---|---|
| Shell | On in `workspace-write` (enforced) | On as Bash, judged per call | On when judged; no sandbox |
| File writes | Workspace roots (enforced; roots unverified) | Edit, Write in working directories (enforced) | Worktree rule for file tools (enforced); shell writes not confined |
| Web (url) | Off, no flag; config unverified | Off, no WebFetch | Off, `--deny-tool url` |
| Memory | Unverified | Unverified | Off, `--deny-tool memory` |
| MCP servers | Off with `--ignore-user-config` (unverified) | Off, `--strict-mcp-config` | Built-ins off. User servers in `~/.copilot/mcp-config.json` still load (known gap) |
| Repository instructions (AGENTS.md, CLAUDE.md) | Unverified | Unverified; `--restricted` does not name CLAUDE.md | Off, `--no-custom-instructions` |
| User hooks | Not bypassed; trust unverified | Off under `--restricted` | On. No help flag disables them |
| Provider keys in child env | `OPENAI_*` forwarded, readable by shell | `ANTHROPIC_API_KEY` forwarded, readable by shell | None. No `COPILOT_*` entry in the env policy |
| Spend cap | None set by MCAO | None (O7 open) | Off by default, behind the O2 switch |
| Git push, `gh` | Unverified | Denied, O8 | Denied, O8 |

## Cross-agent argv by level

Copilot base flags at every level, as `base_args` builds them after PR #75: `--prompt=<text> --output-format json --log-level none --no-auto-update --no-ask-user --no-custom-instructions --disable-builtin-mcps --disallow-temp-dir --usage-output-file <file> -C <dir>`. The rows below name only what each level adds or removes. Run directory: the worktree for tasks, a per-turn directory for chats. `<prompt>` and `<objective>` are one argument each, after `--` for Codex.

| Level | Codex | Claude Code | GitHub Copilot |
|---|---|---|---|
| **Restricted, chat** | `exec --json --sandbox read-only --skip-git-repo-check --ephemeral` [C-today], plus `--ignore-user-config` (new) | `--tools ""` (unchanged) [CL-today] | `--allow-tool read --deny-tool write --deny-tool shell --deny-tool url --deny-tool memory` [CP-today] |
| **Restricted, task** | `exec --json --sandbox workspace-write --ignore-user-config`. No approval flag, so an approval request has no reviewer and is refused (unverified) | `--permission-mode dontAsk --permission-prompts none --restricted --strict-mcp-config --tools Read,Edit,Write --allowedTools Read,Edit,Write` [CL-today] | `--allow-tool read --allow-tool 'write(<worktree>/**)' --deny-tool shell --deny-tool url --deny-tool memory` |
| **Extended, task** | No equivalent. `exec` has no tool allowlist and no web flag. Hidden [C-exec] | `--tools Read,Edit,Write,Bash --allowedTools Read,Edit,Write,"Bash(git status *)","Bash(git diff *)","Bash(git log *)","Bash(git show *)"` with Restricted's permission flags. Pattern syntax unverified [CL-tools] | `--allow-tool read --allow-tool 'write(<worktree>/**)' --allow-tool 'shell(git status)' --allow-tool 'shell(git diff)' --allow-tool 'shell(git log)' --allow-tool 'shell(git show)' --deny-tool url --deny-tool memory`. No shell deny, since a deny would beat the allows. Unmatched shell is denied in `-p` with no judge [CP-allow] |
| **Auto, chat** | `exec --json --sandbox workspace-write --approve-for-me --ignore-user-config --skip-git-repo-check --ephemeral` [C-approve] | `--permission-mode auto --permission-prompts none --restricted --strict-mcp-config --no-session-persistence --tools Read,Edit,Write,Bash --allowedTools Read,Edit,Write`. Bash is not pre-approved, so the classifier decides each call [CL-auto] | `--assisted-approval --allow-tool read --allow-tool 'write(<run-dir>/**)' --deny-tool url --deny-tool memory --deny-tool 'shell(git push)' --max-ai-credits 30` (only with the O2 switch on). Copilot chat only, see Corrections |
| **Auto, task** | `exec --json --sandbox workspace-write --approve-for-me --ignore-user-config -- <objective>` | Auto chat flags without `--no-session-persistence`, plus `--disallowedTools "Bash(git push *)" "Bash(gh *)"` (O8) and `--max-budget-usd <value>` (O7). Working directory is the worktree | `--assisted-approval --allow-tool read --allow-tool 'write(<worktree>/**)' --deny-tool url --deny-tool memory --deny-tool 'shell(git push)' --max-ai-credits 30` (only with the O2 switch on). No `--allow-all`, no shell allow |
| **Full** (chat and task) | `exec --json` plus model selection only. No sandbox, no approval flag, user config loaded. Not the bypass flag (O1) | `--permission-mode bypassPermissions --permission-prompts none --allow-dangerously-skip-permissions`. Drop `--restricted`, `--strict-mcp-config`, `--tools`, `--allowedTools`. Whether the allow flag is needed is unverified [CL-bypass] | Base flags, minus `--disallow-temp-dir` and `--no-custom-instructions`, plus `--allow-all` (`--allow-all-tools --allow-all-paths --allow-all-urls`) [CP-allow]. Keeps `--disable-builtin-mcps` and `--no-ask-user`. Plus `--max-ai-credits` (O2 switch on only) |

Notes on the table:

- **Codex** has no `--assisted-approval` or `--allow-all` equivalent, and its approval policy values are top-level only (`-a`, `on-request` and `never`). `codex exec` rejects `-a` [C-exec].
- **Copilot.** `--allow-all` and `--yolo` are the same flag set (help), and `--allow-all` is used here. Deny rules beat allow rules, including `--allow-all-tools` (help).
- **Claude** Full drops every restriction flag. Its `--permission-prompts none` stays, so a prompt is still a denial.

**Flags never passed at any level:** `--dangerously-skip-permissions`, `--dangerously-bypass-approvals-and-sandbox`, `--dangerously-bypass-hook-trust`, `--ignore-rules`, `--add-dir`, `--allow-all-paths` (except Full), `--autopilot`, `--experimental`, `--allow-all-mcp-server-instructions`, `--bare` (breaks login [CL-bare]).

## Hard limits that stay under Auto

These are argv or environment limits, so they hold when the review fails. Each is listed with what enforces it.

1. **Writes.** Per agent, as stated above. Codex: sandbox. Claude: `--restricted` for file tools. Copilot: write rule and path check for file tools. No `--add-dir` and no `--allow-all-paths`.
2. **No web, memory, MCP servers or repository instructions.** Copilot enforces web, memory and repository instructions with flags. Built-in MCP servers are off, but user MCP servers still load (known gap). Claude enforces web and MCP with flags. Codex enforces none of them by argv (unverified).
3. **Never forward `COPILOT_ALLOW_ALL=true`.** Help: `true` "trusts the working directory without prompting, which loads that directory's skills, plugins, MCP servers, and hooks, including hooks that run shell commands" [CP-env]. The env policy has no COPILOT entry (`env_policy.rs`), so Copilot starts without it. Test: no `COPILOT_*` name is forwarded at any level.
4. **No bypass below Full.** No allow-all, bypass mode, bypass-hook-trust, bypass sandbox, `--ignore-rules`, `--add-dir` or `--allow-all-paths`.
5. **No autopilot or experimental features.** Autopilot answers the agent's own questions and continues up to five times by default, each continuation billed [CP-billing]. `--experimental` and `--allow-all-mcp-server-instructions` widen what the run loads.
6. **No human in the loop.** `--permission-prompts none`, `--no-ask-user`, `--no-auto-update`.
7. **Git and settings writes need a person.** Claude `--restricted`: "lets only a person or the configured permission handler approve writes to settings, git and tool-configuration files" [CL3].
8. **Delivery stays with MCAO.** No `git push` or `gh` by a task agent (O8). Enforced by rule on Copilot and Claude. Unverified on Codex.
9. **Spend caps on paid runs.** O2 for Copilot, off by default behind a switch. O7 for Claude tasks, not implemented. Codex has no MCAO cap.

## When the review cannot answer (fail closed)

The safe outcome is always denial. A turn never retries at another level and never falls back to a looser one. The argv is fixed for the turn. A failed-turn reason is set only when the CLI output shows the cause. Where the signal is unverified, this design does not promise a reason.

| Case | Outcome | Turn | Reason code |
|---|---|---|---|
| Review says "ask" (no human to answer) | Denied | Failed (O3) | `permission_review_denied` |
| Review denies | Denied | Failed (O3) | `permission_review_denied` |
| Review errors, times out or cannot be reached | Denied. The classifier and reviewer failure modes are unverified, so denial is the assumed behavior | Failed | `permission_review_unavailable`, only if the output shows it |
| Copilot judge off, or blocked by managed policy | Requests with no allow rule are denied in `-p`, shell included. Rule-allowed writes still run | Failed only if the warning is visible in output. At `--log-level none` it may not be | `permission_review_unavailable`, only if visible |
| Rule denial (deny rule, path check, `--restricted`, no allow rule) | Denied | Not a failure. Normal behavior | none |

- **The Copilot judge-off case can fail silently.** The judge is "ignored, with [a] warning" when disabled (help), and MCAO runs at `--log-level none`. The live check decides whether a higher log level shows the warning, or whether MCAO needs a probe. Until then, MCAO promises the denial, not the reason.
- **Wording.** Where the review is named, it says "automatic review", never "guaranteed" (see "Default, scope, persistence and confirmation").
- **Do not reuse `permission_denied`.** It means the daemon refused access, per the m7-08 code list.

## Copilot without a sandbox

- **Facts.** `--sandbox` is off by default. Built-in file edits are not OS-sandboxed in any mode (`copilot help sandbox`). With the sandbox off, shell runs with your account's access: "read, write, and delete anywhere you can, reach any network your machine can, and use your credentials without restriction" (`copilot help sandbox`).
- **What D2 changes in MCAO.** No `--sandbox` at any level. `copilot_sandbox_host`, `sandbox_host_status` and their tests are removed (PR #75). The catalog no longer gates Copilot by host, so Copilot is listed on every host, Windows included. No `permission_sandbox_unavailable` code exists.
- **What still bounds Copilot.** Deny rules (`url`, `memory`, `shell(git push)`), the worktree write rule, the path check on file tools, `--disallow-temp-dir`, `--no-ask-user`, and the judge at Auto.
- **Restoring the sandbox later is a new design.** It needs a per-level host gate, a shell policy per level, Windows verification (MXC, on listed versions only, per `copilot help sandbox`) and new tests. Do not treat it as a flag switch.

## Default, scope, persistence and confirmation

- **Default.** Auto for chats and tasks, for all three agents (D1). The task form starts at Auto and can be changed at submit.
- **Chip.** The model picker chip beside the composer ends with the level the next message will use. Its visible label is short, for example `Codex · default · medium · Auto`, so it fits beside the conversation target at 960 CSS px without wrapping or clipping. A word that is default in both parts reads once (`Codex · default · Auto`). The accessible name and the panel carry the full words (`Codex · default model · medium · Auto`). A fixture agent's chip drops its placeholder qualifier, because the placeholder badge already says so. The level is chosen in the chip's panel, which lists the four levels. Extended, and on an older daemon every level but Restricted, stay listed disabled with the reason. The daemon-reported level of the last turn is shown below the composer row. The task form (PR C) takes the same control, and task lists show each task's level. The always-visible indicator and the one-click switch to Restricted were removed at the user's request (DPES-1105, after the PR #82 review), so the chip and its panel are the only place the level is shown and changed.
- **Auto disclosure (removed).** The "What automatic review does" disclosure and its per-agent texts (Codex, Claude, Copilot) were removed from the UI at the user's request (DPES-1105). The Full confirmation still says that automatic review at Auto is not a guarantee.
- **Full.** Confirmation each time Full is chosen, for chats and for each task submit. Focus starts on Cancel. Text: Codex: "Codex runs with your own ~/.codex settings. MCAO adds no sandbox or approval flags, so the settings in that config decide what runs." Claude: "Claude Code skips its permission checks and uses your own Claude settings and hooks. It can run any tool your account allows." Copilot: "Copilot can run any shell command, edit any file your account can, reach any URL, and use your own Copilot settings and hooks. No OS sandbox is applied. Copilot cannot ask you anything during the run." Tasks add: "This task runs unattended after you submit. Objective: `<text>`. Worktree: `<path>`."
- **Displayed level.** Each turn and task shows the level the daemon reports it ran with, not the level chosen. A mismatch shows an error.

## Wire contract and older daemon

- `conversations.send` and `tasks.submit` take `permission?: restricted | extended | auto | full`. `conversations.send` also takes `extended_groups?`, a subset of `git-read`, `web`, `shell-named` and `memory`. An unknown value is `InvalidParams` and records nothing.
- Absent means `restricted`, on the wire and in stored task contracts (serde default).
- Capability `permission_levels`, advertised once all four levels are mapped for that agent.
- Turn and task records carry `permission_level` and `permission_profile`, for example `copilot:assisted:v1`.
- **Older daemon.** It rejects unknown fields and runs nothing, so the desktop must not send the field to it.
  - **Chats (O6).** The chip reads Restricted, with a notice below the composer, and the send goes without the field. The older daemon runs chats read-only, which is Restricted. A chosen non-Restricted level is blocked with: "This daemon predates permission levels. Update the daemon, or choose Restricted."
  - **Tasks (O6).** Blocked with: "This daemon predates task permission levels. Update the daemon to submit tasks." Tasks are blocked, not downgraded, because an older daemon runs a task with today's argv. For Codex that is `--approve-for-me` with no pin (`executor.rs:696`), which is looser than Restricted.
- **Why it holds.** A `permission` field sent to an older daemon fails with `InvalidParams`, so a Restricted-looking request never runs as Full, and the daemon never upgrades a request.
- **Failure codes** on `turn_failed` and task failures: `permission_review_denied`, `permission_review_unavailable`, `permission_level_unsupported` (Codex Extended).
- **Idempotency.** The send and submit keys include `permission` when present. A request without it keeps today's key.
- **Fixture mode.** No call is made. The SIMULATED notice applies, as it does today.

## Audit and migration

- **Conversations.** Migration `0035_conversation_permission.sql` adds nullable `permission_level` (CHECK on the four values) and `permission_profile` to `conversation_turns`. `0034_idempotency_scrub.sql` is the latest migration on `motif-mcao/main`. NULL means a turn from before this change.
- **Tasks: no migration.** The contract is stored as JSON in `tasks.contract_json` (`sqlite_ops.rs:1712`), so the field is a serde addition with default `restricted`. The launch event carries the level fields (`executor.rs:592`, `event_fields`).
- **`conversation_audit` is unchanged.** Its CHECK allows only `clear`, `remove` and `restore` (`0033:9`).
- **Reading the values.** The desktop always sends the field, so a turn recorded as `auto` came from the default or from the user. Older rows read NULL, never `auto`.
- **Not recorded.** Blocked-action counts. How the CLIs report denials is unverified.

## Threat notes

- **Injection.** The review reads the same untrusted repository content as the agent. Copilot's help says the judge "may approve dangerous or unintended actions". Auto is not a boundary.
- **Secrets.** Codex and Claude children hold provider keys, and Auto shell can read them. `--secret-env-vars` on Copilot removes named variables from shell and MCP environments (help). Its baseline is unverified.
- **Spend.** Copilot bills per model call, and whether judge calls are billed is not stated. Retries after denials cost money. Codex has no MCAO cap.
- **Writes outside the worktree.** See "What an unattended Auto task can do". Claude Bash and Copilot shell are the gaps.
- **MCAO can promise.** The argv each level passes. The recorded level equals the flags passed. The hard limits, as far as each CLI enforces its documented flags.
- **MCAO cannot promise.** The judge's or classifier's decisions. That the review engaged. That shell is confined on Claude or Copilot. That a person chose Full (the daemon has no caller identity, per the `0033` comment). That secrets are unreadable to shell.

## Build order

**Prerequisites.** PR #75 merges first. Its files overlap all three PRs below: `copilot_adapter.rs`, `catalog.rs`, `executor.rs`, `api.rs`, `api_conversation_agent.rs`, `daemon/types.ts` and workspace tests. PR #76 changes only `scripts/ci` expected counts. Every PR that adds tests must regenerate those counts after its rebase. Count files will conflict, so merge count changes one at a time.

1. **PR A, daemon and wire (chats).** Argv per level for chats, all three agents. `permission` and `extended_groups` on `conversations.send`. Capability `permission_levels`. Failure codes. Migration `0035`, turn fields. Files: `crates/orchestrator-agents` (`catalog.rs`, adapters), `crates/orchestrator-daemon` (`api_conversations.rs`, `api_conversation_agent.rs`), `crates/orchestrator-store` (migration, turn writes). Rebase on #75 and #76. Size L.
2. **PR B and PR C, in parallel after A.** They share no Rust files.
   - **PR B, desktop chat UI.** Level control in the model picker chip and panel, Auto default for chats, Full confirmation, older-daemon chat notice, displayed level. Files: `apps/desktop/src/ui/views/workspace/*` (`WorkspaceView.tsx`, `ModelPicker`, new picker, chip and confirm components), `apps/desktop/src/daemon/types.ts`, fixtures, e2e. Can be built against fixtures while A is in review. Rebase on #75. Size M.
   - **PR C, tasks.** `permission` on `TaskSubmitParams` (`api.rs`). Contract field (`crates/orchestrator-core/src/contract.rs`). Executor mapping (`executor.rs`: `agent_command` around line 682, `spawn_agent`, `spawn_copilot`). Codex task pin. Spend cap behind the O2 switch (off by default). No Claude task cap (O7 open). Delivery denies (O8). Desktop task-form control in `ProjectDetail.tsx`, which reuses B's picker, so that part merges after B. Size M. Rebase on #75 and A, since C shares `copilot_adapter.rs` and `catalog.rs` with A.

Order: #75 and #76, then A, then B and C together. Every PR uses the org template, the Jira key, a Key Decisions section, a Workflow Runs section with real run links, and a Trivy scan that exits 0 before it is opened or updated.

## Tests

- **Argv, per level and agent.** Exact vectors, in A for chats and in C for tasks. Assert: no `--sandbox` on any Copilot argv (matches #75), the Auto vectors above, and Full dropping the listed flags.
- **Hard limits.** Below Full: no `--allow-all*`, `--dangerously-*`, `--ignore-rules`, `--autopilot`, `--experimental`, `--allow-all-mcp-server-instructions`, `--add-dir` or `--allow-all-paths`. No `COPILOT_*` name in any forwarded environment.
- **Caps and delivery.** Copilot runs carry `--max-ai-credits 30` only when the O2 switch is on (`AGENT_ORCH_COPILOT_CREDIT_CAP=1`). Claude task runs carry no spend cap (O7 open). Task runs carry the `git push` and `gh` denies (O8).
- **Codex task pin.** Restricted and Auto tasks pass `--sandbox workspace-write` and no `--dangerously-*` flag.
- **Wire, in-process.** Absent means `restricted` for chats and tasks. An unknown `permission` value is rejected with no turn or task. Codex Extended is refused with `permission_level_unsupported`. Capability gating. The idempotency key includes `permission`.
- **Store.** Migration `0035` is nullable, and the CHECK holds. Old turn rows read NULL. An old contract with no field reads as `restricted`.
- **Fake agent, process level.** The argv reaches the fake. Fake review-denial output yields `permission_review_denied`, and the fake is launched once. Fake error output yields `permission_review_unavailable`. These fakes test the parser only, not real CLI output.
- **Desktop Vitest.** Auto is the default for chats and tasks. The level is always in the model picker chip and panel. Only Restricted persists. Full needs confirmation each time. The displayed level comes from the daemon, and a mismatch shows an error. An older daemon falls back for chats and blocks tasks. Fixture mode never sends.
- **Desktop e2e, real daemon and fake agent.** A default chat sends `permission: auto`, and the fake's argv shows the Auto flags. Switching to Restricted changes the argv.
- **Cannot be tested without a signed-in CLI and paid calls.** Judge engagement, classifier behavior, the Codex reviewer, denial and error formats, Claude under `--print` on this account, whether `bypassPermissions` needs the allow flag, and network under `workspace-write`.

## Follow-up: live check (not a gate)

You run this with signed-in CLIs. It costs money. Its results go into a later revision of this file. Until it runs, the fail-closed rules above apply, and the chip says "automatic review, not a guarantee".

1. **Copilot, `-p` with `--assisted-approval`.** Does a shell request with no allow rule reach the judge? Do deny rules such as `shell(git push)` win over the judge? Does a denial show in `--output-format json`? Does the judge-off warning appear at `--log-level none`?
2. **Claude, `--permission-mode auto` with `-p`.** Does it run on this account (plan or API key)? Is a classifier call billed? Do denials appear in stream-json? Does `--disallowedTools "Bash(git push *)"` match? Is the bypass allow flag needed for Full?
3. **Codex, `--approve-for-me`.** What is the reviewer: a model or rules? What does it block? Do denials show in `exec --json`? Are the `workspace-write` roots the worktree? Is network off by default?

If an agent gives no denial signal, its reason codes are dropped for that agent, and the design says so.

## Not verified

- **Copilot.** Judge engagement for unmatched requests in `-p`. Whether deny rules win over the judge. Judge billing. Warning visibility at other log levels. Managed-policy behavior. Whether user MCP servers load under Auto in `-p`. The `--secret-env-vars` baseline.
- **Claude.** Whether auto mode needs a setting or account feature, and whether it works under `--print` in practice. The classifier's model and rules. Whether `bypassPermissions` needs the allow flag. Whether CLAUDE.md and auto-memory load under `--restricted`. The `--allowedTools` and `--disallowedTools` pattern syntax. How denials appear in stream-json.
- **Codex.** What `--approve-for-me` reviews, and whether it is a model. The default approval policy for exec. Read scope, writable roots and network under `workspace-write`. Config keys, including approval policy and web search for exec. Whether `--ignore-user-config` also drops the user's model and provider settings. Whether `AGENTS.md` loads. The Windows backend.
- **Not run.** Any CLI prompt or turn. `claude auto-mode` subcommands beyond help.

## Source tags

Help text is `<cli> --help` or `<cli> help <topic>` unless noted.

- **C-today.** `catalog.rs:850-867`, Codex chat argv. Codex task argv: `executor.rs:696`.
- **C-exec.** `codex exec --help`. Lists `--sandbox`, `--approve-for-me`, `--ignore-user-config`, `--ignore-rules`, `--ephemeral`, `--add-dir`, `--dangerously-bypass-approvals-and-sandbox`, `--dangerously-bypass-hook-trust`. No `-a` and no `--search`.
- **C-approve.** `codex exec --help`, `--approve-for-me`: "Route approval requests through automatic review using the workspace-write sandbox".
- **C-bypass.** `codex exec --help`, `--dangerously-bypass-approvals-and-sandbox`: "EXTREMELY DANGEROUS".
- **CL-today.** `catalog.rs:914-948`, `claude_code_adapter.rs:69-89`.
- **CL-tools.** `claude --help`, `--tools`, `--allowedTools`, `--disallowedTools`.
- **CL-auto.** `claude --help`, `--permission-mode` choices, including `auto`.
- **CL3.** `claude --help`, `--restricted`.
- **CL-bypass.** `claude --help`, `--allow-dangerously-skip-permissions`.
- **CL-bare.** `claude --help`, `--bare`.
- **CP-today.** `copilot_adapter.rs:330-358` (conversation and task argv) and `:481-503` (base argv), as on `motif-mcao/main`.
- **CP-allow.** `copilot --help`, `--allow-all-tools`: "Non-interactive mode cannot ask, so it denies tool requests that no allow rule (or `--assisted-approval`) approves."
- **CP-permissions.** `copilot help permissions`: "In autopilot and -p runs, a tool permission request that would prompt is denied."
- **CP-env.** `copilot help environment`: `COPILOT_ASSISTED_APPROVAL`, `COPILOT_ALLOW_ALL`, `COPILOT_AUTO_TIER`.
- **CP-sandbox.** `copilot help sandbox`: off by default, and the effects when disabled.
- **CP-billing.** `copilot help billing`, `copilot help limits` (`--max-ai-credits`, minimum 30, soft cap), and `copilot --help` (`--max-autopilot-continues`, default 5).
- **CP-config.** `copilot help config`, `defaultPermissionMode`: "assisted" is "a probabilistic LLM review".
- **Source files.** `api.rs:78-79` (`TaskSubmitParams`). `api_conversations.rs:90` (`deny_unknown_fields`). `api_conversation_agent.rs` (chat working directories). `sqlite_ops.rs:1712` (contract JSON). `executor.rs:592` (`event_fields`), `:682` (`agent_command`), `:696` (Codex task argv). `env_policy.rs` (no COPILOT entry). `0033:9` (audit CHECK).
