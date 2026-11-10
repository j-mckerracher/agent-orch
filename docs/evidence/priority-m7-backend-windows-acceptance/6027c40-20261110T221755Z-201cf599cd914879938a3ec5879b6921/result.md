# M7 backend Windows run of 6027c40

overall: FAIL
source: 6027c4057be7ef80e93012d73173e6ffe09ac735 (tree f5932f532d25a9d5600e0a58d8e44dbe10474776)
runbook commit (operator-declared, not byte proof): b32966a4a3f9331c8a7d6a58c8a09cc38fc3dcc5
runbook blob: 0738f63fc77080fc0062d1f9e9a3f29014b8999a
run id: 20261110T221755Z-201cf599cd914879938a3ec5879b6921
OS: Microsoft Windows 11 Education 10.0.26200
toolchain: rustc 1.96.0 (ac68faa20 2026-05-25); cargo 1.96.0 (30a34c682 2026-05-25)
scripts directory present at the source: False (expected False; this source has no harness)
workspace-test totals: 733 passed, 1 failed, 1 ignored (derived 747 / 0 / 1)
counts differing from the derived table: 2

## Gates
- workspace-test: exit 101, 887 s
- clippy: exit 0, 103 s
- daemon-build: exit 0, 115 s

## Per-crate counts (derived vs measured)
```
crate                         derived  measured  diff   ignored   failed   macOS
orchestrator-agents               118       118     0       0/0        0     123
orchestrator-cli                   81        68   -13       0/0        0     105
orchestrator-core                  97        97     0       0/0        0      97
orchestrator-daemon               107       107     0       0/0        0     156
orchestrator-engine                 0         0     0       0/0        0       0
orchestrator-git                   87        86    -1       0/0        1      90
orchestrator-host-metrics          25        25     0       0/0        0      24
orchestrator-ipc                   55        55     0       0/0        0      55
orchestrator-runtime               42        42     0       0/0        0      55
orchestrator-store                135       135     0       1/1        0     137
TOTAL                             747       733   -14       1/1        1     842
```

## Proof steps
- T1 t1-host-metrics-crate: PASS - exit=0 passed=25 failed=0 expected=25
- T1 t1-host-metrics-daemon: PASS - exit=0 passed=2 failed=0 expected=2
- T1 t1-live-snapshot: PASS - cpu=ok memory=ok disk=ok gpu=unsupported; memory ratio 1; disk ratio 1; problems: 
- T2 t2-queue-snapshot-native: PASS - exit=0 line 'queue_snapshot: PASS' present=True
- T2 t2-queue-daemon-unit: PASS - exit=0 passed=19 failed=0 expected=19
- T2 t2-queue-ipc: PASS - exit=0 passed=3 failed=0 expected=3
- T3 t3-usage-daemon-unit: PASS - exit=0 passed=9 failed=0 expected=9
- T3 t3-usage-replay: PASS - exit=0 passed=3 failed=0 expected=3
- T3 t3-usage-store: PASS - exit=0 passed=12 failed=0 expected=12
- T4 t4-workstreams-core: PASS - exit=0 passed=13 failed=0 expected=13
- T4 t4-workstreams-store: PASS - exit=0 passed=12 failed=0 expected=12
- T4 t4-workstreams-upgrade: PASS - exit=0 passed=1 failed=0 expected=1
- T4 t4-workstreams-daemon-unit: PASS - exit=0 passed=10 failed=0 expected=10
- T4 t4-workstreams-ipc: PASS - exit=0 passed=4 failed=0 expected=4
- T5 t5-conversations-store: PASS - exit=0 passed=11 failed=0 expected=11
- T5 t5-conversations-daemon: PASS - exit=0 passed=2 failed=0 expected=2
- T5 t5-conversations-daemon-unit: PASS - exit=0 passed=16 failed=0 expected=16
- T5 t5-migration-fresh-and-upgrade: PASS - exit=0 passed=2 failed=0 expected=2
- T5 t5-migration-0031-present: PASS - 0031_conversations.sql present=True
- T5 t5-fails-closed-test-ran: PASS - transcript_fails_closed_survives_restart_and_grants_no_authority ok=True
- T6 t6-catalog-unit: PASS - exit=0 passed=12 failed=0 expected=12
- T6 t6-probe-build: PASS - exit=0 binary exists=True
- T6 t6a-daemon-ready: PASS - daemon running=True
- T6 t6a-cold-no-hang: PASS - callers=6 slow-or-error=0 max-ms=101
- T6 t6a-cold-single-flight: PASS - 6 concurrent cold callers produced login-status calls=1 and debug-models calls=1 (expected 1 and 1)
- T6 t6a-other-agents: PASS - claude-code=unavailable pi=unsupported (expected unavailable and unsupported)
- T6 t6a-cached: PASS - after a second call: login=1 debug=1 (expected 1 and 1 within the 30 s cache)
- T6 t6a-validate-valid-model-effort: PASS - valid=True reason= (expected valid=True reason=)
- T6 t6a-validate-valid-model-only: PASS - valid=True reason= (expected valid=True reason=)
- T6 t6a-validate-unknown-model: PASS - valid=False reason=unknown_model (expected valid=False reason=unknown_model)
- T6 t6a-validate-hidden-model-refused: PASS - valid=False reason=unknown_model (expected valid=False reason=unknown_model)
- T6 t6a-validate-effort-unsupported: PASS - valid=False reason=effort_unsupported (expected valid=False reason=effort_unsupported)
- T6 t6a-validate-effort-requires-model: PASS - valid=False reason=effort_requires_model (expected valid=False reason=effort_requires_model)
- T6 t6a-validate-agent-unsupported: PASS - valid=False reason=agent_unsupported (expected valid=False reason=agent_unsupported)
- T6 t6a-validate-agent-unavailable: PASS - valid=False reason=agent_unavailable (expected valid=False reason=agent_unavailable)
- T6 t6a-validate-empty-selection: PASS - result=InvalidParams (expected InvalidParams)
- T6 t6a-cache-expiry: PASS - after 32 s: login=2 debug=2 (expected 2 and 2)
- T6 t6a-daemon-alive: PASS - daemon running=True
- T6 t6b-daemon-ready: PASS - daemon running=True
- T6 t6b-cold-no-hang: PASS - callers=6 slow-or-error=0 unknown=6 max-ms=1109
- T6 t6b-cold-single-flight: PASS - 6 concurrent cold callers started login-status=1 and debug-models=1 probes (expected 1 and 1)
- T6 t6b-direct-child-killed: PASS - shim processes still running as direct children of the daemon 5 s after the cold wave: 0 (expected 0)
- T6 t6b-mid-call-no-hang: PASS - ms=1117
- T6 t6b-not-wedged: PASS - 38 s after the wave: login-status=2 debug-models=2 (expected at least 2 each: a failed read is not cached and the slot is free again)
- T6 t6b-daemon-alive: PASS - daemon running=True

