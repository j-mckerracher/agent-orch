# M6-01 Threat model and dependency security review

Scope: board item M6-01, dependencies and plugins, environment allowlists,
untrusted inputs, authorization, and protected delivery boundaries. This
extends the M0-04 model in [`../security.md`](../security.md); read that
first. Base: M5-05 round 3 (`efa5e8af3`). Method: code reading of every
trust boundary, `cargo audit`, `cargo deny`, `npm audit`, then fixes with
tests for the in-scope findings.

## 1. Findings

| ID | Severity | Area | Finding | Status |
|----|----------|------|---------|--------|
| F1 | High | Env allowlist | Agent and validation children inherited the full parent environment (plan section 13 requires an allowlist). Any API key, cloud credential, or `AGENT_ORCH_DATA_DIR` (which locates the IPC token) reached agent-controlled code. | Fixed: `orchestrator_git::env_policy`, section 3. |
| F2 | Medium | Authorization | `tasks.submit` accepted `AutonomyPolicy::ReadOnly` but the executor always launches a writing agent, so the policy the user chose was not the policy enforced. | Fixed: the daemon returns `Forbidden`. |
| F3 | Low/Medium | Untrusted input | `contract::validate` had no size or NUL bounds on objective, agent, ids, dependencies, or verification commands. Oversized or NUL-bearing values failed late (spawn, SQL, argv) instead of at the boundary. | Fixed: `contract::limits`. |
| F4 | Low | Untrusted input | `Repository::resolve_base_ref` passed the caller's refspec to `git rev-parse` unchecked; a leading `-` is an option. Delivery already rejected this; worktree creation and verification did not. | Fixed: rejected in `resolve_base_ref`. |
| F5 | Medium | Dependency | RUSTSEC-2026-0459: `libsqlite3-sys` 0.37.0 bundles SQLite older than 3.54 (uninitialized memory returned for a crafted db plus `-wal` pair). Patched in `libsqlite3-sys` 0.39, but `sqlx-sqlite` 0.9.0 (latest) requires `< 0.38`. | Not fixable here. Accepted and recorded in `deny.toml`; residual risk R6. |
| F6 | Info | Dependency policy | No machine-checked policy existed. | Added `deny.toml`; `publish = false` on every crate. |
| F7 | Info | Dependency | Tauri tree warnings: `proc-macro-error` (unmaintained) and `glib` 0.18.5 (unsound `VariantStrIter`). Both are transitive through the GTK/Linux backend; the shipped targets are macOS and Windows. | Accepted; not compiled on supported targets. |
| F8 | Medium (Windows) | Authorization | The Windows named-pipe client opens the pipe without `SECURITY_SQOS_PRESENT` and does not verify the server's identity before sending the token; the Windows data-directory ACL and token owner are not validated as on Unix (already noted in `windows_transport.rs`). A local process that creates the pipe before the daemon could receive the token. | Not fixed: Windows-only code that cannot be run or tested on this Mac. Residual risk R11 with a concrete follow-up. |

Reviewed and found sound (no change): IPC token and framing, daemon request
parsing, diff path validation, delivery gating, Tauri capability and CSP, and
the Claude adapter flags. Details in section 2.

## 2. Trust boundaries

Assets: the user's repositories, the IPC token, provider credentials in the
user's environment, and the integrity of the approval record.

