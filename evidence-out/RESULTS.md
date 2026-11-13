# M7-32 G07 targeted Windows rerun

**Candidate source commit:** 22092e9e216369c7aaad3fa50b7eea3f2d495acb
**Candidate branch:** orch/0eca0ef2-m7-32-fix-clippy
**Parent integration commit:** 4b3ae153e6d213ef01571090499412b7c5d922b1
**Original Windows assignment pin:** 92f56c3af9cd34741a09667f499164edcafec696
**Runbook:** docs/evidence/M7-32-windows-runbook.md

## Result

| Gate | Status | Result |
| --- | --- | --- |
| G07 | PASS | cargo clippy --locked --workspace -- -D warnings exited 0 on the candidate Windows checkout; duration 80.3 seconds; zero warnings in captured stderr. |

This is a targeted rerun of G07 on the pushed clippy-fix candidate, not a rerun of the full consolidated Windows assignment. No native UI, WebView2, screen-reader, DPI, or live-agent checks were run. The original pinned-run result remains available on the prior evidence ref, where G07 failed on two single_match warnings.

## Setup notes

The first disposable-clone attempt was rejected before testing because checkout settings were applied after checkout and Git reported line-ending modifications. A second checkout had the correct SHA and clean tree but did not persist the local settings. Both attempts ran no test command. Local core.autocrlf=false and core.longpaths=true were then configured, the clean tree and exact candidate SHA were verified, and G07 ran successfully. See P00-setup-attempts.md for the setup record.

## Environment

Windows 11 Education, build 26200; cargo 1.96.0; rustc 1.96.0. The run used an isolated checkout and CARGO_TARGET_DIR under the disposable run root. See P05-candidate-checkout.txt, P06-toolchain.txt, and G07-cargo-clippy.* for raw command status and sanitized output.
