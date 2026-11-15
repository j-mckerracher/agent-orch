# Agent permission levels (DPES-1105)

Status: design only. Nothing here is implemented, approved, pushed or opened as a PR.

Basis:
- Code read at `motif-mcao/main`, commit `cc2b17d36`.
- The Copilot sandbox-gate work was read as the uncommitted working tree in `~/Code/agent-orch-worktrees/copilot-no-sandbox`. The branch head equals `main`, and 16 files are modified and uncommitted there. Nothing was edited.
- CLI facts come from help text only: `codex 0.167.2` (`codex --help`, `codex exec --help`), `claude 2.1.311` (`claude --help`), `copilot 1.0.109` (`copilot --help`, `copilot help permissions|sandbox|environment`, `copilot sandbox --help`). No CLI was run for a prompt.

## Decisions needed

1. **Where, and how often.** Recommend: chat gets the control in the composer row, next to the model picker (`WorkspaceView.tsx:514-521`). It applies per message, the same way the model choice does. Tasks get it in the submit form, set once per task and stored on the contract. Recommend against a settings-screen default, because a default would silently change messages already sent.
2. **Wire shape.** Recommend a sibling field `permission` on `conversations.send`, with its own capability `permission_levels`, not a field inside `selection`. `selection` picks a model and is refused for Copilot. Permission must still apply when no model is chosen, and the two must fail independently.
3. **Default and persistence.** Recommend Restricted on every launch. The choice lives in window state only, as the PR 74 model choice does. It is not persisted, so Full is never remembered without a fresh confirmation. No `localStorage`.
4. **Full on chat and on tasks.** Recommend allowing Full on both, each with its own confirmation. Chat turns are also unattended: `-p` mode denies any request that is not pre-approved (Copilot help), and Claude's `--permission-prompts none` denies what would prompt. So "unattended" does not separate a task from a chat. What does separate them is that a task outlives the window and runs later. Tasks should show the objective and worktree in the confirmation.
5. **What Full means.** The CLI's own default cannot be reached headless. Copilot denies unapproved requests in `-p` mode, and Claude's interactive default would prompt. Recommend: Full removes MCAO's restrictions and uses each CLI's explicit allow-all: Copilot `--allow-all`, Claude `--permission-mode bypassPermissions`. Keep `--no-ask-user` and `--permission-prompts none`. For Codex, pass no sandbox or approval flag, so `~/.codex/config.toml` decides. Recommend against Codex's `--dangerously-bypass-approvals-and-sandbox`. Its help text says it is "EXTREMELY DANGEROUS" and intended for externally sandboxed environments.
6. **Extended tool set.** Recommend selectable named groups from a fixed list, not free-form tool names. Groups: `git-read` (status, diff, log, show; default on), `web` (url fetch; off), `shell-named` (commands the user names; off; not offered for Codex), `memory` (Copilot only; off). Git write subcommands (`commit`, `push`, `reset`, `checkout`) are in no group. `git commit` runs repository hooks, which is code execution.
7. **Restricted is not today's behavior for Codex.** Codex chat runs `--sandbox read-only`. That still lets model-generated shell commands run; the help says the sandbox "executing model-generated shell commands" applies to them. Codex tasks pass `--approve-for-me` and no `--sandbox` (`executor.rs:696`), so no sandbox is pinned. Recommend: label Codex Restricted chat "read-only commands", not "no shell", and change the Codex task argv to `--sandbox workspace-write` without `--approve-for-me`. This changes today's task behavior and needs your sign-off.
8. **Coupling with `copilot-drop-sandbox-gate`.** The WIP removes the host gate but still passes `--sandbox` on every run (`copilot_adapter.rs:320` in the WIP). Recommend the gate applies only when a level allows shell, which means Extended with shell groups. Restricted denies shell, and Full runs without `--sandbox`, as the CLI does by default. Tell that work before it lands.
9. **Audit depth.** Recommend a nullable `permission_level` column on `conversation_turns`, via migration `0035`. NULL means "before this change". Store the level on the task contract and in its launch event. Do not extend `conversation_audit`: its CHECK allows only `clear`, `remove`, `restore` (`0033:9`), and SQLite needs a table rebuild to change a CHECK.
10. **Orchestrator escalation.** Recommend: the level is part of the task's authorization envelope. A proposal must carry exactly the envelope's level, the same equality rule `validate_proposal` applies to autonomy (`contract.rs:445-460`). Proposed child tasks default to Restricted. The executor reads the level only from the stored contract.