| Boundary | Untrusted side | Control | Evidence |
|----------|----------------|---------|----------|
| IPC transport | Other local processes and users | 32-byte random token, constant-time comparison; Unix: token file 0600 in a 0700 data directory with identity checks, sockets 0600; Windows: named pipe and token file with a protected DACL for the current user SID, remote clients rejected, first-instance flag (but see F8); frames capped at 1 MiB | `orchestrator-ipc` tests (45) |
| Data directory contents | Other local users | Unix: the daemon, the CLI, the store and the desktop launcher create the data directory 0700 and `store.sqlite`, its `-wal` and `-shm` files, `.orchestrator.lock` and `daemon.log` 0600, and tighten ones that already exist (F-Q2). Windows: nothing is set; the directory inherits the ACL of the user profile (`%LOCALAPPDATA%`), which is private to the user by default. That is not enforced or verified (R11). | store, daemon and desktop-host tests (Unix only) |
| RPC parsing | Any authenticated client | `deny_unknown_fields` on `RpcRequest`, `Handshake`, and every params struct; capabilities negotiated at handshake; idempotency keys validated | daemon `api` tests |
| Task contract | UI and CLI input | `contract::validate`: required fields, size caps, no NUL (F3); the objective refuses control characters other than newline, carriage return and tab, and bidirectional formatting characters; `max_retries` and `max_repairs` are capped at 10 (F-Q7, F-Q9); `tasks.submit` refuses `ReadOnly` (F2) and the UI no longer offers it (F-Q6) | core and daemon tests |
| Repository paths | Task input | `task_repositories` forbids the primary checkout; diff paths reject `..`, absolute paths, leading `-`, and require canonical containment | git and daemon tests |
| Git refs | Task input | `resolve_base_ref` rejects option-like and control-character refs (F4); worktree branch names go through `sanitize_task_id`; delivery refs are limited to `refs/heads/orchestrator/review/` | git tests |
| Agent processes | Agent output and agent actions | Environment allowlist (F1); the supervisor caps each captured output line at 64 KiB; Claude runs `dontAsk`, restricted, strict MCP config, tools Read/Edit/Write only | supervisor and adapter tests |
| Validation commands | Contract content | Deny by default: exact argv plus digest must match a trusted entry; environment allowlist (F1) | git verification tests |
| Delivery | A completed agent attempt | Requires an approval, a passing verification run, and a digest-bound approval; only `report_only` and `local_review` exist; the approval actor is fixed server-side as `local-user` | store and daemon delivery tests |
| Desktop shell | The web view | Capability `default` allows only daemon open, send, and close; no shell, fs, or other plugins; strict CSP; the bridge forbids `handshake` and `shutdown` and caps connections at 8; the sidecar path override is debug-only | `apps/desktop/src-tauri` |

## 3. Environment allowlist (F1)

Implementation: `crates/orchestrator-git/src/env_policy.rs`, applied by the
supervisor (agent launches) and by `verification::run_command` (validation
commands). Both call `env_clear()` and then add only allowed names.

A child receives:

