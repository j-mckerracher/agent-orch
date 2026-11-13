# M7-32 Windows supplemental failed-row rerun

This is a follow-up to the v3 evidence on the original pinned commit. PR #36 merged the D3 provenance fix; this isolated rerun is pinned to the merge commit below and covers only the previously failed rows N08 and N11. It does not replace the v3 report or claim native/WebView2 acceptance.

WIN-BP-N08 | PASS | Microsoft Edge browser-proxy against a real isolated daemon: connected, stale/reconnecting after stop, and reconnected after restart; recovery stayed unknown and no D3 placeholder appeared in any state. | WIN-BP-N08-connected.png; WIN-BP-N08-stale.png; WIN-BP-N08-reconnected.png; WIN-BP-observations.json; WIN-BP-N08-harness.cjs
WIN-BP-N11 | PASS | Connected live daemon; usage unavailable; no D3 placeholder or fabricated cost/counts. UI host metrics matched host.metrics within the recorded thresholds; GPU was reported unsupported. | WIN-BP-N11-live-data-honesty.png; WIN-BP-N11-comparison.json; WIN-BP-observations.json; WIN-BP-N11-harness.cjs
PR36-focused-tests | PASS | Focused native/daemon/fixture regression tests: 3 files and 42 tests passed. | PR36-focused-tests.txt
G08-daemon-build | PASS | Locked Windows daemon build exited 0 and produced the daemon binary used by the browser-proxy checks. | daemon-build.txt
Task-detail real-daemon screen | BLOCKED | The isolated daemon had no real task; the test did not submit a task or start an agent. Focused tests cover live TaskDetail wiring with a FakeDaemon. | task-detail-limit.txt
M7-32 native/WebView2 checklist row 6 | BLOCKED-no-native-window | Browser-proxy rows passed, but native capture was denied; checklist row 6 remains UNVERIFIED. | native-blocker.txt

G01-npm-ci | PASS | Locked desktop dependency install exited 0 under Node 24.22.3. | npm-ci.txt

Run identity:
git rev-parse HEAD: 9312e34d520bf4df1e535caf90617aa95da5fac8
git status --porcelain=v1 --untracked-files=all: (empty)
