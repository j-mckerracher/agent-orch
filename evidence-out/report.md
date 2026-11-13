# M7-32 Windows consolidated check

## Remote changes and pins

Full scans compared all remote heads rather than the watcher baseline. At 07:44:08 UTC there were 148 heads. By 07:49:09 UTC, the remote had added the M7-32 macOS evidence branch and two aliases for the Windows runbook. At 08:14:47 UTC it had added the M7-32 axe-fix branch, advanced the two runbook refs, and advanced integration to 8f66dc9f175a60bf739fcf8e8c8b66e5feeb554e. A full 152-head scan at 08:21:31 UTC found no further changes. The inventory, relevant refs, and deltas are included.

The updated Windows runbook is commit 4c8b9589c72d8257bf964851e4d937c4230f0a3e. Its pinned source remains 92f56c3af9cd34741a09667f499164edcafec696 with tree a3012626c6808b337a719d0afcd4d887f5a504ea. The runbook update corrected two F25 filtered-test commands and requires at least one passing test per filtered run; both commands were run exactly and each passed one test.

The new axe-fix branch is orch/0eca0ef2-m7-32-fix-axe at 32059d14b4a48da405b003cd2db299bda6694ee6, tree 4d025a9a3b67d2e2447bf0096fd0051bdeca32c4. It changes the connection strip to a named region, updates its focused test, and adds Windows native screen-reader/DPI checklist steps.

## Automated Windows results

- Pinned runbook source: npm ci passed; desktop suite passed 52 files / 1,253 tests; production build passed with a Vite chunk-size advisory; contract test passed; script tests passed 97/97.
- Rust workspace: 858 passed, 0 failed, 2 ignored across 73 test-result records. Both updated F25 filtered commands reported 1 passed, 0 failed.
- Daemon examples/binaries build passed and produced the expected executable.
- Clippy failed because two single_match warnings in crates/orchestrator-runtime/src/supervisor.rs at lines 431 and 454 are promoted to errors by -D warnings. No source changes were made.
- Axe-fix branch: focused AppShell test passed 53 tests; full desktop suite passed 1,253 tests; build passed with the same chunk-size advisory. The branch checkout remained clean.

## Native blockers

The runbook preflight could not verify the active console/RDP display because query.exe was missing from PATH and Windows System32. user_interactive was true and WebView2 was detected, but the runbook requires the session query output; therefore native UI rows were marked BLOCKED and no app install or launch was attempted. The installed Codex CLI reports 0.20.0, not the required 0.167.0, so live-agent checks were also blocked. No screen-reader, DPI, installer, native approval, or usage result is claimed.

The M7-32 Windows accessibility checklist item for native WebView2/screen-reader and DPI remains blocked. The separate M6-04 W10/W13/W14 checks remain blocked for the missing installer/signing or released-artifact prerequisites.

## Execution notes

A wrapper initially printed a false clone failed status after the actual git clone command returned exit 0, because it treated a status label and the numeric exit code as one result. The command exit, correction note, and wrapper trace are all retained. The checkout matched the runbook commit and tree and remained clean.

No tracked source files were modified. No machine display, theme, animation, contrast, or screen-reader settings were changed. The evidence output retains all failed command output and status codes.

A further full scan at 08:27:31 UTC again found the same 152 remote heads. The initial local privacy scan found user-profile path references in three F25/G06 logs and authorization-pattern matches in the full Rust log; raw files are retained locally and the publication copies are redacted and rescanned before push. No screenshot was produced because native UI rows were blocked.

## Newly pushed E2E branch

The final remote scan found `orch/0eca0ef2-m7-32-fix-e2e` at `9e02b65e89a2591be3e3581d9dfd86b09313ef87`, adding a real-daemon E2E assertion change and a checklist that defers native work. The exact commit was checked out in a separate clean worktree, dependencies installed, and the daemon built. The focused E2E invocation exited 1 because the daemon did not become ready. The harness documents Unix-socket-only support and excludes Windows named pipes, so this Windows run cannot validate that assertion; the test and native checklist are reported as blocked/unverified. Raw command output is preserved in the E2E evidence files. The direct scan count increased from 152 to 153 solely because of this new branch.

## Publication

Evidence ref: evidence/m7-32-win-92f56c3af-20261113T084114Z. Parent: 92f56c3af9cd34741a09667f499164edcafec696. The publication copy redacted four files, and the final scan recorded zero matches. The raw command outputs remain in the local evidence directory; only the sanitized evidence-out copy is staged.

Publication text and TSV files were normalized to LF and trailing whitespace and extra blank lines at EOF were removed in the sanitized copy; raw local command outputs were retained. This made the staged evidence pass git diff --cached --check. This made the staged evidence pass git diff --check.