## What the code does today (verified in source)

| Fact | Source |
|---|---|
| Copilot chat: `--allow-tool read`, `--deny-tool write`, `--deny-tool shell`, `--deny-tool url`, `--deny-tool memory` | `copilot_adapter.rs:330-340` |
| Copilot task: `--allow-tool read`, `--allow-tool write(<worktree>/**)`, `--deny-tool shell|url|memory` | `copilot_adapter.rs:344-358` |
| Copilot every run: `--prompt=`, `--output-format json`, `--log-level none`, `--no-auto-update`, `--no-ask-user`, `--no-custom-instructions`, `--disable-builtin-mcps`, `--disallow-temp-dir`, `--sandbox`, `--usage-output-file`, `-C` | `copilot_adapter.rs:481-503` |
| Copilot sandbox gate: macOS needs `/usr/bin/sandbox-exec`; Linux needs bwrap and helpers; Windows refused by `WINDOWS_REASON` | `copilot_adapter.rs:51-52, 148-170`; `executor.rs:731` |
| Codex chat: `exec --json --sandbox read-only --skip-git-repo-check --ephemeral` plus selection | `catalog.rs:850-867` |
| Codex task: `exec --json --approve-for-me` plus selection; no `--sandbox`; cwd is the worktree (`working_dir`) | `executor.rs:696-702`; `spawn_agent` at `executor.rs:760` |
| Claude chat: `-p ... --permission-mode dontAsk --permission-prompts none --restricted --strict-mcp-config --no-session-persistence --tools ""` | `catalog.rs:914-948` |
| Claude task: as chat, plus `--tools Read,Edit,Write --allowedTools Read,Edit,Write` | `claude_code_adapter.rs:33, 69-89` |
| Send params reject unknown fields; `selection` is optional | `api_conversations.rs:56-58, 89-91` |
| Provider env forwarded: Codex `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_ORG_ID`, `OPENAI_PROJECT`, `CODEX_HOME`; Claude list at `:36` | `codex_adapter.rs:21-27`; `claude_code_adapter.rs:36` |
| Env policy has no `COPILOT_*` entry; Copilot starts with no provider variable | grep of `orchestrator-git/src/env_policy.rs` and `copilot_adapter.rs:9-10, 450` |
| Task autonomy is `ReadOnly`, `WorkspaceWrites` or `ApprovalRequired`; `tasks.submit` refuses `ReadOnly` | `contract.rs:56-65, 166`; `api.rs:467-469` |
| Send path blocks a choice on daemons without `conversation_selection`; passes choice to `surfaces.conversation.send` | `WorkspaceView.tsx:336-340, 353` |
| Existing tests pin today's argv | `crates/orchestrator-agents/tests/copilot_adapter.rs:243` (asserts no permission-widening flag), `:281`, `:292`; `catalog.rs` tests near `:1315-1345` |

## Argv by level, per agent

"Base" means Copilot `base_args` (`copilot_adapter.rs:481`). Every prompt stays one argument, and no shell is involved.

### GitHub Copilot

Sources: `copilot --help`; `copilot help permissions`; `copilot help sandbox`; `copilot help environment`.

