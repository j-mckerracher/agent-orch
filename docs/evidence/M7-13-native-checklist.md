# M7-13 native review checklist

Run each row on the exact merged candidate SHA. Use a disposable, existing test project and real configured agent(s); do not use `?fixture=1`, fixture clients, mocks, or production approvals. Capture the full SHA, OS/build, WebView version, daemon transport, command output, and PASS/FAIL/UNVERIFIED in the row's `run.md`. Create a fresh evidence directory for every attempt; preserve failures and never overwrite prior evidence. If real daemon/agent data is unavailable, record UNVERIFIED rather than substituting fixtures.

1. **macOS / WKWebView — approve and request changes.** From the candidate checkout root, run:
   ```sh
   set -euo pipefail
   REPO="$(git rev-parse --show-toplevel)"
   RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$(uuidgen)"
   EVIDENCE="$REPO/docs/evidence/M7-13/native/macos/$RUN_ID"
   mkdir -p "$(dirname "$EVIDENCE")"
   mkdir "$EVIDENCE"
   git -C "$REPO" rev-parse HEAD > "$EVIDENCE/source.sha.txt"
   sw_vers > "$EVIDENCE/os.txt"
   node --version > "$EVIDENCE/node-version.txt"
   npm --version > "$EVIDENCE/npm-version.txt"
   cd "$REPO/apps/desktop"
   npm ci
   CARGO_TARGET_DIR="$REPO/target/m7-13-native-$RUN_ID" npm run tauri dev 2>&1 | tee "$EVIDENCE/tauri-dev.log"
   ```
   In the native app, confirm the real daemon is connected and Review & Decisions shows real (non-placeholder) approvals. Select a daemon-created candidate, inspect its diff, choose **Accept**, and verify the daemon records approval after refresh without requesting delivery. Select a second daemon-created candidate, submit **Request changes** with non-empty feedback, and verify the daemon records `changes requested`, not rejection. Capture `M7-13-macos-$RUN_ID-approve.png`, `M7-13-macos-$RUN_ID-request-changes.png`, and `M7-13-macos-$RUN_ID-run.md` in `$EVIDENCE`; record PASS/FAIL for both actions and close the app normally.

2. **Windows / WebView2 / named pipe — approve and request changes.** From the candidate checkout root in PowerShell, run:
   ```powershell
   $repo = (git rev-parse --show-toplevel).Trim()
   $runId = "{0}-{1}" -f (Get-Date -AsUTC -Format yyyyMMddTHHmmssZ), [guid]::NewGuid().ToString('N')
   $evidence = Join-Path $repo "docs/evidence/M7-13/native/windows/$runId"
   New-Item -ItemType Directory -Path $evidence | Out-Null
   git -C $repo rev-parse HEAD | Set-Content (Join-Path $evidence 'source.sha.txt')
   [Environment]::OSVersion.VersionString | Set-Content (Join-Path $evidence 'os.txt')
   node --version | Set-Content (Join-Path $evidence 'node-version.txt')
   npm.cmd --version | Set-Content (Join-Path $evidence 'npm-version.txt')
   $env:CARGO_TARGET_DIR = Join-Path $repo "target/m7-13-native-$runId"
   Set-Location (Join-Path $repo 'apps/desktop')
   npm.cmd ci
   npm.cmd run tauri dev
   ```
   Confirm the native WebView2 app is connected to the real daemon over its Windows named-pipe transport (record the actual pipe endpoint from host/daemon logs), and Review & Decisions shows real, non-placeholder approvals. Repeat the same two actions from row 1: approve one daemon-created candidate and verify persisted approval with no delivery request; submit non-empty request-changes feedback for a second candidate and verify the daemon records `changes requested`, not rejection. Capture `M7-13-windows-$runId-approve.png`, `M7-13-windows-$runId-request-changes.png`, and `M7-13-windows-$runId-run.md` in `$evidence`; record PASS/FAIL for both actions and close the app normally.
