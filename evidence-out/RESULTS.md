# M7-32 Windows consolidated evidence update

P00 | PASS | Fresh isolated clone verified at the pinned source commit and tree; final status remained clean. | P04-checkout.exit.txt; P05-head.stdout.txt; P06-tree.stdout.txt; P07-status.stdout.txt
P01 | PASS | 33.58 GiB free on the run volume, above the 20 GiB minimum. | P01-free-space.txt; P01-fm-variable-names.txt
P02 | PASS | Fresh clone from the repository remote completed successfully. | P02-clone.command.txt; P02-clone.exit.txt; P02-clone.stdout.txt; P02-clone.stderr.txt
P03 | PASS | Repository-local autocrlf=false and longpaths=true. | P03-config-autocrlf.exit.txt; P03-config-longpaths.exit.txt
P04 | PASS | Detached checkout at the required source pin. | P04-checkout.exit.txt; P05-head.stdout.txt
P05 | PASS | HEAD matched 6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d. | P05-head.stdout.txt
P06 | PASS | Tree matched 62524d7c14d68bcca58c5b6d7cad762885eaf4f9. | P06-tree.stdout.txt
P07 | PASS | Source status was clean after all build and package steps. | P07-status.stdout.txt; checkout-final.txt
P08 | PASS | Git version command exited 0. | P08-git-version.stdout.txt; P08-git-version.exit.txt
P09 | PASS | Cargo 1.96.0. | P09-cargo-version.stdout.txt
P10 | PASS | Rust 1.96.0, x86_64-pc-windows-msvc. | P10-rustc-host.stdout.txt
P11 | PASS | Windows 11 Education, build 26200. | P11-windows.txt
P12 | PASS | WebView2 runtime 157.0.4341.46. | P12-webview2.txt
P13 | PASS | v3 replacement probe: session ID, UserInteractive, same-session Explorer and nonzero foreground handle evaluated; legacy query.exe output is not part of v3 criteria. | P13-v3-session-check.txt; P13-desktop.txt
P14 | PASS | Visual Studio C++ tools were found; cl was not initially on PATH. | P14-vswhere.txt; P14-where-cl-before.txt
P15 | PASS | cl.exe was available after loading the VS 2022 developer shell. | P14-vs-devshell.txt; P15-where-cl-devshell.txt
P16 | FAIL | Host Node 22.20.0 is below the supported 22.22.2 minimum. | P16-node-version.txt
P17 | PASS | npm 11.12.1 was available. | P17-npm-version.txt
P18 | PASS | Official Node 24.15.0 win-x64 archive SHA-256 matched the published checksum. | P18-node24-zip.txt
P19 | PASS | Process-local Node 24.15.0 remedy verified; machine PATH was not changed. | P19-node24-runtime.txt
P20 | BLOCKED-EXTERNAL | Codex CLI is 0.20.0, not the required 0.167.0; sign-in state was not queried and no live-agent test was run. | P20-codex-version.command.txt; P20-codex-version.txt; P20-codex-version.exit.txt
G01 | PASS | Carried forward from v2 as instructed; npm ci bootstrap for this isolated build also exited 0. | P97-prior-v3-RESULTS.txt; bootstrap-npm-ci.exit.txt
G02 | PASS | Carried forward from v2: 52 desktop test files and 1,253 tests passed. | P97-prior-v3-RESULTS.txt
G03 | PASS | Fresh npm run build exited 0 under Node 24.15.0. | G03-npm-build.command.txt; G03-npm-build.exit.txt; G03-npm-build.stdout.txt; G03-npm-build.stderr.txt
G04 | PASS | Carried forward from v2; contract test exited 0. | P97-prior-v3-RESULTS.txt
G05 | PASS | Carried forward from v2: 97 script tests passed, 0 failed. | P97-prior-v3-RESULTS.txt
G06 | PASS | Carried forward from v2: 858 passed, 0 failed, 2 ignored across 73 test-result records. | P97-prior-v3-RESULTS.txt
G07 | PASS | Carried forward from v2; locked workspace clippy with warnings denied exited 0. | P97-prior-v3-RESULTS.txt
G08 | PASS | Locked Windows daemon build required by browser-proxy setup exited 0 and produced the daemon executable. | G08-daemon-build.command.txt; G08-daemon-build.exit.txt; G08-daemon-build.stdout.txt; G08-daemon-build.stderr.txt
G01/G02/G04-G08 carry-forward | PASS | Prior v2 gate results remain valid per v3; current-run G03 and the mandatory G08 browser prerequisite are recorded separately. | P97-prior-v3-RESULTS.txt; G03-npm-build.exit.txt; G08-daemon-build.exit.txt

