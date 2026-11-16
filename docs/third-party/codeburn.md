# codeburn: pinned usage reader and bundled Node.js runtime

Scan record and packaging rules for the local AI usage reader (DPES-1105). The release
build ships this reader beside the daemon, under `codeburn/` in the app's resources. The
daemon runs it with the bundled Node.js and checks every file it runs against
`crates/orchestrator-local-usage/codeburn-pin.json`.

Everything here is pinned. The packaging script refuses to stage anything that does not
match a pin, and `apps/desktop/src-tauri/sidecars/codeburn/codeburn-sidecar.test.mjs`
(run by `audit.yml`) fails when this record and the committed files disagree.

## What ships

| Part | Source | Pinned by |
| --- | --- | --- |
| `codeburn/node[.exe]` | Official Node.js 22.23.5 archive, the `node` binary only, unmodified | `sidecars/codeburn/node-runtime.json` (archive and binary SHA-256) |
| `codeburn/pkg/dist/main.js`, `parse-worker.js` | esbuild 0.28.2 bundles of the upstream codeburn 0.9.28 entries | `codeburn-pin.json` `layouts.bundled` |
| `codeburn/pkg/dist/cli.js` | Upstream launcher, unchanged | both layouts of `codeburn-pin.json` |
| `codeburn/pkg/package.json` | Upstream manifest (name and version checked) | `codeburn-pin.json` version, lockfile |
| `codeburn/NOTICES/` | codeburn LICENSE and THIRD_PARTY_NOTICES, Node.js LICENSE, CycloneDX SBOM | this record |

The staged layout is the seam in `crates/orchestrator-local-usage/src/service.rs`
(`ReaderLocations`): `<folder beside the daemon>/codeburn/` on Windows and in development,
and `Contents/Resources/codeburn/` in a macOS app bundle.

## Using codeburn in a development build

Release builds carry the bundle above and never read `PATH`. A development build
(debug) looks for the reader in this order: `AGENT_ORCH_CODEBURN_BIN`, then `PATH`,
then the common global folders (`/opt/homebrew/bin`, `/usr/local/bin`). Any other
version is refused, because only the pinned version is checked and run.

* An npm global install (`npm install -g codeburn@0.9.28`) passes the check.
* A Homebrew install of codeburn 0.9.28 also passes. Homebrew rewrites the
  `#!` line of `dist/cli.js`, which the check ignores, so its SHA-256 matches
  the `npm` layout. Checked on a 0.9.28 install: the reader reports `ok`.
* Node.js 22.15 or newer must be on `PATH`. Node 26 passes.

## Decisions and why

### Runtime: the official Node.js archive, trimmed to `node`

Node.js 22.23.5 is a security release. Its release notes list High-severity advisories in
http2, tls and http (CVE-2026-123863 to CVE-2026-123874), plus Medium and Low ones, so the
pin is 22.23.5 and not an older 22.x.

The SHASUMS256.txt signature was verified with GnuPG against the key
`5BE8A3F6C8A5C01D106C0AD820B1A390B168D356` (Antoine du Hamel, listed in
`nodejs/release-keys`). The copy committed here is `SHASUMS256.txt` with its `.sig`.

Options measured or rejected:

* **Official archive, trimmed (chosen).** The binary is not modified, so Node's Developer ID
  signature (macOS) and Authenticode signature (Windows) stay valid.
* **Single executable application (rejected).** A SEA needs the Node binary modified with a
  blob. That invalidates the official signatures and needs re-signing with MCAO's identity, which
  is unverified here. codeburn also starts `parse-worker.js` as a separate file, so a SEA would
  need an upstream patch. The SEA payload would be the same bundle as below, so there is no size
  gain to offset that.
* **Other runtimes (rejected).** Bun or Deno compiled binaries are not Node and do not
  run codeburn's Node code unchanged. There is no free, OSS option that is clearly smaller.

### Bundler: esbuild, two entries (chosen)

The upstream package installs about 10,075 files in 865 folders (46.3 MB apparent, 75 MB
on disk at 4 KB blocks). Most of that is dependencies the report path loads at startup
(ink, react, zod, the MCP SDK and others), plus 10 MB of source maps. The daemon's pin also
checked only 3 of those files.

esbuild 0.28.2 (exact, in `bundler/package-lock.json`) bundles the two upstream entries
(`dist/main.js` and `dist/parse-worker.js`) into one file each. The launcher `dist/cli.js` is
copied unchanged. Two decisions were needed:

* `react-devtools-core` is an optional development dependency of ink. ink imports it only
  when `DEV=true`, but esbuild hoists the static import, so it is replaced by an empty module.
  The reader's environment never passes `DEV`.
* Bundled CommonJS code calls `require()`. The bundle gets one from `createRequire`, the way
  the upstream launcher works.

Reader output on Node 22.23.5 was compared with the upstream tree for the `claude`, `codex` and
`copilot` providers on the same synthetic home: the JSON is identical except the `generated`
timestamp.

### Sizes

Measured on this Mac (darwin-arm64) with `gzip -9`. Installer compression is a separate
measurement, below.

| | darwin-arm64 | win-x64 |
| --- | --- | --- |
| Node.js binary, on disk | 113,024,160 bytes | 87,077,704 bytes |
| Node.js binary, gzip -9 | 36,882,656 bytes | 33,080,266 bytes |
| Reader files and notices, on disk (9 files in total with Node) | 10,395,538 bytes (about 10.4 MB) | 10,395,538 bytes |
| Reader files, tar+gzip -9 | 1,933,161 bytes | 1,933,161 bytes |
| **Total installed, on disk** | **123,419,698 bytes (about 123 MB)** | **97,473,242 bytes (about 97 MB)** (97,475,974 in the CI staging run; the 2,732-byte difference is the Node LICENSE, which the Windows archive stores with CRLF line endings) |
| **Compressed increase, measured: macOS app zip** (`ditto -c -k` on the shipped folder) | **38,974,335 bytes (about 39.0 MB)** | not applicable |
| **Compressed increase, estimate: Windows installer** (LZMA, `xz --format=lzma -9`, the algorithm NSIS uses) | not applicable | **about 23.4 MB** (node.exe alone about 22.2 MB; the reader adds about 1.2 MB) |
| Compressed increase, gzip -9 (for comparison) | about 38.8 MB | about 35.0 MB |

The Windows figure is an estimate. The NSIS installer itself was not built in CI: see
"Measured installer sizes" below.

The upstream tree, which was rejected, measured 10,075 files, 46,335,907 bytes apparent,
and 10,628,057 bytes gzipped. It would have added about 10.6 MB compressed instead of 1.9 MB.

### Cold first run

Claude provider, empty `CODEBURN_CACHE_DIR`, synthetic home, Node.js 22.23.5, darwin-arm64,
the bundle against the installed upstream tree. The first-touch row was measured on an earlier build of
the same sources (before the license comments and the install-root setting were added, which change
no code); the second row on the final staged bundle. Both are indicative only, measured under a load
average of 25 to 49 from endpoint scanning and a concurrent build:

| Measurement | Installed tree (cold, 3 runs) | Bundle (cold, 3 runs) |
| --- | --- | --- |
| First touch of the files after install (load 25 to 36) | 14.9 s, 8.7 s, 6.4 s | 1.0 s, 0.6 s, 0.5 s |
| After the files were scanned once (load 43 to 49) | 1.7 s, 1.3 s, 1.2 s | 0.37 s, 0.36 s, 0.41 s |

The bundle is faster in both, most on first touch, when endpoint scanning opens each of the
10,000 files. The first-touch figures are the larger ones and depend on the scanner.

## Scan record

The record below is read by `codeburn-sidecar.test.mjs`. Update it only through the upgrade
procedure.

