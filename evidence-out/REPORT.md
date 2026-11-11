# M7 final R1 Windows acceptance — incomplete, blocked

## Pin and environment

- Runbook branch: `docs/m7-final-r1-windows-acceptance-6c17442` at `003d66790652e8a61da29ef680273f20575dac61`.
- Source branch: `orch/m7-final-r1-6c17442`.
- Tested source commit: `6c17442eaee5243b48034dfc53d7f43d25f1ab82`.
- Source tree: `9b9612ab6084c644e344603028149084118cc889`.
- Windows version string: `Microsoft Windows NT 10.0.26200.0`.
- Tool versions: Node `v22.20.0`; npm `11.3.0`; rustc `1.96.0 (ac68faa20 2026-05-25)`; cargo `1.96.0 (30a34c682 2026-05-25)`; PowerShell `7.5.4`.
- `core.autocrlf`: `true`. The initial source status was clean; the source checkout is clean at evidence capture. `checkout_clean_after_run` is **NOT MEASURED** because the harness stopped before its post-run status capture.
- The prescribed SSH clone failed with `Permission denied (publickey)`. A public HTTPS clone was then used without a token or `gh`; the pinned commit and tree both matched. This is a setup deviation.

## Section 2 — full gate harness

- Preflight: **PASS**, Windows (`win32`), exit 0.
- Gate list: **27** gates, `--list` exit 0.
- Full harness: started once. The daemon build failed after 66 seconds with Rust output errors caused by disk exhaustion. The subsequent IPC gate began compiling, then failed to write outputs for the same reason. The captured PowerShell command returned exit 1; `gates.exit` was not written because the volume had no space. No retry was made.
- At failure, C: had 0 free bytes. At this report capture, it had about 66 MiB free, still insufficient to finish the native gates.

| Gate | Expected | Measured | Equal? |
|---|---|---|---|
| ipc | 56 | NOT MEASURED; compilation interrupted by disk exhaustion | No result |
| windows-transport | 14 passed, 42 filtered out | NOT RUN | — |
| daemon | 138 + 1 ignored | NOT MEASURED; daemon build failed before tests | No result |
| store | 153 + 1 ignored | NOT RUN | — |
| lock-takeover | 1 | NOT RUN | — |
| tauri | 14 | NOT RUN | — |
| node self-tests | 97 | 97 passed, exit 0 | Yes |
| vitest | 1105 (51 files) | NOT RUN | — |
| gates | 27 listed | 1 pass, 1 build failure, IPC interrupted, 24 remaining not run | Incomplete |

Overall status: **INCOMPLETE / BLOCKED**. The daemon build failure is an environmental disk-space failure, not a test assertion result. The IPC test suite did not produce a result file.

## Sections 3–8

- Section 3 release/package/install/uninstall gates: **NOT RUN**; the full gate harness stopped on disk exhaustion.
- Section 4 conversation runtime diagnostics and backend probes: **NOT RUN**; M7-08 and M7-25 are **UNPROVEN** on this Windows run. No probe outcome or diagnostic trigger line was measured.
- Section 5 live conversation: **NOT RUN**; prerequisite/login status was not checked after the harness stopped.
- Section 6 usage checks: **NOT RUN**.
- Section 7 manual UI checklist: **NOT RUN**.
- Section 8 N-1 upgrade: **NOT RUN / UNPROVEN**; artifact availability was not assessed.

## Section 9 — evidence and privacy

- This report describes the partial run only; it does not represent overall acceptance.
- Preserved gate outputs are in `evidence-out/`. User profile paths, the user name, and the school email domain were redacted from the pushed copy. The privacy scan passed.
- Disk space prevented creating the runbook's separate evidence worktree. Evidence was assembled from the existing pinned clone into a separate staging directory; tracked source files were not changed.
- Remaining work: restore several GB of free space, rerun this pinned assignment from a fresh isolated root, complete the required diagnostics and release/manual checks, and report all measurements without adjusting expectations.

