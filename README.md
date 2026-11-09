# Agent Orchestrator

Cross-platform (macOS, Windows) desktop application for delegating software
engineering work to autonomous coding agents. Stack: Rust, Tauri 2, React,
TypeScript, SQLite. The full design lives in
[docs/technical-implementation-plan.md](docs/technical-implementation-plan.md);
`board.json` tracks which parts of it are actually implemented.

Updated: 2026-11-07T09:46:00-07:00

## What exists today (scaffold)

- Cargo workspace with eight engine crates. Each crate is a **boundary
  placeholder**: it compiles, but exposes no public API yet. `orchestrator-core`
  intentionally has zero dependencies.
- `orchestrator-cli` binary: prints that task execution is not implemented and
  exits nonzero. It has no subcommands.
- `orchestrator-daemon` binary: prints a placeholder message and exits. It has
  no IPC server.
- Desktop frontend (`apps/desktop/src`): one React page that builds with Vite.
  It talks to no engine.
- Tauri host (`apps/desktop/src-tauri`): one window and a no-op `ping` command
  exposed to the invoke bridge. The bridge has not been exercised, and the host does not call the engine.

Nothing here executes an agent task, persists state, or isolates a Git
worktree. Those are the plan's later milestones.

## Layout

```
Cargo.toml                 Root workspace. Header comment documents the
                           dependency direction rule.
crates/
  orchestrator-core/       Contract root. MUST NOT depend on tokio, Tauri,
                           SQLite, Git, or any agent provider.
  orchestrator-agents/     Intended: Codex / Claude Code / Pi protocol adapters.
  orchestrator-git/        Intended: repo validation, worktree lifecycle,
                           diff capture, local delivery.
  orchestrator-store/      Intended: SQLite (WAL) persistence and event journal.
  orchestrator-runtime/    Intended: process supervision, cancellation, recovery.
  orchestrator-ipc/        Intended: authenticated local JSON-RPC protocol.
  orchestrator-daemon/     Intended: long-lived engine process; composes the
                           other crates; launches on demand.
  orchestrator-cli/        Intended: standalone CLI client; engine runs
                           in-process, no Tauri dependency.
apps/desktop/
  src/                     React + TypeScript frontend (Vite).
  src-tauri/               Tauri 2 host. Standalone Cargo package, NOT a
                           workspace member; see its Cargo.toml header.
docs/                      Plan and prompts.
board.json                 Shared work record (coordinator-owned).
```

### Intended boundaries (not implemented)

Dependency direction (plan section 3): `core` defines contracts;
`agents`/`git`/`store`/`runtime` implement them; `ipc` and `daemon` add
process boundaries; `cli` and the desktop app are clients. The desktop host
is intended to forward to the engine daemon over local IPC, so closing the
window cannot kill running agent work — that forwarding is not built yet.

`apps/desktop/src-tauri` is deliberately not a workspace member: it is a
standalone package with its own `Cargo.lock`, so the engine builds and ships
without desktop toolchain setup and the two build graphs never mix.

## Building

Requires a Rust toolchain, Node.js, and platform Tauri prerequisites (on
Windows the Visual Studio C++ workload, plus a WebView2 runtime for the dev
window). All commands run from the repository root.

```sh
# Engine workspace (builds the eight crates in crates/)
cargo build --workspace

# Desktop frontend (installs into apps/desktop/node_modules)
npm --prefix apps/desktop install
npm --prefix apps/desktop run build

# Tauri host (standalone package; writes apps/desktop/src-tauri/Cargo.lock)
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml

# Dev loop (serves the Vite app and opens the Tauri window)
npm --prefix apps/desktop run tauri dev
```

On Windows, use `npm.cmd` instead of `npm` if the shell selects a Unix wrapper.

The scaffold was verified on Windows with the three build commands above.
On 2026-11-07, native macOS arm64 verification passed the engine build,
frontend build, standalone Tauri build, and debug app bundling. The bundled
app opened and rendered the Agent Orchestrator heading. The workspace test
command passed, but there are no tests yet. Native subprocess launch, output
capture and termination, and a response-only Codex JSON protocol probe also
passed. See [the laptop verification plan and results](docs/laptop-verification-plan.md)
for commands, environment, and limitations. The Windows GUI remains unverified.

## CLI task store and reconciliation

`orchestrator-cli` keeps task, attempt, session and process-identity records in
`<data-dir>/store.sqlite` (`SqliteStore`). Task JSON records written by the
earlier `FileStore`-based CLI are not migrated or read, and no migration
command exists. Such tasks do not appear in `tasks`; keep or retire the old
data directory by hand.

`run`, `cancel`, `recover-owner` and `cleanup` take the `.orchestrator.lock`
data-directory lock for their short mutation sections only, not for the agent's
lifetime. `run` records the agent's process identity at launch and, before it
creates any new task or attempt, reconciles every unfinished record against the
live OS process state. It refuses to schedule while any record is unresolved or
ambiguous (for example a running session with no recorded identity) and prints
the blocking records.

The gate also inspects the recorded worktree and any persisted delivery steps
(read-only; it never applies, removes or repairs anything):

- A session whose process is proven gone is inspected *before* the task is
  marked Failed. A Foreign or Unknown workspace (not a registered worktree, no
  recorded workspace, or the repository cannot be inspected) refuses scheduling
  and leaves the session, attempt and task untouched, so the next run
  re-inspects them.
- A Dirty, Missing or BranchMoved worktree of such a task keeps its files, is
  reported on stderr with the advisory classification, and takes the existing
  Failed transition. This is a gate policy for unrelated new tasks, not a
  success inference, a cleanup authorization or a classifier override.
- For any task, Partial or Ambiguous delivery evidence, or Confirmed delivery
  on a task that is not Succeeded, refuses scheduling. Settled terminal tasks
  never block because their retained worktree is dirty or gone.

See
[the M2-06 evidence](docs/evidence/m2-06-macos/m2-06-cli-wiring.md).

## Dependencies

All direct dependencies are pinned with `=` and all three lockfiles are
committed: `Cargo.lock` (engine workspace), `apps/desktop/src-tauri/Cargo.lock`
(Tauri host), and `apps/desktop/package-lock.json` (frontend). Versions were
pinned 2026-11-07 against crates.io and npm max stable releases; the plan's
dependency table is an input, not the source of truth.
