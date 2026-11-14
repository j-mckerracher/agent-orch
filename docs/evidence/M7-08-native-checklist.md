# M7-08 native verification checklist

**Status:** Deferred to the consolidated native pass. Run every applicable row on macOS and Windows from the same recorded source SHA; do not treat a skipped live-agent test as a pass. Preserve full logs and failed attempts.

For each OS, create a unique run root before starting. macOS: `export RUN_DIR="$(mktemp -d /private/tmp/m7-08-native.XXXXXX)"; export CARGO_TARGET_DIR="$RUN_DIR/cargo"; export AGENT_ORCH_DATA_DIR="$RUN_DIR/data"; mkdir "$RUN_DIR/evidence"`. Windows PowerShell: `$run = Join-Path $env:TEMP ("m7-08-" + [guid]::NewGuid().ToString("N")); if (Test-Path $run) { throw "run directory already exists" }; New-Item -ItemType Directory -Path $run | Out-Null; $env:CARGO_TARGET_DIR = Join-Path $run "cargo"; $env:AGENT_ORCH_DATA_DIR = Join-Path $run "data"; New-Item -ItemType Directory -Path (Join-Path $run "evidence") | Out-Null`. Record `git rev-parse HEAD` and the OS build.

1. **Daemon IPC lifecycle (macOS and Windows; deterministic fake agent).**
   From the repository root, run these commands with the unique `CARGO_TARGET_DIR` above:
   `cargo build --locked -p orchestrator-daemon --examples`
   `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --exact replies_are_ordered_correlated_and_carry_history_and_workstream_context`
   `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --exact replies_survive_a_restart_replay_by_sequence_and_stay_idempotent`
   `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --exact cancel_stops_a_streaming_reply_and_later_output_is_discarded`
   `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --exact a_turn_the_daemon_lost_is_failed_on_restart_not_left_running`
   **Expected:** all pass on the native OS; chunks remain ordered/correlated, cancel freezes the transcript, restart fails a lost turn closed, and replay preserves sequence and idempotent send. Save full stdout/stderr as `m7-08-<os>-daemon-lifecycle.log`. These tests use a fake agent; row 2 covers the real agent.

2. **Real Codex agent (macOS and Windows).**
   Steps: use the same fresh `CARGO_TARGET_DIR`, confirm Codex is signed in and its version is the approved pin `0.167.0`, set `AGENT_ORCH_LIVE_CONVERSATION=1`, and run `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --ignored --exact live_codex_answers_a_global_message --nocapture`. Do not add a new pin to make an unsupported version pass.
   **Expected:** the live test runs (not skipped), completes a global turn with a reply containing `pong`, and leaves task/approval lists empty. Record CLI version and authentication state; save full stdout/stderr as `m7-08-<os>-codex-live.log`.

3. **Native desktop WebView with a real daemon and Codex (macOS WKWebView; Windows WebView2).**
   Build from the repository root with `cargo build --locked -p orchestrator-daemon --bin orchestrator-daemon`. From `apps/desktop`, run `npm ci`. Then launch with the environment for the platform: macOS shell: `export AGENT_ORCH_DATA_DIR="$RUN_DIR/data"; export AGENT_ORCH_DAEMON_BIN="$CARGO_TARGET_DIR/debug/orchestrator-daemon"; export AGENT_ORCH_CONVERSATION_AGENT=codex; npm run tauri -- dev`. Windows PowerShell: `$env:AGENT_ORCH_DATA_DIR = Join-Path $run "data"; $env:AGENT_ORCH_DAEMON_BIN = (Join-Path $env:CARGO_TARGET_DIR "debug\orchestrator-daemon.exe"); $env:AGENT_ORCH_CONVERSATION_AGENT = "codex"; npm run tauri -- dev`.
   In the Workspace view select **Global orchestrator**, type `Reply with exactly: pong` in **Message Global orchestrator**, and click **Send message to Global orchestrator**. Confirm the reply appears and the turn completes. Send a second prompt requesting a long answer, wait for the first reply text, click **Cancel reply**, and confirm the cancelled status and no later chunks. Send `Reply with exactly: resume-check` and wait for completion. Find and terminate only the daemon process whose command line contains this run's exact `--data-dir`; wait for the app to reconnect/relaunch it and confirm the transcript content and order persist. Select an existing non-fixture workstream (create one in the UI if needed) and confirm its transcript is separate. Never terminate a process whose command line does not match the unique data directory.
   **Expected:** no simulated text; completed and cancelled states are accurate; restart/reconnect preserves the transcript. Save screen recording/screenshots, the run's `daemon.log`, OS build, Codex version, and process details under `$RUN_DIR/evidence/m7-08-<os>-webview/` (`$run\evidence\...` in PowerShell).

4. **Negative cases on each native OS.**
   Steps: after row 1 has built examples, run `cargo test --locked -p orchestrator-agents codex_without_credentials_offers_nothing`; `cargo test --locked -p orchestrator-daemon runtime_failures_fail_the_turn_with_a_stable_code_and_keep_the_message`; and `cargo test --locked -p orchestrator-daemon --test conversation_runtime a_version_that_is_not_pinned_never_receives_the_message`.
   **Expected:** missing credentials and permission denial are surfaced as stable turn failures; an unpinned version returns `version_mismatch` and the agent is not launched. Save full stdout/stderr as `m7-08-<os>-negative.log`.

No M7-08-specific screen-reader or DPI acceptance criterion is defined; assess those shared UI properties under the consolidated M7-32 accessibility/responsive checklist.