1. A baseline: `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, locale (`LANG`,
   `LANGUAGE`, and the exact `LC_*` names `LC_ALL`, `LC_CTYPE`, `LC_COLLATE`,
   `LC_MESSAGES`, `LC_MONETARY`, `LC_NUMERIC`, `LC_TIME`, `LC_PAPER`,
   `LC_NAME`, `LC_ADDRESS`, `LC_TELEPHONE`, `LC_MEASUREMENT`,
   `LC_IDENTIFICATION`; an arbitrary `LC_*` name is not forwarded), `TERM`,
   `TZ`, temp and `XDG_*` locations, proxy and CA
   settings (`*_PROXY`, `SSL_CERT_FILE`, `SSL_CERT_DIR`,
   `NODE_EXTRA_CA_CERTS`), and the Windows system and profile variables.
2. Provider names added by the adapter. Codex: `OPENAI_API_KEY`,
   `OPENAI_BASE_URL`, `OPENAI_ORG_ID`, `OPENAI_PROJECT`, `CODEX_HOME`. Claude:
   `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`,
   `CLAUDE_CONFIG_DIR`. Pi: none. Pi users list the variables they need.
3. Validation commands also receive toolchain names (`CARGO_HOME`,
   `RUSTUP_HOME`, `RUSTUP_TOOLCHAIN`, `CARGO_TARGET_DIR`, `RUSTFLAGS`, `CI`,
   `NODE_PATH`, `NVM_DIR`, `VOLTA_HOME`, `GOPATH`, `GOROOT`, `GOCACHE`,
   `GOFLAGS`, `VIRTUAL_ENV`, `PYTHONPATH`, `JAVA_HOME`, `DOTNET_ROOT`).
4. Names the user lists in `AGENT_ORCH_ENV_PASSTHROUGH` (comma separated).
   This is the documented escape hatch for any other variable.

Rules: names that start with `AGENT_ORCH_` are never forwarded, even when
listed, because the data directory path locates the IPC token. Names match
case-sensitively on Unix and case-insensitively on Windows. A list entry must
be a bare variable name: `NAME=value` and `*` are rejected, and so is any
other invalid entry. Only names are handled. A rejected entry is reported on
stderr (`daemon.log` under the desktop host) by its position and, when the
text before any `=` is a plausible name (ASCII letters, digits and `_`, at
most 64 characters), that name, plus a fixed reason. The rest of the entry,
including any value, is never logged or recorded (F-Q1).

Passthrough is an explicit trust decision. A name listed there is forwarded
as is, including loader and git variables such as `LD_PRELOAD`,
`DYLD_INSERT_LIBRARIES`, `GIT_SSH_COMMAND` and `GIT_CONFIG_*`, which change
what the child executes. List only names whose values you trust.

Provider names that are not forwarded by default and are not checked against
a provider's documentation here: other Codex, Claude Code, Bedrock or Vertex
variables (for example a Claude OAuth token or cloud credentials) fail
closed. List the names you need in `AGENT_ORCH_ENV_PASSTHROUGH` (R8).

Limit: this narrows what a child inherits. It does not contain it (R1).

## 4. Dependency and plugin review

Tool availability (all run on the Mac with network access; raw output is in
the evidence directory):

| Tool | Available | Version | Result |
|------|-----------|---------|--------|
| `cargo audit` | Not preinstalled; installed into a private tool root for this run | 0.22.2, 1430 advisories | Workspace lockfile (187 crates): 1 vulnerability, RUSTSEC-2026-0459 (F5). Tauri lockfile (413 crates): 0 vulnerabilities, 2 warnings (F7). |
| `cargo deny` | Not preinstalled; installed into the same root | 0.20.2 | New `deny.toml`: workspace and Tauri tree both report `advisories ok, bans ok, licenses ok, sources ok`. The Tauri run prints one `advisory-not-detected` warning for the F5 ignore, which is correct because that tree has no SQLite. |
| `npm audit` | Present | npm 11.20.0 | `apps/desktop`: 0 vulnerabilities, with and without `--omit=dev`. |

Rust: all direct dependencies are pinned with `=`; `Cargo.lock` is committed
and every command here uses `--locked`. Licenses are permissive (MIT,
Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0, Unlicense, MIT-0, CC0-1.0, 0BSD,
and MPL-2.0 in the Tauri tree only); `r-efi` offers LGPL-2.1-or-later only as
an `OR` alternative next to MIT and Apache-2.0. The only `build.rs` is the
Tauri shell's `tauri-build` call, which lists the three allowed app commands.
Only the crates.io registry is allowed; there are no git dependencies.

npm: every dependency is pinned to an exact version and `package-lock.json`
is committed. Production dependencies are `react`, `react-dom`, and
`@tauri-apps/api`. One optional package has an install script (`fsevents`, a
macOS file-watch helper pulled in by vite for development); it is not in the
shipped bundle.

Plugins: the Tauri host registers no plugins and its capability grants no
shell, fs, http, or opener permissions. There is no auto-updater. Agent
"plugins" are the three adapters (Codex, Claude Code, Pi). Their programs are
chosen by `AGENT_ORCH_{CODEX,CLAUDE,PI}_PROGRAM` in the daemon's own
environment, which a child never sees after F1.

Policy going forward: run `cargo deny --locked check` for the root and
`cargo deny --manifest-path apps/desktop/src-tauri/Cargo.toml --locked check`,
plus `npm audit`, before each release. M6-02 owns wiring this into CI; this
change does not edit workflows.

## 5. Residual risks

| ID | Risk | Why it remains |
|----|------|----------------|
| R1 | An agent runs as the user's OS account. It can read `daemon.token` and the data directory, call IPC (including `approvals.resolve`), and read any file the user can. Env scrubbing is a speed bump only. | Needs an OS sandbox or a separate account; out of scope for v1 (M0-04 section 5). |
| R2 | A worktree is not a sandbox. | Same as R1. |
| R3 | Codex runs with `--approve-for-me`; `ApprovalRequired` is not enforced per tool call. The executor treats it as workspace-writes with approval before delivery. The CLI hardcodes workspace-writes. | Adapter capability limit. The daemon refuses `ReadOnly` with `Forbidden` and the UI no longer offers it. |
| R4 | No signed release or updater. The Tauri bundle target is `all` and unsigned. | Release engineering (M6-02 and M6-03). |
| R5 | Git subprocesses inherit repo-location variables (`GIT_DIR`, `GIT_WORK_TREE`, and similar) from the user's own environment. About ten spawn sites in git, cli, and daemon. | Set by the user, not an attacker; the daemon's environment is trusted. |
| R6 | RUSTSEC-2026-0459 (F5). Exposure needs a crafted database plus `-wal` pair in the per-user data directory, which means the attacker already has the user's file access. | Blocked on a `sqlx` release. Re-check on each sqlx update; remove the ignore when `libsqlite3-sys >= 0.39` resolves. |
| R7 | Proxy URLs in the baseline can embed credentials. | Deliberate: agents must work behind a proxy. |
| R8 | The Pi adapter forwards no provider key by default, and the Codex and Claude lists cover only the names in section 3. | Fails closed. List the needed names in `AGENT_ORCH_ENV_PASSTHROUGH`. |
| R9 | Windows-only code paths were checked on the Mac only by `cargo check --target x86_64-pc-windows-gnu --tests` for git, runtime, core, agents and ipc. The store and daemon cannot be cross-compiled from a Mac (`libsqlite3-sys`); the Windows branch of the store's `private.rs` was compiled alone with `rustc --target x86_64-pc-windows-gnu -D warnings`, and the remaining store changes are `#[cfg(unix)]` additions. | See section 6. |
| R10 | Fixed in M6-01 remediation: the two daemon tests held one connection across a wait for the agent, and the server closes a connection idle for 2 s, so the next call got `Frame(Io(BrokenPipe))`. The tests now reconnect after each wait. A 2.5 s injected sleep fails the old tests and passes the new ones. | None; the IPC crate is unchanged. |
| R11 | F8: Windows pipe-server identity is not verified by the client, and the Windows data-directory ACL and token owner are not validated. The 0700 and 0600 modes of F-Q2 have no Windows equivalent in the code: no DACL is set on the data directory, the database files or `daemon.log`, so they rely on the inherited profile ACL. | Fix needs native Windows testing. Follow-up: open the pipe with `SECURITY_SQOS_PRESENT \| SECURITY_IDENTIFICATION`, compare `GetNamedPipeServerProcessId` with the daemon pid or its owner SID with the current user SID, and validate or set the data-directory DACL like the Unix identity checks. |
| R12 | On Windows a whole command line is limited to 32,767 characters. A prompt near the 64 KiB objective cap (passed as one argument to Codex and Claude Code), or large verification arguments, makes the launch fail with a spawn error after validation succeeded. Unix limits are far larger. | Fails closed. Not fixed: the adapters take the prompt as an argument, and passing it by stdin is not verified for either provider. Keep objectives short on Windows or point the agent at a file in the repository. |