- **Restricted chat**: base plus `--allow-tool read --deny-tool write --deny-tool shell --deny-tool url --deny-tool memory`. Unchanged. Help: deny rules win over every allow. Memory is off in prompt mode by default (`--enable-memory`: "disabled by default").
- **Restricted task**: base plus `--allow-tool read --allow-tool 'write(<worktree>/**)' --deny-tool shell --deny-tool url --deny-tool memory`. Unchanged. Help: file access is limited to the working directory and temp; `--disallow-temp-dir` drops temp.
- **Extended task**: base without `--deny-tool shell` and `--deny-tool url`. A deny beats an allow, so the deny must be removed, not added to. Then add `--allow-tool 'shell(git status)' 'shell(git diff)' 'shell(git log)' 'shell(git show)'` for `git-read`, `--allow-url <domain>` for `web`, and `--enable-memory` for `memory`. Keep `--sandbox`, which makes the gate apply.
- **Extended chat**: Restricted chat without the `shell` and `url` denies, plus the same allow rules. `write` stays denied.
- **Full (chat and task)**: base minus `--disable-builtin-mcps`, `--no-custom-instructions`, `--disallow-temp-dir` and `--sandbox`; plus `--allow-all`, which help defines as `--allow-all-tools --allow-all-paths --allow-all-urls`. Keep `--no-ask-user`, because nobody can answer headless. Without `--allow-all`, a `-p` run denies every request that is not pre-approved.
- **Env**: no change. Never forward `COPILOT_ALLOW_ALL`. Help: the value `true` "trusts the working directory without prompting, which loads that directory's skills, plugins, MCP servers, and hooks, including hooks that run shell commands."
- **Optional**: `--secret-env-vars <names>` removes named variables from shell and MCP environments (help). Candidate for Extended and Full on shell-capable runs. The baseline's contents are unverified.

### OpenAI Codex

Sources: `codex exec --help`; `codex --help`.

- **Restricted chat**: unchanged, `exec --json --sandbox read-only --skip-git-repo-check --ephemeral`. Shell commands still run under read-only. `exec` help has no tool allowlist and no approval flag; `-a` appears only at top level.
- **Restricted task (proposed change)**: `exec --json --sandbox workspace-write -- <objective>`, replacing `--approve-for-me`. Help: `workspace-write` is the policy for model-generated commands. Writable roots beyond the working directory are not stated. Do not pass `--add-dir`.
- **Extended**: `--sandbox workspace-write` with `--approve-for-me`. Help: approval requests are "routed through automatic review using the workspace-write sandbox". Codex exec offers no per-tool flags, so Extended is one step, not a group list. Web search is `--search` in top-level help only; its availability in `exec` is unverified.
- **Full**: `exec --json` plus selection only, with no sandbox or approval flag. The user's `~/.codex/config.toml` decides, which is what the CLI does by hand.
- **Env**: unchanged (`codex_adapter.rs:21-27`).

### Claude Code

Sources: `claude --help`.

- **Restricted chat**: unchanged. `--tools ""` disables all built-in tools (help).
- **Restricted task**: unchanged. Help for `--restricted`: it removes Bash, PowerShell and other code-running tools, and WebFetch, "unless --tools names them". It confines file tools to working directories. It lets only a person or a permission handler approve writes to settings, git and tool-configuration files.
- **Extended**: keep `--restricted --strict-mcp-config --permission-mode dontAsk --permission-prompts none`. Add the selected tools to `--tools` (for example `Read,Edit,Write,Bash,WebFetch`) and patterns to `--allowedTools` (for example `Bash(git status *)`, `Bash(git diff *)`, `Bash(git log *)`, `Bash(git show *)`). Help: `--tools` gates availability, and its `--allowedTools` example uses `Bash(git *)`. On Windows the shell tool is `PowerShell`, which `--restricted` also removes.
- **Full**: drop `--restricted`, `--strict-mcp-config`, `--tools` and `--allowedTools`. Keep `--permission-prompts none`. Add `--permission-mode bypassPermissions`, a listed choice. Do not use `auto`: help lists only `auto-mode` as a classifier inspect/reset subcommand. Do not use `--dangerously-skip-permissions` until its opt-in is confirmed, since help describes a separate allow flag.
- **Not usable**: `--bare`. Help: "OAuth and keychain are never read", which breaks the subscription login.

## What each level allows, and what it does not stop

