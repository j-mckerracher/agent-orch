# M7 final r1 rerun addendum (commit 6c17442)

This addendum supersedes nothing in `RUNBOOK.md`. It is **mandatory preflight**: do it before RUNBOOK section 1, then
follow the whole RUNBOOK again.

Why: the first r1 run was blocked. `C:` had 0 bytes free, `daemon-build` failed, and only the node self-tests ran. The
agent also cloned over HTTPS, had `core.autocrlf=true` and did not set `core.longpaths`.

| Item | Value |
|---|---|
| Commit to test | `6c17442eaee5243b48034dfc53d7f43d25f1ab82` |
| Tree | `9b9612ab6084c644e344603028149084118cc889` |
| New evidence ref (you push) | `evidence/m7-final-r1-rerun-6c17442-<UTC>-<hex>` |

Never reuse or delete an existing `evidence/*` ref. Pushing to a new ref is the only push you do.

## 1. Free space (first step, before anything else)

1. Record free space on `C:` and on every other volume you may use (for example
   `Get-PSDrive -PSProvider FileSystem | Select Name,Free`). Save it as `evidence/preflight/free-space-start.txt`.
2. You need at least **20 GB** free on the volume that holds the build output and temp files. Either:
   - free at least 20 GB on `C:` by deleting old `target` directories, build output of earlier runs, and temp files. Do not
     delete evidence refs or any evidence you have not yet pushed; or
   - set `CARGO_TARGET_DIR` and `TEST_TMP` (and `TEMP`/`TMP` for the session) to a directory on a volume with at least
     20 GB free, and use the same values for every step.
3. Verify with a second free-space check after the cleanup or the variable change. Save it as
   `evidence/preflight/free-space-ready.txt`, with the values of `CARGO_TARGET_DIR` and `TEST_TMP`.
4. If 20 GB cannot be reached, **stop**. Write `evidence/preflight/FAILED.txt` with the numbers and the reason, push the
   evidence ref, and report `BLOCKED`. Do not run partial gates.
5. Record free space again, with a timestamp, immediately before **each** `cargo` step (build, test, clippy, package,
   release). Append to `evidence/preflight/free-space-log.txt`. If a check shows less than 20 GB, stop and record it.

## 2. Clone and git settings

1. Use SSH: `git@github.com:j-mckerracher/agent-orch.git`. Use another path (for example HTTPS or a bundle) only if SSH is
   unavailable. If you do, record the reason and the exact path in `evidence/preflight/clone-method.txt`.
2. Set the options **before checkout**:
   ```
   git init <dir>; cd <dir>
   git config core.autocrlf false
   git config core.longpaths true
   git remote add origin <url>
   git fetch origin 6c17442eaee5243b48034dfc53d7f43d25f1ab82
   git checkout --detach 6c17442eaee5243b48034dfc53d7f43d25f1ab82
   ```
   If you used `git clone`, pass `-c core.autocrlf=false -c core.longpaths=true --no-checkout`, then check out.
3. Verify and save to `evidence/preflight/git-settings.txt`: `git config --get core.autocrlf` (false),
   `git config --get core.longpaths` (true), `git rev-parse HEAD`, and `git rev-parse HEAD^{tree}`.
   The tree must start with `9b9612ab` (full value above). If it does not, stop and report `BLOCKED`.

## 3. Rerun everything

Run the **entire** RUNBOOK again, including:

- the full **27-gate harness** (RUNBOOK section 2), every gate, with measured counts;
- the **conversation runtime diagnostic** (section 4);
- the **M7-25** live exchange (section 5) and the **M7-26** estimate (section 6);
- the **full M7-32 UI checklist** (section 7), with at least one screenshot for **every row group**:
  - themes: light, dark, system default, high contrast, and reduced motion;
  - DPI and responsive layout at several window sizes and scale factors (for example 100%, 150%, 200%; narrow, medium, wide);
  - Narrator;
  - daemon stop and restart: the connection-failure state and the recovery;
  - approvals.

Do not skip a gate or a row. A row you cannot run is `NOT RUN` with the reason. Never edit scripts, expected counts or tests
to pass a gate. Also run sections 3 and 8 of the RUNBOOK as written.

## 4. Evidence and push

Put all evidence, including the `evidence/preflight/` files and the screenshots, in the evidence tree of RUNBOOK section 9.
Push it to a **new** ref: `evidence/m7-final-r1-rerun-6c17442-<UTC>-<hex>` (UTC timestamp as `YYYYMMDDTHHMMSSZ`, random
hex suffix). Verify the push with `git ls-remote origin <ref>` and report the ref and its SHA. Use the final verdict wording
from RUNBOOK section 10.
