# M7-32 Windows results

Updated runbook commit 4c8b9589c72d8257bf964851e4d937c4230f0a3e; pinned source commit 92f56c3af9cd34741a09667f499164edcafec696.

row id | status | note | evidence files
P01 | PASS | 37.54 GiB free before run; above 20 GiB requirement. | P01-free-space.txt
P02 | PASS | Fresh clone succeeded (exit 0); wrapper false-failure is preserved and explained. | P02-clone.exit.txt; P02-status.txt; P02-status-review.txt
P03 | PASS | Both local checkout settings applied. | P03-config-autocrlf.exit.txt; P03-config-longpaths.exit.txt
P04 | PASS | Detached checkout at pinned source commit. | P04-checkout.exit.txt; P04-P06-pin.txt
P05 | PASS | HEAD matched 92f56c3af9cd34741a09667f499164edcafec696. | P05-head.stdout.txt
P06 | PASS | Tree matched a3012626c6808b337a719d0afcd4d887f5a504ea. | P06-tree.stdout.txt
P07 | PASS | Checkout status was clean. | P07-status.stdout.txt
P08 | PASS | Git version command succeeded. | P08-git-version.stdout.txt
P09 | PASS | Cargo version command succeeded. | P09-cargo-version.stdout.txt
P10 | PASS | Rust compiler host/version captured. | P10-rustc-host.stdout.txt
P11 | PASS | Windows edition, version and build captured. | P11-windows.txt
P12 | PASS | WebView2 Runtime 157.0.4341.46 detected. | P12-webview2.txt
P13 | BLOCKED | user_interactive was true, but query.exe was missing from PATH and System32, so active console/RDP display could not be verified; native rows not run. | P13-desktop.txt; P13-session.command.txt; P13-session.stderr.txt; P13-session.exit.txt
P14 | PASS | VS C++ installation found; cl.exe was absent before loading its developer shell as expected. | P14-where-cl-before.exit.txt; P14-devshell.txt
P15 | PASS | cl.exe was found after loading the Visual Studio developer shell. | P15-where-cl-devshell.stdout.txt
P16 | PASS | Initial Node 22.20.0 was unsupported; remediation selected. | P16-node-version.stdout.txt
P17 | PASS | npm version command succeeded. | P17-npm-version.stdout.txt
P18 | PASS | Official Node 24.22.3 zip SHA-256 matched SHASUMS256.txt. | P18-node-download.txt
P19 | PASS | Remedied Node 24.22.3 satisfies the runbook version gate. | P19-node-remedy-version.stdout.txt
P20 | BLOCKED | Codex CLI is 0.20.0, not the required 0.167.0; live-agent rows were not run. Sign-in state was not queried or claimed. | P20-codex-version.stdout.txt
G01 | PASS | npm ci exited 0 with supported Node 24.22.3. | G01-npm-ci.stdout.txt; G01-npm-ci.stderr.txt; G01-npm-ci.exit.txt
G02 | PASS | 52 files and 1,253 desktop tests passed. | G02-npm-test.stdout.txt; G02-npm-test.exit.txt
G03 | PASS | Production build exited 0; Vite reported a large-chunk advisory. | G03-npm-build.stdout.txt; G03-npm-build.stderr.txt; G03-npm-build.exit.txt
G04 | PASS | Contract test command exited 0. | G04-contract-test.stdout.txt; G04-contract-test.exit.txt
G05 | PASS | Script tests: 97 passed, 0 failed. | G05-scripts-tests.stdout.txt; G05-scripts-tests.exit.txt
G06 | PASS | Workspace tests: 858 passed, 0 failed, 2 ignored across 73 test-result records. | G06-cargo-test-workspace.stdout.txt; G06-cargo-test-workspace.exit.txt
G07 | FAIL | Clippy with -D warnings found two single_match warnings at supervisor.rs:431 and :454. | G07-cargo-clippy.stderr.txt; G07-cargo-clippy.exit.txt
G08 | PASS | Locked daemon examples/binaries build exited 0; orchestrator-daemon.exe exists. | G08-daemon-build.stdout.txt; G08-daemon-build.stderr.txt; G08-daemon-build.exit.txt
N01 | BLOCKED | Package/install/launch/uninstall requires native session verification (P13); not run. | P13-session.stderr.txt
N02 | BLOCKED | Theme, contrast and forced-colors visual checks require native session (P13); no settings changed. | P13-session.stderr.txt
N03 | BLOCKED | Responsive and DPI visual measurements require native session (P13); no display settings changed. | P13-session.stderr.txt
N04 | BLOCKED | Keyboard-only native UI checks require native session (P13). | P13-session.stderr.txt
N05 | BLOCKED | Narrator/NVDA native accessibility checks require native session (P13); no screen-reader result claimed. | P13-session.stderr.txt
N06 | BLOCKED | Reduced-motion, hidden-window and resource sampling need native UI (P13); no machine setting changed. | P13-session.stderr.txt
N07 | BLOCKED | Native route/deep-link checks require interactive app session (P13). | P13-session.stderr.txt
N08 | BLOCKED | Native named-pipe disconnect/recovery UI checks require interactive app session (P13). | P13-session.stderr.txt
N09 | BLOCKED | Live lifecycle/drafts require native app and Codex 0.167.0 (P13/P20). | P13-session.stderr.txt; P20-codex-version.stdout.txt
N10 | BLOCKED | Real approvals and fixture-boundary UI checks require native app and live-agent prerequisites (P13/P20). | P13-session.stderr.txt; P20-codex-version.stdout.txt
N11 | BLOCKED | Native live-data honesty checks require interactive app and daemon (P13). | P13-session.stderr.txt
N12 | BLOCKED | Installed-package CSP/network/path UI checks require N01 and interactive app (P13). | P13-session.stderr.txt
F08 | BLOCKED | Exact deterministic tests and negative paths passed in G06; live Codex/native conversation is blocked by P13/P20. | F08-daemon-tests.log; F08-negative.log; F08-live-codex.log; G06-cargo-test-workspace.stdout.txt
F10 | BLOCKED | Real-daemon notes and Global truthfulness UI steps require P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F11 | BLOCKED | Real agents, multi-project workstreams and live task volume require P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F12 | BLOCKED | Live activity/event-refresh UI checks require P13. | P13-session.stderr.txt
F14 | BLOCKED | Real agent inventory and task controls require P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F15 | BLOCKED | Packaged/native five-route visual scan requires P13. | P13-session.stderr.txt
F16 | BLOCKED | Live counts, search and usage UI checks require P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F17 | BLOCKED | Live device-resource DPI checks require P13. | P13-session.stderr.txt
F18 | BLOCKED | Installed package/background renderer checks require N01 and P13. | P13-session.stderr.txt
F20 | BLOCKED | Occupied slots, queued tasks and live capacity UI require P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F22 | BLOCKED | Real-session usage popover requires P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F23 | BLOCKED | Pre-migration owner data was not provided; native migration checks require P13. | P13-session.stderr.txt
F24 | BLOCKED | Real Global/workstream conversations and native accessibility require P13/P20. | P13-session.stderr.txt; P20-codex-version.stdout.txt
F25 | BLOCKED | Full daemon workspace tests and both updated filtered tests passed; live replies/recovery/authority UI checks require P13/P20. | F25-daemon-tests.log; F25-authority.txt; G06-cargo-test-workspace.stdout.txt
F26 | BLOCKED | Store/daemon automated tests passed in G06; real Codex usage task and UI summary require P13/P20. | F26-tests.log; G06-cargo-test-workspace.stdout.txt
M6-04 W10 | BLOCKED-EXTERNAL | No installer artifact was built because P13 blocked native setup; signature validation also needs signing material. | P13-session.stderr.txt
M6-04 W13 | BLOCKED-EXTERNAL | Requires two released artifacts not supplied by this run. | RESULTS.md
M6-04 W14 | BLOCKED-EXTERNAL | Requires signed N-1 and N artifacts not supplied by this run. | RESULTS.md
M7-32 checklist 1 macOS | BLOCKED | macOS-only, not run on Windows. | n/a
M7-32 checklist 2 Windows | BLOCKED | Native WebView2/screen-reader check blocked by missing query.exe session verification (P13). | P13-session.stderr.txt
M7-32 checklist 3 Windows DPI | BLOCKED | Native DPI screenshots/checks require verified interactive app session (P13). | P13-session.stderr.txt
AXE branch automated checks | PASS | Exact commit 32059d14b4a48da405b003cd2db299bda6694ee6: focused AppShell test 53 passed; full desktop 1,253 passed; build passed. | AXE-results.md; AXE-G02-focused-test.stdout.txt; AXE-G03-full-test.stdout.txt; AXE-G04-build.stdout.txt
AXE branch native screen-reader/DPI | BLOCKED | Native Windows WebView2 and screen reader/DPI evidence unavailable because P13 is blocked. | AXE-results.md; P13-session.stderr.txt