- **Copilot Restricted**: tasks read the working directory and write only under the worktree. Chat writes nothing. No shell, url or memory. Does not stop: the user's own Copilot hooks in `~/.copilot`, which run on every run (comment at `copilot_adapter.rs` base args; `copilot --help` has no flag to disable them). Built-in file edits are not OS-sandboxed (`copilot help sandbox`), so the write rule is the only control on edits.
- **Copilot Extended**: named commands and url hosts only. Does not stop: side effects of an allowed command, or arguments, because a shell rule matches command name and subcommand, not arguments (help). On macOS the sandbox limits what allowed commands can write.
- **Copilot Full**: everything the CLI can do, unsandboxed, outside the worktree. Does not stop: prompt injection, the account's own credentials and login store, or anything the account can reach.
- **Codex Restricted**: chat writes are denied, but commands run read-only. Tasks write within the workspace (roots unverified). Does not stop: read-only shell commands; reads the sandbox allows (read scope not in help); `OPENAI_API_KEY` in the child environment, readable by any turn that can run commands.
- **Codex Extended and Full**: write and network scope come from the sandbox and user config. Unverified (see the last section).
- **Claude Restricted**: chat has no tools. Tasks have three file tools. Does not stop: CLAUDE.md or auto-memory loading, which `--restricted`'s help does not list; `--bare` would skip CLAUDE.md but breaks login. Edits inside the worktree, including the `.git` pointer file.
- **Claude Extended**: named Bash patterns, plus WebFetch if listed. Does not stop: an allowed pattern doing something unexpected, such as a git option that writes. Pattern matching is unverified.
- **Claude Full**: everything. Full drops `--restricted`, which ignores user, project and local settings, so the user's own settings and hooks load. Does not stop: prompt injection and the account's credentials.

## Windows and macOS

- **Copilot macOS**: the gate is `/usr/bin/sandbox-exec` (Seatbelt backend, help). Restricted and Extended-with-shell need it. Full does not use it.
- **Copilot Windows**: today's code refuses it (`WINDOWS_REASON`). The WIP drops the refusal but keeps `--sandbox`. Help: Windows uses MXC ProcessContainer, supported on listed Windows versions only, and on an unsupported host an explicit `--sandbox` makes "every sandboxed shell command, MCP/LSP server, and extension fail". Restricted denies shell, so it is probably unaffected, but this is unverified on Windows. Extended-with-shell must refuse on Windows until verified.
- **Copilot shell rules on Windows**: PowerShell rules match the name as typed, not its aliases (help). PowerShell names need their own entries in any group.
- **Copilot path rules**: `[`, `*` and `?` are glob characters in `write()` and `read()` patterns. A worktree path containing them needs bracket escaping (help). Backslash is a separator, not an escape.
- **Codex Windows**: help gives no backend description. Unverified.
- **Claude Windows**: `PowerShell` appears in `--restricted`'s list, so Extended lists differ by platform.
- **Linux**: Copilot needs bwrap 0.5.0 and helpers (`copilot_adapter.rs:55-63`). Windows has no bwrap. Out of scope for the first release.

## Orchestrator-created tasks (question h)

The answer is no. The rules:
- A task's level is set at submission and stored on the contract, like `autonomy` (`contract.rs:166`).
- A proposal carries its own level and is accepted only if it equals the envelope's level. A different level is denied with a reason, under the same equality rule as autonomy (`contract.rs:445-460`).
- The executor takes the level only from the stored contract, never from agent output, as it does for the model choice (`executor.rs:586`).
- Proposed child tasks default to Restricted, not to the parent's level.
- Coordinator notes take no level. The desktop already blocks a choice for them (`WorkspaceView.tsx:336-338`).
- Refuse rather than widen. `tasks.submit` already does this for `ReadOnly` (`api.rs:467-469`).

## Wire contract (question e)

- `conversations.send {target, text, idempotency_key, selection?, permission?}`. `permission` is `"restricted" | "extended" | "full"`. With `extended`, add `extended_groups: ["git-read", "web", "shell-named", "memory"]`, a subset. An unknown group is `InvalidParams` and records nothing, the same rule `selection` follows (`docs/m7-08-conversation-service.md`, "Per-message choice").
- Capability `permission_levels`, advertised only once the daemon maps the levels.
- **Restricted** omits the field. The request is then byte-identical to today, and an older daemon is unaffected.
- **Non-Restricted against an older daemon**: the desktop blocks the send with a message, as `WorkspaceView.tsx:339-340` does for selection. It is never dropped.
- **Older daemon receiving the field**: `ConversationSendParams` has `deny_unknown_fields` (`api_conversations.rs:90`). It rejects the request with `InvalidParams` and records nothing. A Full request therefore cannot run as Restricted on an older daemon, and the reverse cannot happen.
- **Failure codes** on `turn_failed`, following the `selection_*` family: `permission_agent_unsupported`, `permission_sandbox_unavailable`, `permission_group_unavailable`.
- **Idempotency**: the send key derives from text, selection and permission (the PR 74 approach). A Restricted send with no field keeps today's fingerprint.
- **Fixture mode**: the picker's fixture path never calls `surfaces.conversation.send`. Tests assert no IPC call, and the SIMULATED notice applies, the same as the model picker (PR 74: "Fixture mode is unchanged and still never sends").

