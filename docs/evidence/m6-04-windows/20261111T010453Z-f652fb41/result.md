# M6-04 native Windows build-box run

Source pin: 632c77db2d41a72bd5283013aa444798278fa9be.
Runbook pin: docs/evidence/m6-04-clean-machine-runbook.md, blob 88bc1c0402f415adcf4eb4ae0b3884550739322d.
Main remained 973b7ffd9c41ac0ae62817dc602ad38b41302957.

## Results

Node self-tests exited 1: 94 passed, 1 failed, 0 skipped, versus expected 95. Test 71, the macOS signed older-build path, expected forward-slash /w/Applications-old/... while Windows produced backslash-separated paths. Full sanitized TAP output is node-self-tests.txt.
npm ci first failed through PowerShell npm.ps1 with Unknown command: pm. npm.cmd retry exited 0, installed 155 packages, and reported 0 vulnerabilities. EBADENGINE warnings: jsdom@31.0.0 and w3c-xmlserializer@6.0.0 require Node ^22.22.2 or later listed versions, while release.yml pins Windows Node 22.20.0.
Windows package exited 0. Unsigned artifact Agent-Orchestrator-0.1.0-windows-x64-unsigned-setup.exe; SHA-256 250de3c3334501244cb4cb49c6eafdabd3cc707499826cfcf33767e1a811444c.
cargo build --locked -p orchestrator-daemon --example release_smoke exited 0.
install-smoke windows --allow-unsigned exited 0: 9 PASS, 0 FAIL, 1 INFO, 2 UNPROVEN. release_smoke passed 12 steps. No prior release was supplied, so this was a reinstall, not a proven upgrade.
No daemon or smoke process remained. The sentinel survived uninstall, was removed, and the previously absent LOCALAPPDATA/agent-orch directory is absent again.
Environment deviation: the runbook requests a throwaway Windows user or VM. This was a standard non-admin profile. Preflight found no existing app data, install directory or matching uninstall registration. Clean-machine behavior remains unproven.

## Finding

Runbook says a passing smoke deletes its work directory. This exit-0 run left release-smoke-result.json and AgentOrchestrator/uninstall.exe (size 79235; SHA-256 8CE2260C7CA9690FBFA367A41B662D5C700D3C8B41B83D34F0240D1FE768A0A7), although app and daemon executables were removed. Sanitized result and residue note are included; raw uninstaller is retained locally only.

Real signing, clean-machine behavior, SmartScreen, Defender, standard-user UAC, N-1 upgrade and GUI flow remain UNPROVEN.
Local cleanup of the generated work directory was attempted only after confirming its exact path, two expected files, no running test processes, and a matching preserved residue hash. The recursive cleanup command was rejected by local execution policy; the generated work directory remains on the host with only the recorded work residue.