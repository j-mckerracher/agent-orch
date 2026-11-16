TARGET LAPTOP: c (Windows, worker-c)

# Goal
Run the automated test suite of motif-mcao natively on this Windows laptop against the latest main (snapshot below) and report results. Report defects; do not fix code.

# Context
- Runbook (follow it top to bottom; it is the authority): https://github.com/j-mckerracher/agent-orch/blob/runbook/motif-mcao-windows-tests/docs/evidence/motif-mcao-windows-test-runbook.md  (branch `runbook/motif-mcao-windows-tests`, public repo `j-mckerracher/agent-orch`, file `docs/evidence/motif-mcao-windows-test-runbook.md`).
- Source snapshot: this task's base. Hub repo `j-mckerracher/agent-bus-hub-gub-gub`, branch `snap/win-mcao-400d542f`, commit `ea5b5abca917ff9bf8799a486b2e425afa755a87`. It is a history-free copy of mc-innersource/motif-mcao main `400d542f8faa035ad928c722f4470d73d72cca33` (tree `b56571f49d6492cac1ffd259b5b2e877e0a06921`). 1445 files; `.github/workflows` was removed on purpose. Fetch it with the hub access you already use for bus tasks. motif-mcao itself is not reachable from this laptop; do not try.
- Your shell is cmd.exe; use PowerShell as the runbook shows. Clear FM_STATE_OVERRIDE, FM_HOME, FM_DATA_OVERRIDE, FM_CONFIG_OVERRIDE, FM_PROJECTS_OVERRIDE and use an isolated CARGO_TARGET_DIR first.
- This replaces the earlier task 20261116-0001-win-mcao-e2e-f257a874 (cancelled).

# Deliverable
`RESULTS.md` per runbook section 5, plus `summary.md`, `overall.result.json` and failing-gate logs from the harness, in artifacts/. Put the harness verdict (PASS/FAIL/INCONCLUSIVE) and any count mismatch or failing gate in --summary. Use `busctl result`. Do not push evidence to any Git remote other than the hub task branch.

# Out of scope
Editing code, `scripts/ci/expected.mjs` or any workflow; installing software; elevation; logging in anywhere; installing the packaged app.

# When blocked
Use `busctl progress --state blocked --question ...`; do not guess.