## 6. Windows verification

Run natively on Windows from a clean checkout of the final commit, with
`--locked`. Expected counts are the passing counts observed on the Mac for
the same filters; Windows must match them unless noted.

| # | Command | Expected |
|---|---------|----------|
| 1 | `cargo test --locked -p orchestrator-git --lib env_policy` | 11 passed (8 before the M6-01 remediation, plus the wildcard, exact-locale and no-value-in-report tests). Includes the name-case test, which asserts case-insensitive matching on Windows. |
| 2 | `cargo test --locked -p orchestrator-git --lib validation_commands_do_not_inherit_unlisted_environment` | 1 passed. |
| 3 | `cargo test --locked -p orchestrator-git --lib resolve_base_ref_refuses_option_like_and_control_character_refs` | 1 passed. |
| 4 | `cargo test --locked -p orchestrator-runtime --test env_isolation` | 1 passed. Runs `cmd.exe /C set`; confirms `PATH`, the launcher-listed name, and the passthrough name arrive, and that an unlisted secret and `AGENT_ORCH_DATA_DIR` do not. |
| 5 | `cargo test --locked -p orchestrator-core --lib contract` | 17 passed (16 before the remediation, plus the control and bidi character test; the retry and repair caps extend existing tests). |
| 6 | `cargo test --locked -p orchestrator-daemon --bins submit_` | 2 passed. |
| 7 | `cargo test --locked -p orchestrator-git --lib` | 47 passed (44 plus the 3 new env_policy tests). |
| 8 | `cargo test --locked -p orchestrator-ipc --lib` | 45 passed (unchanged). |
| 9 | `cargo test --locked -p orchestrator-daemon --test lifecycle` | Exit 0 (custom harness that reports 0 tests; every STEP must print through the last). The agent pid-file variable was renamed to `LIFECYCLE_AGENT_PID_FILE` and is forwarded through `AGENT_ORCH_ENV_PASSTHROUGH`. |
| 10 | `cargo test --locked -p orchestrator-cli --test portable_acceptance --test cleanup_safety --test engine_reconciliation_gate --test crash_concurrency` | Exit 0. Mac: 2, 4, 22, 6 passed. These fixtures now pass their fake-agent and git-config variables through `AGENT_ORCH_ENV_PASSTHROUGH`; a Windows failure here means a variable the fixture needs is missing from that list. |
| 11 | `cargo test --locked -p orchestrator-store --lib` | 32 passed. The Mac count is 37; the five tests added by the remediation (`private::` x3, the data-lock directory test and the database-file test) are `#[cfg(unix)]`, so they do not run on Windows. |
| 12 | `cargo test --locked --workspace --no-fail-fast` | Exit 0. Mac total: 654 passed, 0 failed, 1 ignored before the remediation. After it the Mac total is higher by the new tests, of which the five store tests and the whole daemon `tests/daemon.rs` file (10 tests, one new) are Unix-only. No Windows total was ever measured, so judge this item by exit code and by items 1 to 11. |

Items 1 to 6 are the new tests. Items 7 to 12 show that existing Windows
coverage still passes with the allowlist in force. If item 4 or 10 fails,
first check that `SystemRoot`, `ComSpec`, and `PATHEXT` reach the child;
`cmd.exe` and most Windows tools do not start without them.

## 7. Evidence

Raw outputs live in `.agent-worktrees/evidence-m6-01-20261110T045853Z/`
(first pass) and `.agent-worktrees/evidence-m6-01r-20261110T060117Z/`
(remediation, including `REMEDIATION-REPORT.md`):
`cargo-audit-workspace.txt`, `cargo-audit-tauri.txt`,
`cargo-deny-workspace.txt`, `cargo-deny-tauri.txt`, `npm-audit.txt`,
`workspace-test-final.log`, `vitest.log`, `e2e.log`, `clippy-final.log`,
`windows-cross-check.log`, and `OWNER-REPORT.md`.