## Persistence, confirmation and indicator (questions b and c)

- **Default**: Restricted. Window state only. Resets on app restart and on window close. No `localStorage` and no daemon storage. The daemon stores nothing beyond the turn record.
- **Full confirmation**, shown before a chat send or a task submit. One sentence per agent, and the confirm button names the consequence. Focus starts on Cancel.
  - Copilot: "Copilot can run any shell command, edit any file your account can, reach any URL, and use Copilot's own settings and hooks. No OS sandbox is applied. It cannot ask you anything during the run."
  - Codex: "Codex uses your own ~/.codex settings with no MCAO limits. It may run commands and write files your account allows, under the sandbox your settings choose."
  - Claude: "Claude Code skips permission checks (bypassPermissions), uses your own Claude settings and hooks, and may run any tool your account allows."
- **Tasks**: the confirmation shows the objective and the worktree path, and says the task runs unattended after submit.
- **Indicator**: while non-Restricted, a chip beside the composer shows the level and group names. It uses text and color, never color alone. The transcript shows the level on each turn. Full reads "Full access" in the chip.

## Extended groups (question d)

Recommended selectable groups, with the default set in bold.

| Group | Copilot | Claude | Codex | Default |
|---|---|---|---|---|
| `git-read` | `shell(git status)`, `shell(git diff)`, `shell(git log)`, `shell(git show)` | `Bash(git status *)` and the same for diff, log, show | none (workspace-write only) | **on** |
| `web` | `--allow-url <domain>` | add `WebFetch` to `--tools` | `--search`, unverified for exec | off |
| `shell-named` | user-named `shell(...)`, never git write subcommands | user-named `Bash(...)` | not offered | off |
| `memory` | `--enable-memory` | not offered (unverified) | not offered | off |

Free-form tool names are not offered, because the flags for Codex do not exist and the combinations would need separate tests.

## Audit trail (question f)

- **`conversation_turns`** (`0031`): add nullable `permission_level` with a CHECK on the three values, in migration `0035_conversation_permission.sql`. The latest migration is `0034_idempotency_scrub.sql`, and no other worktree has a `0035`. Write it when the turn row is created, next to `runtime_name`. NULL means before this change. The Activity view reads it.
- **Tasks**: the level is a contract field with a serde default of Restricted, and omitted when unset, like `model_selection` (`contract.rs:560-575`). It is also recorded in the task's launch event (`Job::event_fields`, `executor.rs:592`).
- **`conversation_audit`**: unchanged in v1. Its CHECK is `clear | remove | restore` (`0033:9`), and append-only triggers block UPDATE and DELETE. Recording escalations there means rebuilding the table and recreating its triggers.
- **Actor**: NULL. The daemon has no caller identity (`0033` comment). The daemon cannot prove that a person chose Full. Stated as a limit.

## Threat notes (question k)

- **Prompt injection from repository content.** Restricted limits damage to the worktree, and output is reviewed before delivery. Extended and Full let injected text drive any command the level allows. `git-read` is read-only except for options such as `git diff --output`, which is unverified and needs a test.
- **Secrets in the environment.** Codex and Claude children receive provider keys (`codex_adapter.rs:21-27`, `claude_code_adapter.rs:36`). A turn that can run commands can read them. Copilot starts from the baseline. Recommend `--secret-env-vars` for Copilot shell-capable runs. MCAO cannot remove Codex's key without possibly breaking its login (unverified).
- **`COPILOT_ALLOW_ALL=true`.** Must never be forwarded. Its effect is above.
- **User hooks.** Copilot's `~/.copilot` hooks run at every level, and Restricted does not stop them. Claude's `--restricted` ignores user settings, but Full removes it.
- **Writes outside the worktree.** Restricted tasks: a Copilot path rule; Claude confines file tools to working directories; Codex workspace-write roots are unverified. Full: anywhere the account can write.
- **What MCAO can promise.** The exact argv each level passes. The level recorded on a turn or task is the one passed. Restricted chat passes no shell, url or write tool to Copilot and Claude, per help.
- **What MCAO cannot promise.** That each CLI enforces its flags (fake-CLI tests check argv only). That user-level CLI config adds no capability at Codex and Claude Full. That a person chose Full (no caller identity). That a shell-capable turn cannot read secrets.