N01 | BLOCKED-no-native-window | Unsigned installer built and completed without a UAC prompt; app window rendered but daemon was unavailable (daemon exited 1: cannot open daemon store). Required PNG capture failed because the Snipping Tool overlay could not be targeted; native procedure stopped. Uninstall completed with app-data deletion unchecked. | N01-package.txt; N01-sha256.txt; N01-install.txt; N01-native-window-blocker.txt
N02 | BLOCKED-no-native-window | Native theme/contrast checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N03 | BLOCKED-no-native-window | Native width/DPI checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N04 | BLOCKED-no-native-window | Native keyboard-only checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N05 | BLOCKED-no-native-window | Narrator/NVDA checks not attempted after the same required capture blocker; NVDA was not installed. | P13-desktop.txt; N01-native-window-blocker.txt
N06 | BLOCKED-no-native-window | Native reduced-motion and idle-resource checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N07 | BLOCKED-no-native-window | Packaged WebView2 route checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N08 | BLOCKED-no-native-window | Native named-pipe recovery check not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N09 | BLOCKED-no-native-window | Native recovery lifecycle not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL because Codex 0.20.0 is not the required 0.167.0. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
N10 | BLOCKED-no-native-window | Native approvals not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL because Codex 0.20.0 is not the required 0.167.0. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
N11 | BLOCKED-no-native-window | Native live-data checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
N12 | BLOCKED-no-native-window | Installed-package CSP/network checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt

F08 | BLOCKED-no-native-window | Native/live conversation checks not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F10 | BLOCKED-no-native-window | Native conversation-target checks not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F11 | BLOCKED-no-native-window | Native live-workstream checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
F12 | BLOCKED-no-native-window | Native live-activity checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
F14 | BLOCKED-no-native-window | Native inventory not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F15 | BLOCKED-no-native-window | Native WebView2 surface checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
F16 | BLOCKED-no-native-window | Native counts/search checks not attempted after the same required capture blocker; live data unavailable. | P13-desktop.txt; N01-native-window-blocker.txt
F17 | BLOCKED-no-native-window | Native device-resource DPI checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
F18 | BLOCKED-no-native-window | Installed renderer checks not attempted after the same required capture blocker. | P13-desktop.txt; N01-native-window-blocker.txt
F20 | BLOCKED-no-native-window | Native capacity/dispatch checks not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F22 | BLOCKED-no-native-window | Native usage popover checks not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F23 | BLOCKED-no-native-window | Native migration check not attempted after the same required capture blocker; no owner-supplied pre-migration data was used. | P13-desktop.txt; N01-native-window-blocker.txt
F24 | BLOCKED-no-native-window | Native Global/workstream conversation checks not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F25 | BLOCKED-no-native-window | Native/live checks not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt
F26 | BLOCKED-no-native-window | Native real-task usage check not attempted after the same required capture blocker; live-agent portion also BLOCKED-EXTERNAL. | P13-desktop.txt; P20-codex-version.txt; N01-native-window-blocker.txt

M6-04 W10 | WAIVED | Signed-installer validation remains waived; unsigned package behavior was attempted but stopped at the required capture blocker. | P97-prior-v3-RESULTS.txt; N01-native-window-blocker.txt
M6-04 W13 | WAIVED | N-1-to-N upgrade validation remains waived. | P97-prior-v3-RESULTS.txt
M6-04 W14 | WAIVED | Signed-installer smoke remains waived. | P97-prior-v3-RESULTS.txt
M7-32 fix-e2e focused real-daemon test | N/A | Windows real-daemon e2e harness is Unix-socket-only. | P97-prior-v3-RESULTS.txt

WIN-BP-N02 | PASS | Browser-proxy system theme and saved explicit preference checks carried forward from the v3 run at the original pin. | P97-carry-forward.txt; P97-prior-v3-RESULTS.txt
WIN-BP-N03 | PASS | Browser-proxy 10 viewport cases passed with no horizontal overflow or focus obstruction; carried forward. | P97-carry-forward.txt; P97-prior-v3-RESULTS.txt
WIN-BP-N04 | PASS | Browser-proxy keyboard navigation and focus restoration passed; carried forward. | P97-carry-forward.txt; P97-prior-v3-RESULTS.txt
WIN-BP-N06 | PASS | Browser-proxy reduced-motion check passed; carried forward. | P97-carry-forward.txt; P97-prior-v3-RESULTS.txt
WIN-BP-N07 | BLOCKED | Browser-proxy global/legacy routes and unknown-route history passed; task-detail routes need a real task ID and none was available. | P97-carry-forward.txt; P97-prior-v3-RESULTS.txt
WIN-BP-N08 | PASS | PR36-fix supplement verified named-pipe stop/restart and recovery without D3 placeholder at 9312e34; carried forward. | P97-carry-forward.txt; P97-prior-fix-RESULTS.txt
WIN-BP-N11 | PASS | PR36-fix supplement verified live-data honesty and host metrics without D3 placeholder at 9312e34; carried forward. | P97-carry-forward.txt; P97-prior-fix-RESULTS.txt
WIN-BP browser suite | PASS | npm run test:browser passed 11 tests at the pinned v3 commit; browser-proxy viewport and route observations remain carried forward from prior Edge run evidence. | WIN-BP-test-browser.command.txt; WIN-BP-test-browser.exit.txt; WIN-BP-test-browser.stdout.txt; WIN-BP-test-browser.stderr.txt; P97-carry-forward.txt; P97-prior-v3-RESULTS.txt

Run identity:
git rev-parse HEAD:
6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d
git status --porcelain=v1 --untracked-files=all: (empty)