## Problems
- gates with a non-zero exit: workspace-test
- failed tests in workspace-test: 1

## Findings
- COUNT orchestrator-cli: measured 68 passed and 0 ignored, derived 81 and 0
- COUNT orchestrator-git: measured 86 passed and 0 ignored, derived 87 and 0
- CLIPPY warning locations in orchestrator-runtime: 2 (the macOS baseline has warnings only in orchestrator-cli)
- CLIPPY warning locations in orchestrator-store: 2 (the macOS baseline has warnings only in orchestrator-cli)
- AGENTS-WINDOWS-KILL descendants of the timed-out hang shim alive 5 s after the wave: 2 (Windows kills only the direct child; the source documents this)
- AGENTS-WINDOWS-SLOT at 5 s after the cold wave, probes started: login-status=1 debug-models=1 (2 and 2 = the slot was released promptly after the kill; 1 and 1 = a descendant that held the output pipes kept the read open)

## UNPROVEN
- daemon queue, usage and model-selection through the executor on Windows (executor and api_models tests are unix-only; tests/daemon.rs is unix-only)
- killing the descendants of a timed-out probe on Windows (only the direct child is killed)
- a descendant that holds a probe output pipe: only observed by T6, not proven bounded
- upgrade of a store that has migration 0030 but not 0031 (only an upgrade from below 0030 is tested)
- conversations against a real agent (no concrete runtime; UnavailableRuntime fails closed; workstream targets are not wired)
- host.metrics GPU on Windows (derived unsupported; no value proven)
- real agent CLIs (T6 uses fake shims)
- all Windows counts (derived, not measured before this run)
- apps/desktop (npm, vitest, Tauri build, installer), a real window, GUI install and launch
- unproven-second-user
- unproven-remote-client
- unproven-slow-reader-64kib
- unproven-tauri-webview-click-path
- unproven-e2e-windows
- unproven-gui-install-launch
- unproven-gui-task-survives-close
- unproven-gui-event-reconnect
- unproven-stale-token-relaunch-timing
- unproven-daemon-idle-behaviour
- unproven-daemon-kill-and-sidecar-failure
- unproven-data-dir-acl (real host)
- unproven-launch-failure-redaction
- unproven-objective-64kib-r12