<!-- scan-record:begin -->
```json
{
  "package": "codeburn",
  "version": "0.9.28",
  "tarball_source": "https://registry.npmjs.org/codeburn/-/codeburn-0.9.28.tgz",
  "tarball_integrity": "sha512-fl9u2m2n8n+cY6FheIY/jthw4GeuUtM38NfX3KR3Ljj2X7GFcUCfCGeRrnIaaM+aqmMLOw70Uy2BGnzoQgQlPQ==",
  "lockfile": "apps/desktop/src-tauri/sidecars/codeburn/package-lock.json",
  "lockfile_sha256": "a08a58a3245c1bea9e568b3bb406a89a1dac71faebaffde336426a6edb1ec3cf",
  "lockfile_packages": 163,
  "bundler_lockfile": "apps/desktop/src-tauri/sidecars/codeburn/bundler/package-lock.json",
  "bundler_lockfile_sha256": "479d1f5bc4bad2f70fcb54e7bd19d250e8a0543fa6b93571f30bd64cbca09fc8",
  "bundler": "esbuild 0.28.2",
  "sbom": "apps/desktop/src-tauri/sidecars/codeburn/sbom.cdx.json",
  "sbom_sha256": "adad4afb4983e7bf64b031e190844d805a727551ca1108241fa0a66cb333467f",
  "node": {
    "version": "22.23.5",
    "shasums": "apps/desktop/src-tauri/sidecars/codeburn/SHASUMS256.txt",
    "shasums_sha256": "658770bd465343e4bf441d22e89e7d944244b37041695f3f2a85b53d1a84b53f",
    "shasums_signature_sha256": "4d73b8d2bfb07f874a8fc426abbb317cf83e8dee8d9e14ad9837aa700b0700c7",
    "platforms": {
      "darwin-arm64": {
        "archive_sha256": "e7131995759b61a0170b646010d2ad35a5d37b4cb0d471f3e8317eac6f742f3d",
        "binary_sha256": "ff9365ccf73e95b7ac69867418124973addde63043650c0db96bd6a722c53cd8"
      },
      "win-x64": {
        "archive_sha256": "4a191082e9706866830379d577d2cc8a50c776fcd9436a8d061b063ed0d9a0e3",
        "binary_sha256": "247e9693d316da9857e44990e5e11709f596f18112251b8026ec6eed80bdf38a"
      }
    }
  },
  "trivy": {
    "version": "0.76.2",
    "vulnerability_db_updated_at": "2026-11-14T19:00:12Z",
    "scanned_at": "2026-11-15T05:39:46Z",
    "official_command_exit": 0,
    "results": {
      "HIGH": 0,
      "CRITICAL": 0,
      "MEDIUM": 0,
      "LOW": 0,
      "UNKNOWN": 0,
      "secrets": 0,
      "misconfigurations": 0
    },
    "licenses": {"packages": 163, "copyleft": 0, "unknown": 0, "names": ["MIT", "ISC", "BSD-3-Clause", "BSD-2-Clause", "Apache-2.0", "0BSD", "MIT OR CC0-1.0"]}
  },
  "trivy_bundler": {
    "command": "trivy fs --include-dev-deps --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 apps/desktop/src-tauri/sidecars/codeburn/bundler",
    "exit": 0,
    "packages": 27,
    "HIGH": 0,
    "CRITICAL": 0
  },
  "npm_audit": {
    "runtime_tree_high": 0,
    "bundler_high": 0
  }
}
```
<!-- scan-record:end -->

## Commands

Run from the repository root. Trivy reads the lockfiles and the installed trees; it does
not read the bundler output.

```sh
# Official gate (must exit 0 before a pull request is opened or updated)
trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .

# Runtime tree, all severities, with licenses
trivy fs --scanners vuln,secret,misconfig,license --severity LOW,MEDIUM,HIGH,CRITICAL,UNKNOWN \
  --format json --output <file> apps/desktop/src-tauri/sidecars/codeburn

# Bundler (build time only). Trivy skips npm devDependencies unless asked, and every
# bundler package is a devDependency, so the flag is required to scan anything here.
trivy fs --include-dev-deps --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 \
  apps/desktop/src-tauri/sidecars/codeburn/bundler

# SBOM (CycloneDX, with licenses)
trivy fs --format cyclonedx --scanners vuln,license --output \
  apps/desktop/src-tauri/sidecars/codeburn/sbom.cdx.json <copy of the sidecar with node_modules>

# npm advisories, lockfile only
npm audit --package-lock-only --omit=dev --audit-level=high   # in sidecars/codeburn
npm audit --package-lock-only --audit-level=high              # in sidecars/codeburn/bundler
```

The SBOM was generated from a scratch copy of the sidecar that has `node_modules` installed,
so that Trivy reads the license of each package. Trivy's SBOM also includes a timestamp and
generated identifiers, so regenerating it gives different bytes; the record pins the bytes
that were committed.

## Results

* Trivy 0.76.2, vulnerability database UpdatedAt 2026-11-14T19:00:12Z (DownloadedAt
  2026-11-15T01:52:07Z). Zero findings at every severity for the runtime lockfile, and zero
  secrets and misconfigurations in the sidecar folder. The bundler lockfile was scanned with
  `--include-dev-deps` (27 packages, esbuild and its platform binaries): zero HIGH and CRITICAL.
* The official gate (`trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL
  --exit-code 1 .` from the repository root) exits 0 and covers the runtime lockfile, but
  not the bundler: its packages are devDependencies, which Trivy skips by default. The
  bundler is therefore covered by the separate `--include-dev-deps` run recorded above.
