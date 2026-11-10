# Release CI (M6-02)

Native macOS and Windows gates, dependency audits, and signed or unsigned release builds on
GitHub Actions. Nothing here publishes a release.

## Workflows

| File | Runs on | Triggers | What it does |
| --- | --- | --- | --- |
| `.github/workflows/ci.yml` | `macos-15`, `windows-2025` | pull request, push to `main`, manual, called by `release.yml` | Sets `core.autocrlf=false` and `core.longpaths=true`, then runs every native gate once with `CI=true`, its own target directory and an isolated test temp directory. |
| `.github/workflows/audit.yml` | `ubuntu-24.04` | changes to manifests, lockfiles or `deny.toml`; push to `main`; daily; manual; called by `release.yml` | `cargo deny` for both manifests, `cargo audit` for both lockfiles, `npm audit --audit-level=high`. |
| `.github/workflows/release.yml` | `macos-15`, `windows-2025` | tag `v*`, manual | Requires `ci.yml` and `audit.yml` to pass, then builds the macOS app and the Windows NSIS installer. Signs when secrets are present. Uploads artifacts only. |

Every third-party action is pinned to a commit SHA. Workflows have `contents: read` and no
other permission. Secrets are mapped only into the `Package` step of `release.yml`.

## Gates

`scripts/ci/gates.mjs` defines the gates as data; `scripts/ci/run-gates.mjs` runs them;
`scripts/ci/parse.mjs` judges the output. A gate passes only when the command exits 0 within its
time limit **and** the output matches the expected shape: test counts, the `running N tests`
sequence, named tests, `Compiling` lines for a build gate (so a cached target cannot pass), the
Vitest file and test counts, and the artifact. Nothing is retried. A gate whose prerequisite did
not pass is `SKIPPED`, which also fails the run.

The Windows list is the published runbook
`docs/evidence/priority-11r3-windows-acceptance-efa5e8a-runbook.md` (branch
`docs/priority-11r3-windows-acceptance-efa5e8a`): same commands, order and derived counts.

| Gate | Command | Where |
| --- | --- | --- |
| `daemon-build` | `cargo build --locked -p orchestrator-daemon` | both |
| `ipc-tests` | `cargo test --locked -p orchestrator-ipc` (45 tests) | both |
| `windows-transport-tests` | `cargo test --locked -p orchestrator-ipc windows_transport` (13 tests) | Windows |
| `daemon-tests` | `cargo test --locked -p orchestrator-daemon` (Windows 46, macOS 59+4+9) | both |
| `daemon-lifecycle` | `cargo test --locked -p orchestrator-daemon --test lifecycle -- --nocapture` | both |
| `coordinator-review-tests` | `cargo test --locked -p orchestrator-daemon --test coordinator_review` (4 tests) | both |
| `clippy-scoped` | `cargo clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` (baseline: two `single_match` warnings in `supervisor.rs`) | both |
| `workspace-tests` | `cargo test --locked --workspace` (638 passed, 1 ignored) | macOS |
| `desktop-npm-ci`, `desktop-tsc`, `desktop-build`, `desktop-vitest` | in `apps/desktop`: `npm ci`, `npx tsc -b`, `npm run build`, `npx vitest run` (15 files, 287 tests) | both |
| `desktop-e2e` | `npm run test:e2e` (1 file, 19 tests, real daemon over a Unix socket) | macOS |
| `tauri-build`, `tauri-tests`, `tauri-clippy-clean`, `tauri-clippy` | in `apps/desktop/src-tauri`: `cargo build --locked`, `cargo test --locked` (Windows 9, macOS 10), `cargo clean -p agent-orch-desktop`, `cargo clippy --locked --all-targets --verbose` | both |
| `desktop-package-win` / `desktop-package-mac` | `npm run package:win` (NSIS installer) / `npm run package:mac` (`.app`) | Windows / macOS |

Windows and macOS counts differ because tests are gated by `cfg`. Expected counts are never edited
to fit a run; a change needs a reason in the commit that edits it.

Run on both platforms, outside the checkout: `node --test "scripts/**/*.test.mjs"` (harness
self-tests).

### Evidence

Each run writes, per gate, raw `<gate>.stdout.txt` and `<gate>.stderr.txt`, a `<gate>.result.json`
with the verdict and reasons, `environment.txt`, `summary.md` (also the job summary), and for
Windows the list of rows this CI cannot prove. The workflow uploads the directory as
`gate-evidence-<platform>`.

## Dependency audits

- `deny.toml` pins the graph to `aarch64-apple-darwin`, `x86_64-apple-darwin` and
  `x86_64-pc-windows-msvc`, so Linux-only crates are not judged. It denies unknown registries and
  git sources, yanked crates, and licenses outside an allow-list. `cargo deny` runs once per
  manifest because `apps/desktop/src-tauri` is outside the root workspace.
- `cargo audit --file <lockfile>` runs for both lockfiles; vulnerabilities fail, warnings (the
  unmaintained GTK3 crates that only Linux builds use) do not.
- `npm audit --audit-level=high` runs against `apps/desktop/package-lock.json`.
- **Known exception:** `RUSTSEC-2026-0459` (bundled SQLite before 3.54.0 in `libsqlite3-sys`
  0.37.0, reached through `sqlx` 0.9.0) is ignored in `deny.toml` and `.cargo/audit.toml` with a
  reason. It is a tracked finding, not an acceptance. Remove both entries once the lockfile has
  `libsqlite3-sys` 0.39 or later.