M7-32 fix-e2e branch pin | PASS | Exact remote commit and clean worktree were verified; automated real-daemon checks are recorded separately. | E2E-P01-pin.txt; E2E-P03-status.txt
M7-32 fix-e2e focused real-daemon test | BLOCKED | Test invoked but daemon readiness timed out; the harness documents Unix-socket-only support, so Windows cannot verify this assertion. | E2E-G01-daemon-build.exit.txt; E2E-G02-real-daemon.exit.txt; E2E-G02-real-daemon.stdout.txt; E2E-G02-real-daemon.stderr.txt; E2E-results.md
M7-32 fix-e2e native checklist | BLOCKED | Deferred by the branch checklist; Windows and accessibility/DPI rows remain unverified. | E2E-results.md
REMOTE-SCAN | PASS | Final direct scan found 153 heads; the only change from the prior 152-head inventory was the inspected M7-32 E2E branch. | remote-heads-all-final.tsv; E2E-results.md
P99 privacy scan | PASS | Final publication-copy privacy scan returned zero matches. | P99-privacy-scan.txt; P99-redaction-summary.txt
P98 evidence normalization | PASS | Publication text and TSV copies normalized to LF and trailing whitespace removed; original raw evidence is retained locally. | P98-normalization-summary.txt
git -C $Src rev-parse HEAD
92f56c3af9cd34741a09667f499164edcafec696
git -C $Src status --porcelain=v1 --untracked-files=all
