# M7-32 Windows runbook v3 supplement

This supplement changes only the v2 rerun procedure. Use it with [the consolidated runbook](M7-32-windows-runbook.md) and [v2](M7-32-windows-runbook-v2.md); do not copy or repeat their unchanged setup and row procedures.

## Pin and scope

Use the same detached source pin and tree as v2: commit `6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d`, tree `62524d7c14d68bcca58c5b6d7cad762885eaf4f9`. Keep the checkout clean and do not change source files. Rerun P13 as informational, G03 only, N01–N12, and the F rows listed below. G01, G02 and G04–G08 remain PASS from v2; record that once and do not rerun them.

## P13 and N01

Run the v2 §2 session probe and record every field, including the foreground-window handle. P13 is informational only: none of its values blocks N01, native rows or the browser-proxy pass. Immediately after recording P13, attempt N01 (unsigned NSIS install, launch, inspect/capture, uninstall) using v1 §4.1. Do not change machine-wide settings unless the original value is recorded and restored before finishing.

If N01 cannot launch an inspectable window, or at any point a required screenshot/capture is denied or the window cannot be inspected, stop the native portion. Mark every in-scope row that requires a native window `BLOCKED-no-native-window` with the same single observed reason; do not claim those rows were attempted. Continue with G03 and the browser-proxy pass below. If native inspection works, continue the native N/F procedures in v1 §§4.1–4.2 and then run the browser-proxy pass as well.

## G03 only

Run G03 exactly once on either N01 outcome; it does not gate N01 or the remaining native rows. Use v1 §2 G03 (`npm.cmd run build` in `apps\desktop`). In the v1 §1.1 `Invoke-Gate` helper, around the native invocation, set `$ErrorActionPreference = 'Continue'` and `$PSNativeCommandUseErrorActionPreference = $false`. Save and restore the prior preference state in `finally` (remove the native preference variable afterward if it was initially undefined). The statement immediately after `& $Exe @Arguments ...` must be `$Code = $LASTEXITCODE`; do not run logging or other PowerShell statements first. Record the helper's exit-code file and build output; only exit code 0 is PASS. A stderr diagnostic alone is not a substitute for the native exit code.

## Mandatory Windows browser-proxy pass

Run this even if native checks pass. At the pinned clone, build the daemon using the v1 §2 G08 command, then run `npm run test:browser` in `apps\desktop`; record the command outcome and pass count. Use Microsoft Edge (`msedge`) already installed on the Windows host. Keep the Playwright script in a fresh temp directory outside the repository; add no repository dependencies and download no browser. Start `npm run dev` and connect it to an isolated real daemon over a named pipe. Browser-proxy results are not WebView2/native proof.

Exercise and screenshot these rows, recording each as PASS or FAIL:

- `WIN-BP-N02`: explicit themes, saved theme and `prefers-color-scheme`.
- `WIN-BP-N03`: widths 1280, 1150, 960, 760 and 480 at heights 700 and 900; no horizontal overflow and keyboard focus not obscured.
- `WIN-BP-N04`: keyboard traversal of skip link, navigation, workstream tree, target picker and review tabs.
- `WIN-BP-N06`: reduced-motion emulation.
- `WIN-BP-N07`: all global and legacy routes, unknown-route fallback, and browser back/forward.
- `WIN-BP-N08`: daemon stop/restart, connection strip, stale labels and recovery; live mode must never show fixtures.
- `WIN-BP-N11`: live-data honesty.

Label the method **browser-proxy** in every row, note and evidence index. Retain screenshots and the script's machine-readable observations. Use v1 §4.1 for unchanged expected behavior and route details.

## Remaining rows and dispositions

For native N01–N12 and the native/live portions of F08, F10, F11, F12, F14, F15, F16, F17, F18, F20, F22, F23, F24, F25 and F26, follow v1 §§4.1–4.2, applying the N01 no-window rule above. Do not repeat automated substeps already PASS in v2. Run live-agent portions only if Codex `0.167.0` is already signed in; otherwise mark those portions `BLOCKED-EXTERNAL` without logging in or recording authentication output. Signed-install rows remain WAIVED. The Windows real-daemon e2e harness remains N/A.

The native-only work deferred by this delivery is itemized in the [numbered native checklist](M7-32-win3-native-checklist.md).

Write `evidence-out/RESULTS.md` using v1 §5.1's row format and §5.2 redaction. Include one carry-forward line confirming G01/G02/G04–G08 PASS from v2 (not rerun), plus P13, G03, all in-scope N/F rows, and the seven `WIN-BP-*` rows. Use statuses `PASS`, `FAIL`, `BLOCKED`, `BLOCKED-no-native-window`, `BLOCKED-EXTERNAL`, `WAIVED` or `N/A` as applicable. Defects are recorded, not fixed. Publish only the evidence directory from the pinned parent to `evidence/m7-32-win3-6f2fe17f-<UTC timestamp>` with the v1 §5.3 identity and trailer; never force-push or include private run data.