- MPL-2.0 is in the license allow-list because the desktop graph contains it. Legal needs to
  decide this before a public release.

## Release builds and signing

`scripts/release/package.mjs <macos|windows> --out DIR` runs the same `npm run package:*` as the
CI gate, verifies the result, and writes the artifact, `SHA256SUMS` and `release-info.json`. The
artifact name ends in `signed` or `unsigned`.

`scripts/release/signing.mjs` decides from variable names only. No secret value is ever read into
a message.

| Situation | Result |
| --- | --- |
| No signing secret is set | Warning `No <platform> signing secrets are set ...; building UNSIGNED`; the build succeeds. |
| Some, but not all, required secrets are set | Error naming the missing variables. A half-configured release never ships unsigned. |
| `REQUIRE_SIGNING=true` and no secret is set | Error. Set it with the `require_signing` input of the manual run or the repository variable `REQUIRE_SIGNING`. |
| All signing secrets set | Signed build; verified afterwards. |

Secrets live in the GitHub environment `release` (add required reviewers there to gate releases):

| Platform | Signing | Notarization |
| --- | --- | --- |
| macOS | `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`; all three or none |
| Windows | `WINDOWS_CERTIFICATE` (base64 `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` | not applicable |

Optional variable: `WINDOWS_TIMESTAMP_URL` (default `http://timestamp.digicert.com`). Empty
variables are dropped before Tauri runs, because Tauri treats a variable that is set but empty as
configured. On macOS Tauri imports the certificate into a temporary keychain and, with the
notarization variables, notarizes; afterwards `codesign --verify --deep --strict` (and `stapler
validate` plus `spctl` when notarized) must pass. On Windows the certificate is imported into the
current-user store, passed to Tauri as `certificateThumbprint` through `TAURI_CONFIG`, checked with
`Get-AuthenticodeSignature` (status `Valid`), and removed afterwards.

## Run it locally

```sh
node --test "scripts/**/*.test.mjs"
node scripts/ci/run-gates.mjs --list
# Needs a clean, committed tree; the evidence dir must not exist; the target dir must be empty.
CI=true node scripts/ci/run-gates.mjs \
  --evidence-dir ../evidence-run --target-dir ../target-run --test-tmp ../tt
```

Keep the test temp path short: macOS Unix socket paths are limited to about 104 characters.
`--only a,b` runs a subset (later gates that need an earlier one are then `SKIPPED`).

## Windows verification

The Windows agent must run these natively on a Windows host. None of it has been run on Windows by
the M6-02 owner.

1. `git config --global core.autocrlf false` and `git config --global core.longpaths true`, then a
   clean checkout of the commit under test at a short path.
2. Toolchain: Rust 1.96.0 (MSVC), Node 22.20.0, npm, Visual Studio Build Tools with the C++
   workload, Windows SDK, network access (NSIS download). Record versions; a missing tool is
   `BLOCKED`.
3. `node --test "scripts/**/*.test.mjs"`: the parser tests (including CRLF output) must pass.
4. `node scripts/ci/run-gates.mjs --list` must show the 16 Windows gates, including
   `windows-transport-tests` and `desktop-package-win`.
5. `$env:CI = "true"; node scripts/ci/run-gates.mjs --evidence-dir <new> --target-dir <new empty> --test-tmp <new>`
   Expected: every gate `PASS` with the derived counts in the gate table. A count mismatch is a
   finding: report it; do not edit `scripts/ci/gates.mjs` to fit.
6. Confirm `git-config`, `gitattributes-before/after` and `source-status-before/after` pass, and
   that the run leaves the tree clean.
7. With a throwaway test certificate (never a real one), run
   `node scripts/release/package.mjs windows --out <new>` with `CARGO_TARGET_DIR` set to an
   absolute path: once with no secrets (expect an unsigned build and the warning), once with
   `REQUIRE_SIGNING=true` (expect an error), once with a test `.pfx` in `WINDOWS_CERTIFICATE` and
   `WINDOWS_CERTIFICATE_PASSWORD` (expect `Valid` only if the test certificate chains to a trusted
   root; record the real status either way).
8. Report the real counts, durations, and the first failing gate, if any.

## What only a real GitHub runner can prove

Local checks (below) prove the YAML, the shell and the harness logic. They do not prove:

- that the runner images `macos-15` and `windows-2025` have the expected Xcode, MSVC, Windows SDK,
  WebView2 and rustup, or that `setup-node` can install Node 26.13.0 and 22.20.0;
- Windows gate counts and the Windows clippy baseline at these toolchain versions (they are the
  runbook's derived values until a Windows host measures them);
- the NSIS download, the Windows installer build and the sidecar being bundled;
- that `taiki-e/install-action` installs the pinned `cargo-deny` and `cargo-audit` and that the
  advisory database is reachable from the runner;
- the Apple and Windows signing, notarization and timestamp paths with real certificates;
- artifact upload, `environment: release` behaviour, and wall-clock time against the 180-minute
  and 120-minute limits;
- that reusable-workflow `workflow_call` composition in `release.yml` resolves on the default
  branch.

Not covered by M6-02: updater signing keys and update metadata, installing the artifact on a clean
machine, and the Windows desktop e2e (it needs a Unix socket).
