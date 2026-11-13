# M7-32 Windows v3 results

P00 | PASS | Pinned source commit and tree verified; reused v2 isolated checkout was clean. | P00-run-identity.txt
P01 | PASS | 22.05 GiB free against the 20 GiB minimum. | P01-free-space.txt
P02 | PASS | Reused the isolated pinned clone from v2; source remained clean at the required commit. | P02-P07-carry-forward.txt
P03 | PASS | Repository-local autocrlf=false and longpaths=true. | P02-P07-carry-forward.txt
P04 | PASS | Detached checkout remained at the exact pinned commit. | P02-P07-carry-forward.txt
P05 | PASS | HEAD matched 6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d. | P02-P07-carry-forward.txt
P06 | PASS | Tree matched 62524d7c14d68bcca58c5b6d7cad762885eaf4f9. | P02-P07-carry-forward.txt
P07 | PASS | Source checkout status was clean. | P02-P07-carry-forward.txt
P08 | PASS | Git 2.46.2.windows.1. | P08-git-version.txt
P09 | PASS | Cargo 1.96.0. | P09-cargo-version.txt
P10 | PASS | Rust 1.96.0, x86_64-pc-windows-msvc, LLVM 22.1.2. | P10-rustc-host.txt
P11 | PASS | Windows 11 Education, build 26200. | P11-windows.txt
P12 | PASS | WebView2 runtime 157.0.4341.46. | P12-webview2.txt
P13 | BLOCKED | Foreground handle was zero; native capture returned GetCursorPos access denied (0x80070005). | P13-desktop.txt
P14 | PASS | Visual Studio C++ tools present; cl was not on the initial path. | P14-where-cl-before.txt
P15 | PASS | cl was available in the Visual Studio developer shell. | P15-where-cl-devshell.txt
P16 | FAIL | Host Node 22.20.0 was unsupported by the pinned desktop toolchain; process-local Node 24.22.3 remedy passed P19. | P16-node-version.txt; P19-node-remedy-version.txt
P17 | PASS | npm 11.3.0 available. | P17-npm-version.txt
P18 | PASS | Official Node 24.22.3 archive SHA-256 matched the published checksum. | P18-node-download.txt
P19 | PASS | Node 24.22.3 supported runtime verified without changing machine PATH. | P19-node-remedy-version.txt; P19-node24-runtime.txt
P20 | BLOCKED-EXTERNAL | Codex CLI was 0.20.0, not required 0.167.0; sign-in was not queried, so live-agent rows were not run. | P20-codex-version.txt
G01 | PASS | Carried forward from v2; npm ci exited 0 under process-local Node 24.22.3. | G01-npm-ci.command.txt; G01-npm-ci.exit.txt; G01-npm-ci.stdout.txt; G00-node24-runtime.txt
G02 | PASS | Carried forward from v2; 52 desktop test files and 1,253 tests passed. | G02-npm-test.command.txt; G02-npm-test.exit.txt; G02-npm-test.stdout.txt
G03 | PASS | Fresh v3 npm run build exited 0; built-in Vite chunk-size warning was informational. | G03-npm-build.command.txt; G03-npm-build.exit.txt; G03-npm-build.stdout.txt; G03-npm-build.stderr.txt
G04 | PASS | Carried forward from v2; contract test exited 0. | G04-contract-test.command.txt; G04-contract-test.exit.txt; G04-contract-test.stdout.txt
G05 | PASS | Carried forward from v2; 97 script tests passed, 0 failed. | G05-scripts-tests.command.txt; G05-scripts-tests.exit.txt; G05-scripts-tests.stdout.txt
G06 | PASS | Carried forward from v2; 858 passed, 0 failed, 2 ignored across 73 test-result records. | G06-cargo-test-workspace.command.txt; G06-cargo-test-workspace.exit.txt; G06-cargo-test-workspace.stdout.txt
G07 | PASS | Carried forward from v2; locked workspace clippy with warnings denied exited 0. | G07-cargo-clippy.command.txt; G07-cargo-clippy.exit.txt; G07-cargo-clippy.stdout.txt; G07-cargo-clippy.stderr.txt
G08 | PASS | Carried forward from v2; locked daemon examples and binaries built and daemon binary existed. | G08-daemon-build.command.txt; G08-daemon-build.exit.txt; G08-daemon-build.stdout.txt; G08-daemon-binary.txt
G01/G02/G04-G08 carry-forward | PASS | v2 evidence was reused as instructed and not rerun in v3; G03 alone was run once in v3. | G01-npm-ci.exit.txt; G02-npm-test.exit.txt; G04-contract-test.exit.txt; G05-scripts-tests.exit.txt; G06-cargo-test-workspace.exit.txt; G07-cargo-clippy.exit.txt; G08-daemon-build.exit.txt
N01 | BLOCKED-no-native-window | Unsigned package built and current-user installer launched; inspectable-window capture was denied, stopping the native procedure. | N01-package.txt; N01-sha256.txt; N01-install-attempt.txt; N01-native-window-blocker.txt
N02 | BLOCKED-no-native-window | Native checks not attempted after capture denial; browser-proxy theme checks passed. | P13-desktop.txt; WIN-BP-N02-system-dark.png; WIN-BP-N02-explicit-saved-dark.png; WIN-BP-N02-system-light.png
N03 | BLOCKED-no-native-window | Native checks not attempted after capture denial; browser-proxy 10 viewport cases had 0 overflow, 0 obscured focus and 0 focus-not-visible. | P13-desktop.txt; WIN-BP-N03-1280x700.png; WIN-BP-N03-1280x900.png; WIN-BP-N03-1150x700.png; WIN-BP-N03-1150x900.png; WIN-BP-N03-960x700.png; WIN-BP-N03-960x900.png; WIN-BP-N03-760x700.png; WIN-BP-N03-760x900.png; WIN-BP-N03-480x700.png; WIN-BP-N03-480x900.png; WIN-BP-observations.json
N04 | BLOCKED-no-native-window | Native checks not attempted after capture denial; browser-proxy keyboard navigation passed. | P13-desktop.txt; WIN-BP-N04-keyboard.png
N05 | BLOCKED-no-native-window | Native screen-reader checks were not attempted after capture denial. | P13-desktop.txt
N06 | BLOCKED-no-native-window | Native checks not attempted after capture denial; browser-proxy reduced-motion check passed. | P13-desktop.txt; WIN-BP-N06-reduced-motion.png
N07 | BLOCKED-no-native-window | Native routes not attempted; browser-proxy global/legacy routes and unknown-route history passed, but real task/task-review routes were blocked because no genuine task ID was available. | P13-desktop.txt; WIN-BP-N07-routes.png; WIN-BP-observations.json
N08 | BLOCKED-no-native-window | Native checks not attempted after capture denial; browser-proxy stop/restart recovered but showed PLACEHOLDER DATA (D3) in live connected, stale and recovered states. | P13-desktop.txt; WIN-BP-N08-stop-restart.txt; WIN-BP-N08-connected-before-stop.png; WIN-BP-N08-stale-after-stop.png; WIN-BP-N08-reconnected-after-restart.png
N09 | BLOCKED-no-native-window | Native lifecycle not attempted; live-agent portion additionally BLOCKED-EXTERNAL because required signed-in Codex 0.167.0 was not verified. | P13-desktop.txt; P20-codex-version.txt
N10 | BLOCKED-no-native-window | Native approvals and fixture-boundary checks were not attempted after capture denial. | P13-desktop.txt
N11 | BLOCKED-no-native-window | Native checks not attempted after capture denial; browser-proxy metrics matched within tolerance and usage was unavailable, but PLACEHOLDER DATA (D3) appeared in live mode. | P13-desktop.txt; WIN-BP-N11-comparison.json; WIN-BP-N11-host-metrics-response.json; WIN-BP-N11-live-data-honesty.png
N12 | BLOCKED-no-native-window | Native package-security and network checks were not attempted after capture denial. | P13-desktop.txt
F08 | BLOCKED-no-native-window | Native/live-conversation checks not attempted; previously passing automated substeps were not repeated, and G06 remains the v2 carry-forward. | P13-desktop.txt; G06-cargo-test-workspace.exit.txt
F10 | BLOCKED-no-native-window | Native live-conversation checks were not attempted after capture denial. | P13-desktop.txt
F11 | BLOCKED-no-native-window | Native live-workstream checks were not attempted after capture denial. | P13-desktop.txt
F12 | BLOCKED-no-native-window | Native live-activity checks were not attempted after capture denial. | P13-desktop.txt
F14 | BLOCKED-no-native-window | Native inventory was not attempted; live-agent portion additionally BLOCKED-EXTERNAL because required signed-in Codex 0.167.0 was not verified. | P13-desktop.txt; P20-codex-version.txt
F15 | BLOCKED-no-native-window | Native/packaged route checks were not attempted after capture denial. | P13-desktop.txt
F16 | BLOCKED-no-native-window | Native live-count and search checks were not attempted after capture denial. | P13-desktop.txt
F17 | BLOCKED-no-native-window | Native DPI checks were not attempted after capture denial. | P13-desktop.txt
F18 | BLOCKED-no-native-window | Installed-renderer checks were not attempted after capture denial. | P13-desktop.txt
F20 | BLOCKED-no-native-window | Native live-capacity checks were not attempted after capture denial. | P13-desktop.txt
F22 | BLOCKED-no-native-window | Native real-session usage checks were not attempted after capture denial. | P13-desktop.txt
F23 | BLOCKED-no-native-window | Native migration checks were not attempted; no owner-supplied pre-migration data was used. | P13-desktop.txt
F24 | BLOCKED-no-native-window | Native live-conversation checks were not attempted after capture denial. | P13-desktop.txt
F25 | BLOCKED-no-native-window | Native checks not attempted; previously passing automated substeps were not repeated, and G06 remains the v2 carry-forward. | P13-desktop.txt; G06-cargo-test-workspace.exit.txt
F26 | BLOCKED-no-native-window | Native real-task usage and UI checks were not attempted; automated substeps were not repeated. | P13-desktop.txt; G06-cargo-test-workspace.exit.txt
M6-04 W10 | WAIVED | Owner waived signed-installer validation; unsigned N01 was attempted but native inspection was blocked. | M7-32-dispositions.txt; N01-native-window-blocker.txt
M6-04 W13 | WAIVED | Owner waived N-1-to-N upgrade validation. | M7-32-dispositions.txt
M6-04 W14 | WAIVED | Owner waived signed-installer smoke. | M7-32-dispositions.txt
M7-32 fix-e2e focused real-daemon test | N/A | The Windows harness is Unix-socket-only. | M7-32-dispositions.txt
WIN-BP-N02 | PASS | Browser-proxy system dark/light behavior and saved explicit dark preference persistence passed. | WIN-BP-N02-system-dark.png; WIN-BP-N02-explicit-saved-dark.png; WIN-BP-N02-system-light.png; WIN-BP-observations.json
WIN-BP-N03 | PASS | Browser-proxy responsive checks passed at 10 viewport sizes; no horizontal overflow or focus obstruction was observed. | WIN-BP-N03-1280x700.png; WIN-BP-N03-1280x900.png; WIN-BP-N03-1150x700.png; WIN-BP-N03-1150x900.png; WIN-BP-N03-960x700.png; WIN-BP-N03-960x900.png; WIN-BP-N03-760x700.png; WIN-BP-N03-760x900.png; WIN-BP-N03-480x700.png; WIN-BP-N03-480x900.png
WIN-BP-N04 | PASS | Browser-proxy keyboard-only navigation and focus restoration passed. | WIN-BP-N04-keyboard.png; WIN-BP-observations.json
WIN-BP-N06 | PASS | Browser-proxy reduced-motion check passed. | WIN-BP-N06-reduced-motion.png
WIN-BP-N07 | BLOCKED | Global/legacy routes and unknown-route history passed; task/task-review routes need a real task ID, and no task was submitted. | WIN-BP-N07-routes.png; WIN-BP-observations.json
WIN-BP-N08 | FAIL | Named-pipe stop/restart and recovery worked, but the live UI showed PLACEHOLDER DATA (D3), including after reconnect. | WIN-BP-N08-stop-restart.txt; WIN-BP-N08-connected-before-stop.png; WIN-BP-N08-stale-after-stop.png; WIN-BP-N08-reconnected-after-restart.png; WIN-BP-observations.json
WIN-BP-N11 | FAIL | Metrics matched daemon host.metrics within tolerance; usage was unavailable, but PLACEHOLDER DATA (D3) appeared in live mode. | WIN-BP-N11-comparison.json; WIN-BP-N11-host-metrics-response.json; WIN-BP-N11-live-data-honesty.png; WIN-BP-observations.json
WIN-BP browser suite | PASS | npm run test:browser exited 0 with 11 tests passed. | WIN-BP-test-browser-summary.txt
P98 | PASS | No matching v3 evidence branch existed before publication. | P98-prepush-ref-check.txt
P99 | PASS | Publication copies passed the runbook privacy scan with 0 matches; screenshots were reviewed and local path rows redacted. | P99-privacy-scan.txt

--- Final checkout verification ---
git rev-parse HEAD:
6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d
git status --porcelain=v1 --untracked-files=all:
