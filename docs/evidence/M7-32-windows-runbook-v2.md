# M7-32 Windows native rerun runbook (v2)

| Item | Value |
|---|---|
| Pinned source commit | `6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d` |
| Pinned source tree | `62524d7c14d68bcca58c5b6d7cad762885eaf4f9` |
| Previous runbook | [M7-32 Windows consolidated native runbook](https://github.com/j-mckerracher/agent-orch/blob/runbook/m7-32-windows-92f56c3af/docs/evidence/M7-32-windows-runbook.md) |
| Evidence ref | `evidence/m7-32-win2-6f2fe17f-<UTC timestamp>` (`yyyyMMddTHHmmssZ`) |
| Machine | Same Windows 11 machine as the first pass. |

This is a scoped rerun supplement to v1, not a second copy of its native test descriptions. Use v1 sections 0–6 for safety, isolated run/evidence directories, tool setup, unchanged row steps, redaction and cleanup, except where this document overrides them. The pinned commit and tree above are fixed; do not substitute another SHA.

## 1. Scope

Create a fresh isolated clone at `6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d` as in v1 §1.2. Verify and record that detached `HEAD` is exactly `6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d`, its tree is exactly `62524d7c14d68bcca58c5b6d7cad762885eaf4f9`, and the checkout status is clean. This is pin/setup verification, not a rerun of the previous preflight rows. Do not repeat P01–P12 or P14–P20 as independent checks. Run only:

- P13, using the replacement check in §2.
- G01–G08 once each at the new pin. G01–G06 and G08 are the short re-verification; G07 is clippy and is expected to pass after F4. Use the commands, evidence naming and pass-count rules in v1 §2.
- N01–N12, and only the following F rows that were BLOCKED in the first RESULTS.md: F08, F10, F11, F12, F14, F15, F16, F17, F18, F20, F22, F23, F24, F25 and F26. Use their unchanged row steps and evidence lists in v1 §§4.1–4.2. Skip native rows/substeps already PASS in the first RESULTS.md, but run each G gate once at the new pin as listed above. For F08/F25, do not repeat automated substeps already PASS there; G06/G07 provide the new-pin automated re-verification.

## 2. P13 interactive-session check

Run once in PowerShell 5.1 or PowerShell 7 for this v2 run, after setting `$Out` from v1 §1.1. Keep using that PowerShell process for the remaining v2 commands so its isolated paths and environment remain set. Record the four required values in `P13-desktop.txt`: the PowerShell session ID, `UserInteractive`, whether a running `explorer.exe` has the same session ID, and the foreground window handle. Also record the matching Explorer PID/session ID when present.

```powershell
$P13SessionId = (Get-Process -Id $PID).SessionId
$P13UserInteractive = [Environment]::UserInteractive
$P13Explorer = @(Get-Process -Name explorer -ErrorAction SilentlyContinue |
  Where-Object { $_.SessionId -eq $P13SessionId })
$P13ExplorerSameSession = $P13Explorer.Count -gt 0
$P13ExplorerIds = ($P13Explorer | ForEach-Object { "$($_.Id):$($_.SessionId)" }) -join ','
$P13Foreground = $null
try {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class M7Win2User32 {
  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();
}
'@ -ErrorAction Stop
  $P13Foreground = [M7Win2User32]::GetForegroundWindow()
} catch { }
$P13ForegroundHandle = if ($null -eq $P13Foreground) { 'unavailable' } else { '0x{0:X}' -f $P13Foreground.ToInt64() }
$P13Failures = @()
if ($P13SessionId -eq 0) { $P13Failures += 'session_id=0' }
if (-not $P13UserInteractive) { $P13Failures += 'UserInteractive=false' }
if (-not $P13ExplorerSameSession) { $P13Failures += 'no explorer.exe in this session' }
if ($null -eq $P13Foreground -or $P13Foreground.ToInt64() -eq 0) { $P13Failures += 'foreground window handle is zero or unavailable' }
$P13Status = if ($P13Failures.Count -eq 0) { 'PASS' } else { 'BLOCKED' }
$P13Reason = if ($P13Failures.Count -eq 0) { 'none' } else { $P13Failures -join '; ' }
@(
  "session_id=$P13SessionId",
  "user_interactive=$P13UserInteractive",
  "explorer_same_session=$P13ExplorerSameSession",
  "explorer_pid_session=$P13ExplorerIds",
  "foreground_window_handle=$P13ForegroundHandle",
  "status=$P13Status",
  "reason=$P13Reason"
) | Set-Content -LiteralPath (Join-Path $Out 'P13-desktop.txt')
```

P13 passes only when the session ID is nonzero, `UserInteractive` is true, same-session Explorer is running, and the foreground handle is nonzero. Do not throw or stop the automated gates on a P13 failure. If any condition fails, run the automated gates but do not start the app or any native row; mark every in-scope N/F native row `BLOCKED` with the failed P13 condition(s).

## 3. Automated gates

Use the v1 §2 commands, same isolated `CARGO_TARGET_DIR`, and a fresh evidence root. Run each gate once. If G01 fails, mark G02–G04 not run/blocked as in v1, but continue independent gates. Record exit codes and pass counts for G02, G05 and G06. G08 must produce the daemon binary before native setup. Do not repeat the first pass's gates beyond this one new-pin verification.

## 4. Native rows and conditional rows

When P13 passes, run only the N/F rows listed in §1, following their row text in v1 §§4.1–4.2. N01 is the unsigned NSIS package install/launch/uninstall check; N12 uses that unsigned package. Do not attempt signing or change machine-wide settings. If a prerequisite for one row is absent, mark only that row BLOCKED with the specific reason and continue with independent rows.

Before any live-agent step, record `codex --version` and run the non-interactive `codex login status` check; record only the version and an already-signed-in true/false result, never auth output or credentials. Run live-agent steps only when the version prints `0.167.0` and the agent is already signed in. Do not login, install, or upgrade Codex. Otherwise mark the affected live-agent rows `BLOCKED-EXTERNAL` (credentials or unsupported CLI version) and move on. This applies to the live-agent portions of N09, N10, F08, F10, F11, F14, F16, F20, F22, F24, F25 and F26. If P13 itself fails, P13 is the blocking reason for all native rows and no app/live-agent step is attempted.

Other assignment dispositions:

- The M7-32 fix-e2e focused real-daemon harness row is `N/A` on Windows: the harness is Unix-socket-only, not a Windows failure.
- M6-04 W10 signed-installer validation, W13 N-1-to-N upgrade and W14 signed-installer smoke are `WAIVED` by the owner. Unsigned N01 remains in scope.
- Do not run macOS-only checks. Do not rerun any row already PASS in the first RESULTS.md.

## 5. Results and publication

Write `evidence-out/RESULTS.md` with one line per in-scope row, in this format:

```text
row id | PASS/FAIL/BLOCKED/BLOCKED-EXTERNAL/WAIVED/N/A | one-line note | evidence file(s)
```

Include P13, G01–G08, N01–N12, the listed F rows, M6-04 W10/W13/W14 and the Windows real-daemon harness N/A row. For rows blocked by P13, name the failed condition; for live-agent rows blocked externally, name the version/sign-in prerequisite without recording credentials. Apply v1 §5.2 redaction and privacy scanning to publication copies. Publish only `evidence-out/` on a new evidence branch based on `6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d`, using ref `evidence/m7-32-win2-6f2fe17f-<UTC timestamp>`, the `j-mckerracher <135927658+j-mckerracher@users.noreply.github.com>` author and committer, and the required Copilot co-author trailer. Never force-push or change `origin`. Keep the evidence directory until the owner confirms it; use v1 §6 for cleanup.