## Tests (question i)

| Layer | Proves | Cannot prove |
|---|---|---|
| Rust argv tables (`catalog.rs`, `copilot_adapter.rs`, `claude_code_adapter.rs`) | Exact argv per level and agent. Restricted matches today, except the Codex task change. | That the CLI enforces the flags |
| `tests/copilot_adapter.rs:243` | Today's rule "no permission-widening flag" becomes "only Full passes `--allow-all`". | |
| In-process `api_conversations_tests.rs` | Validation, `deny_unknown_fields`, omitted means Restricted, capability block, idempotency fingerprint | |
| Process level (`tests/conversation_runtime.rs`, `examples/fake_agent.rs`) | The level reaches the fake's argv. An invalid group launches nothing. | |
| Store | Migration `0035` is nullable. Turn rows record the level. Old rows read NULL. | |
| Contract | A proposal with another level is denied. Child tasks default to Restricted. | |
| Vitest (desktop) | Default is Restricted. Full needs confirmation. No `localStorage` writes. Chip present. Blocked on missing capability. Fixture never calls send. | |
| Desktop e2e (real daemon, fake agent) | Choose Full, confirm, send, and the argv contains `--allow-all`. | |
| Live, opt-in, not run | Canary file outside the worktree. Forbidden command. Copilot `-p` denial. Claude `--tools ""`. Codex read-only. | Needs a signed-in CLI, costs money, needs your approval |

Cannot be tested without a signed-in CLI: whether Copilot denies in `-p` as help says; argument matching for `shell(...)`; Claude pattern matching; Codex read and write scope and the `exec` approval default; Windows MXC behavior.

## Rollout (question j)

Ordered, with rough sizes:
0. This document, as a docs-only PR. Optional. Not opened now.
1. `copilot-drop-sandbox-gate` lands first, or is coordinated so the gate becomes level-dependent. Size S.
2. PR A: Copilot argv per level. Refactor base into level builders. Restricted output is identical. Update `tests/copilot_adapter.rs:243`. Size M.
3. PR B: Claude and Codex argv per level, including the Codex task pin. Needs decision 7. Size M.
4. PR C: wire field, capability, failure codes, migration `0035`, contract field and equality check, turn record, Activity display. Size M to L.
5. PR D: desktop picker, confirmation, chip, capability block, fixture rules, tests. Size M.
6. PR E: opt-in live verification tests, run only with your approval. Size S.

Every PR: run the Trivy scan (`trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .`) before opening, use the PR template from `mc-innersource/.github`, and do not push without approval.

## Not verified

- **Copilot**: `shell(...)` argument matching; `-p` denial in practice; the contents of the `--secret-env-vars` baseline; whether hooks run under `--allow-all`.
- **Codex**: the `exec` default approval policy; whether the config key `approval_policy` exists (not in help); read scope and writable roots of the sandboxes; network under workspace-write; `--search` for exec; the Windows backend.
- **Claude**: `bypassPermissions` with `-p` and `--permission-prompts none` in practice; the `--dangerously-skip-permissions` opt-in; pattern syntax such as `Bash(git diff *)`; whether CLAUDE.md loads under `--restricted`; memory under Restricted; `auto` mode.
- **Not run**: no CLI prompt; no `codex features list` (outside the help-only rule); no live tests.
- **Not read**: the contents of `claude_code_adapter.rs:36`; the full Copilot spawn path (`spawn_with_host`); the WIP diff beyond the argv lines, since it is uncommitted.
- **Open product conflict**: `docs/ui/wireframe.md` decision D-5 (line 247) ships the model picker only as a placeholder or hidden in the first release. A live permission control needs the same release decision. Not resolved here.