* Licenses: 163 packages. MIT 145, ISC 8, BSD-3-Clause 5, BSD-2-Clause 1, Apache-2.0 1,
  0BSD 2, `MIT OR CC0-1.0` 1. No copyleft license.
* The lockfile lists 166 entries. They are 163 distinct name and version pairs: three nested
  entries repeat a name and version that another entry already has. The SBOM lists the 163
  distinct pairs plus the root component.
* npm audit at high severity: 0 for each lockfile.

## Coverage and its limits

Stated plainly, because the evidence is not complete:

* **Trivy does not scan the Node.js binary's internals.** It reads package manifests and
  lockfiles, not the contents of an executable. For the runtime the evidence is:
  1. The Node.js 22.23.5 release notes, which list every advisory fixed in this version
     (<https://nodejs.org/en/blog/release/v22.23.5>). The pin is that exact version.
  2. The SHA-256 of the archive and of the extracted binary, both pinned here, and the
     signature check on the release's checksum file (above).
  3. `osv-scanner` is not installed on the machine used for this record (`which osv-scanner`
     is empty), so it was not run. Running it on the archive would be the second scanner.
     It is not a requirement of this record.
* **Trivy scans the lockfile and the installed dependency tree, not the bundler output.**
  The bundles are generated from the locked tree, so the bundler output's correctness is
  covered by the rebuild check, not by Trivy: the smoke test rebuilds the bundle from the
  lockfile and compares it with the pin on each run (Windows and macOS in the harness, Linux in
  `audit.yml`).
* **esbuild** is a build-time dependency only. Trivy reads its lockfile when `--include-dev-deps`
  is given (see Commands). `npm ci --ignore-scripts` in `bundler/` installs
  its platform binary through `optionalDependencies` (`@esbuild/darwin-arm64` on this machine).
  The runtime install uses `npm ci --omit=dev --ignore-scripts`, which does not install esbuild.
* **Newer advisories** do not change this record. The nightly `audit.yml` run executes the
  npm advisory checks above and warns when a newer Node.js 22.x release exists.

## Node.js runtime: signatures and notarization

Verified in this record:

* SHASUMS256.txt: GnuPG "Good signature" from the Node.js release key above.
* Archive SHA-256 and extracted binary SHA-256: checked by `prepare-codeburn-sidecar.mjs`
  and by the smoke test on every run.
* macOS: `codesign --verify --strict` passes on the official binary and the packaging script
  checks it. The binary is Developer ID Application "Node.js Foundation (HX7739G8FX)" with the
  hardened runtime and a secure timestamp, and it carries `com.apple.security.cs.allow-jit` and
  `allow-unsigned-executable-memory`. V8 needs the JIT entitlement, so the packaging script
  fails if it is missing.
* Windows: `node.exe` has an Authenticode certificate (OpenJS Foundation, Microsoft ID Verified
  CS AOC CA 04). The packaging script runs `Get-AuthenticodeSignature` and requires status
  `Valid` and that subject.
* Tauri's signing (tauri-bundler 2.12.4 source, `bundle/macos/app.rs`) signs the sidecars in
  `Contents/MacOS`, frameworks and the app bundle with `codesign --force` and no `--deep`.
  Resources are not signed one by one. Locally, with an ad-hoc identity, a Tauri-style sign of an app
  holding the official `node` in `Contents/Resources` left the file's bytes and signature
  unchanged, with or without `--deep`, and `codesign --verify --deep --strict` passed. On
  Windows, Tauri signs resource `.exe` files only when `signtool verify /pa` fails, so the
  official signature is not replaced.

**Not verified here, and needs real credentials:**

* Signing with MCAO's Developer ID and notarization. None was run. No credentials were available.
* Whether Apple's notary service accepts an app whose nested `node` is signed by Node.js
  Foundation's team, and whether a child process started from the notarized app is blocked by
  Gatekeeper or quarantine.
* Windows Authenticode status after NSIS packaging, on a signed build.
* Whether the installed reader starts from the packaged app's resources on a clean machine.

### Release owner dry run (with credentials)

1. Run the release workflow with `require_signing: true` (workflow_dispatch) so an absent
   signing secret is an error, not an unsigned build.
2. macOS, on the artifact's app bundle:
   * `codesign --verify --deep --strict --verbose=2 "Agent Orchestrator.app"` must pass.
   * `codesign -d --entitlements :- "Agent Orchestrator.app/Contents/Resources/codeburn/node"`
     must list `com.apple.security.cs.allow-jit` and `allow-unsigned-executable-memory`.
   * `codesign -dvv ".../codeburn/node"` must show `Authority=Developer ID Application: Node.js Foundation (HX7739G8FX)` and `flags=0x10000(runtime)`.
   * Notarize: `ditto -c -k --keepParent "Agent Orchestrator.app" app.zip`, then
     `xcrun notarytool submit app.zip --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_PASSWORD" --wait`.
     Status must be `Accepted`. If not, `xcrun notarytool log <id>` names the file.
   * `xcrun stapler validate "Agent Orchestrator.app"` and `spctl -a -vv -t exec "Agent Orchestrator.app"`
     must report accepted and "Notarized Developer ID".
   * Run `"Agent Orchestrator.app/Contents/Resources/codeburn/node" --version`; it must print
     `v22.23.5`. A start failure with the hardened runtime means the JIT entitlement was lost.
3. Windows, in an installed copy (`<install dir>\codeburn\node.exe`):
   * `Get-AuthenticodeSignature` must report `Valid` and signer OpenJS Foundation.
   * `node.exe --version` must print `v22.23.5`.
4. Both: run the reader on a synthetic home, as the smoke test does, and record the output.

## Measured installer sizes

Not measured from CI. The release workflow's build jobs depend on the verify jobs, and the
verify jobs fail on main as well, so no installer was produced by the release workflow (runs
43682658590 with the bundle and 43681975293 on main, both linked from the pull request). The
failures are count drift in `scripts/ci/expected.mjs` for the daemon's unit tests and the
workspace tests on macOS and Windows, which the gates owner maintains. The installed-size and
compressed figures above are local measurements of the same staged files. The NSIS number
remains an estimate until a run is possible.

The Windows desktop package job (`npm run package:win`) ran inside CI on this branch's verify
jobs (run 43682658590). The bundled node.exe passed the Authenticode check (signer OpenJS Foundation,
status Valid), its binary hash matched the pin, and the staged reader ran against the
synthetic home.

## Reproducibility

* Two builds of the bundle from the same lockfile produce identical bytes, from the same
  checkout and from a copy in a different folder (absolute paths do not reach the output;
  esbuild's working directory is the install root).
* The pin check runs on Windows and macOS (`scripts/smoke/usage-reader.smoke.mjs`, in the
  "Usage reader smoke test" step of `ci.yml`, when the reader's inputs change) and on Linux (`audit.yml`). The same `main.js` and `parse-worker.js` hashes are
  required on every platform; a mismatch fails the test on that platform.

## Upgrade procedure (one change)

1. Check the new release notes and diff the package against the old one (review network
   calls, credential reads, telemetry, `sync` and `update`). `quota` reads login tokens and
   must stay unused.
2. Verify the tarball: `npm view codeburn@<new> dist.integrity` must match the SHA-512 of the
   packed file.
3. In `sidecars/codeburn/`, set the exact version in `package.json`, run
   `npm install --package-lock-only --ignore-scripts`, and in `bundler/` check the esbuild pin.
4. Run `npm ci --omit=dev --ignore-scripts` and `npm ci --ignore-scripts` (in `bundler/`), then
   `node bundler/bundle.mjs . <out>`. Copy `node_modules/codeburn/dist/cli.js` from the new
   package.
5. Update `crates/orchestrator-local-usage/codeburn-pin.json`: `version`, `tarball_integrity`,
   and both `layouts` (npm: the three upstream files; bundled: `cli.js` and the two new bundle
   hashes). This is the only daemon constant to change.
6. Re-run the reader parity check on the synthetic home for `claude`, `codex` and `copilot`
   against the new tree. If the JSON fields changed, update `parse.rs` and the fixture.
7. Rescan: the Trivy commands above, the SBOM, the npm audits, then update the scan record block
   (lockfile and SBOM hashes, Trivy results and DB date). Update `SHASUMS256.txt` only from a
   new Node.js release, with its signature verified.
8. Run `node --test --test-reporter=tap apps/desktop/src-tauri/sidecars/codeburn/codeburn-sidecar.test.mjs`
   and `node --test scripts/smoke/usage-reader.smoke.mjs`, and `cargo test -p orchestrator-local-usage`.

## Newer Node.js 22 releases

`check-node-updates.mjs` runs nightly in `audit.yml` and warns when a newer 22.x release
exists, naming the security releases. It never fails the build. A new Node.js line (for example
24.x) is a separate decision: it changes `node_minimum`, the archives and this record.
