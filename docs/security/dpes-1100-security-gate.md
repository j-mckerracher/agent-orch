# DPES-1100 security gate (native unit)

Status: first unit only. The whole DPES-1100 board scope is not complete. See "Open gates".

## Commands

Run from the repository root. Scanner binaries are installed into a directory the runner reads; nothing is installed globally.

```
node scripts/security/gate.mjs install --dest <scanners-dir>
node scripts/security/gate.mjs controls --scanners <scanners-dir> --evidence-dir <dir>
node scripts/security/gate.mjs scan --repo . --base <sha> --head <sha> --scanners <scanners-dir> --evidence-dir <dir>
node --test scripts/security/gate.test.mjs
```

`install` downloads each pinned release archive, checks its SHA-256 against the pin in `scripts/security/gate.mjs`, extracts it, and checks `--version` against the pin.

## Pins

| Tool | Version | Platform | Archive | SHA-256 |
|---|---|---|---|---|
| Trivy | 0.76.2 | darwin-arm64 | trivy_0.76.2_macOS-ARM64.tar.gz | ae7ea358326960406b703c77b2ac490190797d80b306b5b50c2a27221da0a5b8 |
| Trivy | 0.76.2 | win32-x64 | trivy_0.76.2_windows-64bit.zip | 691f737a7e5566a673c3a74dab9160552ec32101e953b659e5c77443d16dddb6 |
| TruffleHog | 3.99.9 | darwin-arm64 | trufflehog_3.99.9_darwin_arm64.tar.gz | 1d85e5c9c7eddbf12ae2714eb89317ba2cbbbf59be4e161e4b2ff396adbaa3a8 |
| TruffleHog | 3.99.9 | win32-x64 | trufflehog_3.99.9_windows_amd64.tar.gz | da6754ea6f16be84fbda9860bdc49492edfa7a141eddc3ac900047fe88d16648 |

Hashes come from the upstream `*_checksums.txt` release manifests for v0.76.2 (Trivy) and v3.99.9 (TruffleHog).

## Scan semantics

- Trivy: `trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 --format json --output <work>/trivy.json --config "" --secret-config "" --ignorefile "" --cache-dir <work>/trivy-cache --timeout 15m --disable-telemetry --skip-version-check --no-progress .`, run from the repository root. The mandated options are `--scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1` and `.`. The rest is runner hardening: private JSON output, no repository-supplied config, secret config, or ignore file is read, the cache is task-local, and there is no telemetry or version check.
- TruffleHog: `trufflehog git file://. --since-commit <merge-base> --branch <head> --results=verified,unverified,unknown --no-verification --no-update --no-ignore-tag --fail --fail-on-scan-errors --json`. `<merge-base>` is the merge base of the base and head SHAs, `<head>` is the pull request head SHA, and the range is the pull request change set. `--no-verification` means no online credential verification.
- Exit codes: Trivy 0 (clean) or 1 (findings); TruffleHog 0 (clean) or 183 (findings). Any other exit, signal, timeout, abandoned pipe, malformed or mismatched evidence, version mismatch, dirty working tree, HEAD not equal to `--head`, or unresolvable merge base is ERROR.
- Runner exit codes: 0 PASS, 1 FAIL (findings or a refused configuration), 2 ERROR. ERROR never becomes PASS.
- Waivers: none exist. Strict default refuses the run (FAIL) when the tree tracks a Trivy ignore or config file (`.trivyignore*`, `trivy.y*ml`, `trivy-secret.y*ml`) or contains an inline ignore marker for either scanner.

## Report

`security-gate-report.json` in the evidence directory records: status, base, head, merge base, pinned versions, refusals, and per scanner the argv (scratch path replaced by `<work>`), exit code, signal, timeout flag, observed version, status, and reason. Findings are curated at capture time:

- Trivy: target, rule or vulnerability ID, severity, start line.
- TruffleHog: detector name, verified flag, file, line, commit.

Raw matches, code lines, redacted values, scanner stderr, and committer email addresses are never written. Raw scanner output lives in a scratch directory that is deleted at the end of each run.

## Controls

`controls` runs the real scanners against synthetic temporary git repositories:

- clean: Trivy PASS, TruffleHog PASS.
- iac-misconfig (`USER root` Dockerfile): Trivy FAIL.
- seeded-secret: a PAT-shaped token generated per run, committed in the scanned range. Trivy FAIL and TruffleHog FAIL. The token is never printed and never verified online. The run fails if the token appears in any evidence file.

## Workflow

`.github/workflows/security.yml` runs on `pull_request` only, on `macos-15` and `windows-2025`, with the same commands on both. No path filters, no `continue-on-error`, `contents: read` only, all actions pinned to commit SHAs already used in `ci.yml`. Each run uploads its evidence as an artifact.

## Open gates

- Product verifier integration for motif-mcao and motif-mcao-cloud: not started.
- Local pre-PR orchestrator gate: not started.
- Required status-check activation, branch protection, and rulesets: not changed; needs approval.
- Trusted-base enforcement: the workflow is read from the pull request head, so a pull request can change its own gate. Needs a base-owned workflow or reusable workflow before the result is independent proof.
- Scoped, time-limited waivers: owner and expiry mechanism unresolved. Strict refusal is the only behavior.
- TruffleHog remediation: deferred by board instruction. Findings are recorded, not waived; the gate blocks new findings in the change set.
- Native Windows execution: unverified until the workflow runs in CI.
- Cloud adoption: not started.
