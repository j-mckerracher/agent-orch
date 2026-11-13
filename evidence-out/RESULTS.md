# M7-32 Windows v2 results

P13 | BLOCKED | Session ID 1, UserInteractive true, same-session Explorer present; foreground window handle was zero, so no app or native row was started. | P13-desktop.txt
G01 | PASS | npm ci exited 0 using process-local Node 24.22.3. | G01-npm-ci.stdout.txt; G01-npm-ci.stderr.txt; G01-npm-ci.exit.txt; G00-node24-runtime.txt
G02 | PASS | 52 desktop test files and 1,253 tests passed. | G02-npm-test.stdout.txt; G02-npm-test.exit.txt
G03 | BLOCKED | Build output was produced, but PowerShell 7 surfaced a native-command diagnostic before the helper recorded an exit code; result cannot be verified and this executed gate was not rerun. | G03-npm-build.stdout.txt; G03-npm-build.stderr.txt; G03-execution-note.txt
G04 | PASS | Contract test exited 0. | G04-contract-test.stdout.txt; G04-contract-test.exit.txt
G05 | PASS | Script tests: 97 passed, 0 failed. | G05-scripts-tests.stdout.txt; G05-scripts-tests.exit.txt
G06 | PASS | Workspace tests: 858 passed, 0 failed, 2 ignored across 73 test-result records. | G06-cargo-test-workspace.stdout.txt; G06-cargo-test-workspace.stderr.txt; G06-cargo-test-workspace.exit.txt
G07 | PASS | cargo clippy -- -D warnings exited 0; no warning or error lines captured. | G07-cargo-clippy.stdout.txt; G07-cargo-clippy.stderr.txt; G07-cargo-clippy.exit.txt
G08 | PASS | Locked daemon examples and binaries build exited 0; daemon binary exists. | G08-daemon-build.stdout.txt; G08-daemon-build.stderr.txt; G08-daemon-build.exit.txt; G08-daemon-binary.txt
N01 | BLOCKED | P13 failed because the foreground window handle was zero; package install, launch and uninstall were not attempted. | P13-desktop.txt
N02 | BLOCKED | P13 failed because the foreground window handle was zero; no theme, contrast or forced-colors checks were attempted and no settings were changed. | P13-desktop.txt
N03 | BLOCKED | P13 failed because the foreground window handle was zero; no responsive or DPI checks were attempted and no display settings were changed. | P13-desktop.txt
N04 | BLOCKED | P13 failed because the foreground window handle was zero; keyboard-only UI checks were not attempted. | P13-desktop.txt
N05 | BLOCKED | P13 failed because the foreground window handle was zero; screen-reader checks were not attempted. | P13-desktop.txt
N06 | BLOCKED | P13 failed because the foreground window handle was zero; reduced-motion and hidden-window checks were not attempted and no machine settings were changed. | P13-desktop.txt
N07 | BLOCKED | P13 failed because the foreground window handle was zero; native route and deep-link checks were not attempted. | P13-desktop.txt
N08 | BLOCKED | P13 failed because the foreground window handle was zero; native daemon disconnect/recovery checks were not attempted. | P13-desktop.txt
N09 | BLOCKED | P13 failed because the foreground window handle was zero; live lifecycle checks were not attempted and Codex status was not queried. | P13-desktop.txt
N10 | BLOCKED | P13 failed because the foreground window handle was zero; native approvals and fixture-boundary checks were not attempted. | P13-desktop.txt
N11 | BLOCKED | P13 failed because the foreground window handle was zero; native live-data checks were not attempted. | P13-desktop.txt
N12 | BLOCKED | P13 failed because the foreground window handle was zero; package security and network checks were not attempted. | P13-desktop.txt
F08 | BLOCKED | Previously passing automated substeps were skipped as instructed; live/native conversation checks were blocked by the zero foreground handle. | P13-desktop.txt; G06-cargo-test-workspace.stdout.txt
F10 | BLOCKED | P13 failed because the foreground window handle was zero; live conversation checks were not attempted. | P13-desktop.txt
F11 | BLOCKED | P13 failed because the foreground window handle was zero; live workstream checks were not attempted. | P13-desktop.txt
F12 | BLOCKED | P13 failed because the foreground window handle was zero; live activity checks were not attempted. | P13-desktop.txt
F14 | BLOCKED | P13 failed because the foreground window handle was zero; live agent inventory checks were not attempted. | P13-desktop.txt
F15 | BLOCKED | P13 failed because the foreground window handle was zero; packaged/native route checks were not attempted. | P13-desktop.txt
F16 | BLOCKED | P13 failed because the foreground window handle was zero; live counts/search checks were not attempted. | P13-desktop.txt
F17 | BLOCKED | P13 failed because the foreground window handle was zero; native DPI checks were not attempted. | P13-desktop.txt
F18 | BLOCKED | P13 failed because the foreground window handle was zero; installed renderer checks were not attempted. | P13-desktop.txt
F20 | BLOCKED | P13 failed because the foreground window handle was zero; live capacity checks were not attempted. | P13-desktop.txt
F22 | BLOCKED | P13 failed because the foreground window handle was zero; real-session usage checks were not attempted. | P13-desktop.txt
F23 | BLOCKED | P13 failed because the foreground window handle was zero; migration UI checks were not attempted and owner-supplied pre-migration data was not used. | P13-desktop.txt
F24 | BLOCKED | P13 failed because the foreground window handle was zero; live conversation checks were not attempted. | P13-desktop.txt
F25 | BLOCKED | Previously passing automated substeps were skipped as instructed; live/native checks were blocked by the zero foreground handle. | P13-desktop.txt; G06-cargo-test-workspace.stdout.txt
F26 | BLOCKED | Previously passing automated substeps were skipped as instructed; real-task usage and UI checks were blocked by the zero foreground handle. | P13-desktop.txt; G06-cargo-test-workspace.stdout.txt
M6-04 W10 | WAIVED | Owner waived signed-installer validation; unsigned package checks remain part of N01, which was blocked by P13. | M7-32-dispositions.txt
M6-04 W13 | WAIVED | Owner waived N-1-to-N upgrade validation. | M7-32-dispositions.txt
M6-04 W14 | WAIVED | Owner waived signed-installer smoke. | M7-32-dispositions.txt
M7-32 fix-e2e focused real-daemon test | N/A | The v2 runbook states the focused daemon harness is Unix-socket-only and not applicable on Windows. | M7-32-dispositions.txt

--- Final checkout verification ---
git rev-parse HEAD:
6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d
git status --porcelain=v1 --untracked-files=all:
