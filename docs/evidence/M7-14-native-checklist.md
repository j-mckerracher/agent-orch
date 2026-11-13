# M7-14 native checklist

These checks are deferred to the consolidated native pass. Run against the exact integrated source SHA and record it with each result. Do not mark a row PASS unless the named native surface and real daemon scenario ran.

## Scenario and evidence rules

- Use a disposable registered repository and the real daemon with real configured agent binaries. Prepare one task with a real Running session and one task with a real Failed session; retain their attempt history and session events. Do not use `FakeDaemon`, browser/e2e fixtures, fixture mode, or the standalone `orchestrator-cli` SQLite store as daemon data. The CLI store is separate from the daemon's IPC store.
- Keep the tasks and repository isolated from user work. If the daemon or safe real-agent scenario cannot provide the required states, record the exact missing prerequisite and mark the affected row UNVERIFIED; do not synthesize native evidence.
- Use one unique run ID and evidence directory per platform/run. Preserve failed output and screenshots; never overwrite a previous run. Include the full source SHA, OS version, daemon connection status, real agent inventory, commands, exit status, and observed result.

## Numbered checks

1. **macOS / WKWebView / real daemon and agents — AC1–AC4.** From the task worktree root, run:
   ```sh
   WT="$(git rev-parse --show-toplevel)"
   test "$(git -C "$WT" branch --show-current)" = "orch/0eca0ef2-m7-14-verify"
   RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$(uuidgen)"
   EVIDENCE="$WT/docs/evidence/M7-14-macos-$RUN_ID"
   mkdir "$EVIDENCE"
   printf '%s\n' "$(git -C "$WT" rev-parse HEAD)" > "$EVIDENCE/source-sha.txt"
   cd "$WT/apps/desktop"
   export CARGO_TARGET_DIR="/private/tmp/m7-14-$RUN_ID-cargo"
   mkdir "$CARGO_TARGET_DIR"
   npm ci
   npm run tauri dev
   ```
   In the native WKWebView, verify Connected and live (no PLACEHOLDER DATA), compare every displayed installed/launchable/capability flag against the real `agents.capabilities` response, and confirm unreported usage is Unknown. In particular, verify Pi is installed and `launchable: false` when Pi is reported; do not infer flags for missing agents.
   Check the headline and all five tab labels/lists against the real tasks/sessions (including disconnected as unknown when recovery classification is unavailable). Confirm Available equals the count of reported adapters with both installed and launchable true. Select the running and failed tasks; verify attempt history and the session execution feed. On the disposable running task request Cancel and confirm the daemon acknowledgement does not claim the process stopped. On the disposable failed task Retry and confirm a new queued attempt.
   Verify Configure providers, Amend TaskContract, Request new decomposition, Resume session, Inspect retained checkpoint, Review routing proposal, and Choose manual assignment stay disabled with readable reasons. Record PASS/FAIL/UNVERIFIED per AC. Evidence: `macos-live-agents.png` and `macos-live-agents.md` in `$EVIDENCE`.

2. **Windows / WebView2 / real daemon and agents — AC1–AC4.** In PowerShell from the task worktree root, run:
   ```powershell
   $wt = (git rev-parse --show-toplevel).Trim()
   if ((git -C $wt branch --show-current).Trim() -ne 'orch/0eca0ef2-m7-14-verify') { throw 'Wrong worktree branch' }
   $runId = [guid]::NewGuid().ToString('N')
   $evidence = Join-Path $wt "docs\evidence\M7-14-windows-$runId"
   if (Test-Path $evidence) { throw 'Evidence path already exists' }
   New-Item -ItemType Directory -Path $evidence -ErrorAction Stop | Out-Null
   (git -C $wt rev-parse HEAD).Trim() | Set-Content (Join-Path $evidence 'source-sha.txt')
   Set-Location (Join-Path $wt 'apps\desktop')
   $env:CARGO_TARGET_DIR = Join-Path $env:TEMP "m7-14-$runId-cargo"
   if (Test-Path $env:CARGO_TARGET_DIR) { throw 'Cargo target path already exists' }
   New-Item -ItemType Directory -Path $env:CARGO_TARGET_DIR -ErrorAction Stop | Out-Null
   npm.cmd ci
   npm.cmd run tauri dev
   ```
   In the native WebView2 window, repeat check 1 with the same isolated real-daemon scenario and assertions. Record PASS/FAIL/UNVERIFIED per AC. Evidence: `windows-live-agents.png` and `windows-live-agents.md` in `$evidence`.

3. **macOS / VoiceOver.** In the native WKWebView from check 1, enable VoiceOver with `Command-F5`, navigate the Agents & Sessions page with Tab and VoiceOver navigation, and activate each tab. Confirm the selected tab and count are announced, every inventory flag is associated with its adapter, and each disabled control's reason is read with its control. Disable VoiceOver with `Command-F5`. Evidence: `macos-voiceover.md` in the check 1 evidence directory.

4. **Windows / Narrator.** In the native WebView2 window from check 2, enable Narrator with `Windows+Control+Enter`, navigate by Tab and Narrator reading commands, and activate each tab. Confirm selected tab/count announcements, adapter/flag association, and disabled-control reasons. Disable Narrator with `Windows+Control+Enter`. Evidence: `windows-narrator.md` in the check 2 evidence directory.

5. **macOS / native display scaling.** With the WKWebView open, use System Settings → Displays to test the available default and increased-workspace scales. At each scale inspect the page and inventory table, keyboard-focus every tab/control, and confirm labels/reasons remain visible and no control is clipped; the inventory's own horizontal scroller may scroll. Record the selected scale and screenshots. Evidence: `macos-display-scale-default.png`, `macos-display-scale-increased.png`, and `macos-display-scale.md` in the check 1 evidence directory.

6. **Windows / native DPI scaling.** With the WebView2 window open, set Settings → System → Display → Scale to 100%, 150%, and 200%, restarting the app only if Windows requests it. At each scale inspect the page and inventory table, keyboard-focus every tab/control, and confirm labels/reasons remain visible and no control is clipped; the inventory's own horizontal scroller may scroll. Record the selected scale and screenshots. Evidence: `windows-dpi-100.png`, `windows-dpi-150.png`, `windows-dpi-200.png`, and `windows-dpi.md` in the check 2 evidence directory.
