# treehouse: pinned worktree tool

Scan record and packaging rules for treehouse (DPES-1044), the worktree pool that MCAO uses
for every repository task. The release build ships treehouse beside the daemon. The daemon
runs it to lease and return worktrees under `<data>/worktrees/`.

Everything here is pinned. The staging script refuses to stage a file that does not match the
pin, and `apps/desktop/src-tauri/sidecars/treehouse/treehouse-sidecar.test.mjs` (run by
`audit.yml`) fails when this record and the committed files disagree.

## What ships

| Part | Source | Pinned by |
| --- | --- | --- |
| `treehouse` (macOS) or `treehouse.exe` (Windows), the upstream v3.1.8 release binary, unmodified | GitHub release `v3.1.8` of `kunchenguid/treehouse` (commit `f168807c583ec3afaec7635806b3688d5530e945`) | `sidecars/treehouse/treehouse-pin.json` (archive and binary SHA-256) |
| `checksums.txt` | The upstream release's checksum file, committed as published | `treehouse-pin.json` `checksumsSha256`, this record |

Platforms: `darwin-arm64` (`treehouse-v3.1.8-darwin-arm64.tar.gz`) and `win-x64`
(`treehouse-v3.1.8-windows-amd64.zip`). Those are the two platforms the packaging
scripts support, as for the Node.js runtime. Nothing else is bundled.

The staged path is `Contents/MacOS/treehouse` in the macOS app and `treehouse.exe` in the
install folder, beside `orchestrator-daemon`. The daemon finds it there in a release build
(`ExecutorConfig::for_data_dir`).

## Decisions and why

### The pin is enforced at staging, not in the bundle

Upstream publishes SHA-256 checksums (`checksums.txt`) and no signature. The staging script
(`apps/desktop/scripts/prepare-treehouse-sidecar.mjs`) checks the archive's SHA-256 and the
extracted binary's SHA-256 against the pin, and the binary's `--version` output, before it copies
anything. It stops on the first mismatch.

Tauri re-signs an `externalBin` file when it bundles it, so the bytes that ship are not the
upstream bytes. The pin therefore does not reach the installed file. The release smoke already
requires the daemon `externalBin` to carry the app's team identifier
(`scripts/release/install-lib.mjs`, the sidecar check), so re-signing is the existing assumption,
and treehouse is checked the same way. Who built the file is established by the pin; that the
shipped file is the pinned file is established by the signature.

### The binary is the upstream release build, not a rebuild

MCAO does not compile treehouse. A Go rebuild would not be byte-identical to the release, and
nothing in the pin would prove the rebuild. The binary is taken as published and checked by hash.

### One private HOME for every treehouse call

The daemon runs treehouse with `HOME` and `USERPROFILE` pointing at a private directory
(`<data>/treehouse-home/`). Treehouse reads `~/.config/treehouse/config.toml` and runs
`[hooks]` from the user's configuration on `get`. The private home keeps user hooks from running
inside MCAO's worktrees. The daemon writes the one setting it needs there (`max_trees`), and
`TREEHOUSE_NO_UPDATE_CHECK=1` keeps the update check off.

## Scan record

The record below is read by `treehouse-sidecar.test.mjs`. Update it only through the upgrade
procedure.

<!-- scan-record:begin -->
```json
{
  "package": "treehouse",
  "version": "3.1.8",
  "source": "https://github.com/kunchenguid/treehouse",
  "tag": "v3.1.8",
  "commit": "f168807c583ec3afaec7635806b3688d5530e945",
  "license": "MIT",
  "checksums": "apps/desktop/src-tauri/sidecars/treehouse/checksums.txt",
  "checksums_sha256": "2614f7081d68018fb975ad8e25624201ec2cf0d3c0d63bd4a8910c59ff2003e9",
  "platforms": {
    "darwin-arm64": {
      "archive_sha256": "007c55c26e0bd8b672b1a917465f06c3f89d93a66cd171364ffb65743d268601",
      "binary_sha256": "3757e3f5f38800036495b4744799db634bb1d77eb9073bc88de4f0805e1e77b7"
    },
    "win-x64": {
      "archive_sha256": "72e7d61886e80e2befa067f5fab1c7ce2afbb49ba79c654cb7f4fc026cbc6b33",
      "binary_sha256": "b00743793a8bb53fc0c0df0b5e7c3ba7066b71700da210124e669c924f3d7cab"
    }
  },
  "trivy": {
    "version": "0.76.2",
    "vulnerability_db_updated_at": "2026-11-18T13:03:23Z",
    "scanned_at": "2026-11-18T19:30:11Z",
    "official_command_exit": 0,
    "results": {
      "HIGH": 0,
      "CRITICAL": 0,
      "MEDIUM": 0,
      "LOW": 0,
      "UNKNOWN": 1,
      "secrets": 0,
      "misconfigurations": 0
    },
    "licenses": {"packages": 2, "copyleft": 0, "unknown": 0, "names": ["Apache-2.0", "MIT"]}
  }
}
```
<!-- scan-record:end -->

## Commands

Run from the repository root.

```sh
# Official gate (must exit 0 before a pull request is opened or updated)
trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .

# Upstream source at the pinned tag, the same gate and all severities with licenses
git clone --depth 1 --branch v3.1.8 https://github.com/kunchenguid/treehouse.git <checkout>
trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 <checkout>
trivy fs --scanners vuln,secret,misconfig,license --severity LOW,MEDIUM,HIGH,CRITICAL,UNKNOWN \
  --format json --output <file> <checkout>

# Embedded module versions of each shipped binary (Go build info, no execution)
go version -m <extracted treehouse binary>
```

## Results

* Trivy 0.76.2, vulnerability database UpdatedAt 2026-11-18T13:03:23Z. The official gate
  (`--severity HIGH,CRITICAL --exit-code 1`) exits 0 on the upstream source and on the
  extracted binary folder of each platform.
* One finding at UNKNOWN severity, not in the HIGH/CRITICAL gate: **CVE-2026-39824** in
  `golang.org/x/sys` v0.41.0 (integer overflow in `NewNTUnicodeString`, `golang.org/x/sys/windows`).
  The fixed version is 0.44.0. The module is an upstream dependency, so the fix is an upstream
  release, not a local patch. It is in both shipped binaries' dependency lists. This record does
  not establish whether treehouse reaches the vulnerable function. Decide whether to accept it
  before the Windows build ships.
* Licenses: `gopkg.in/yaml.v3` (Apache-2.0 and MIT notices) and `github.com/mattn/go-isatty` (MIT).
  No copyleft license.
* Treehouse itself is MIT (`LICENSE` at the pinned tag).

## Coverage and its limits

* **Trivy does not read the binary's contents.** The binaries produced no scan targets. The
  module versions come from Go's build info instead: `go version -m` on each binary lists its
  modules (`golang.org/x/sys` v0.41.0 and `github.com/mattn/go-isatty` v0.0.20 among them), and
  every one matches the version in the upstream `go.mod`. Those are the modules Trivy scanned
  from source.
* **The binary is not built from the pinned source here**, so the source scan shows what the
  release was built from, not what the binary contains. The commit in the pin is the release's
  commit, and the binary's embedded VCS revision (`vcs.revision`) equals it, with `vcs.modified=false`.
* **Trivy scanned only the source checkout.** The binaries were scanned with the same gate, but
  as folders, which contain no lockfile.

## Not verified here

* Developer ID signing of the re-signed `treehouse` in the macOS app, and notarization. No
  credentials were available. The release owner dry run must show `codesign -dvv` on
  `Contents/MacOS/treehouse` with the app's `TeamIdentifier`.
* Authenticode status of `treehouse.exe` after Tauri signs it, on a signed Windows build.
* The MIT license notice: the bundle does not carry treehouse's `LICENSE` text. The repository
  ships no notices file for its other dependencies either, so this needs a decision for the
  whole app, not only this file.

## Upgrade procedure (one change)

1. Read the release notes and the diff of the new tag against the old one. Check network calls,
   credential reads and how `get` runs hooks, because the daemon's private HOME depends on them.
2. Download the new release's `checksums.txt` and archives into an empty directory outside the
   repository. Check each archive against `checksums.txt`, then extract only the binary into another
   directory and check its SHA-256.
3. Update `sidecars/treehouse/treehouse-pin.json` (version, tag, commit, `distBase`,
   `checksumsSha256`, both platforms) and replace `sidecars/treehouse/checksums.txt`.
4. Rescan: the Trivy commands above, then update the scan record block (hashes, Trivy results and
   DB date, scan time).
5. Run `node --test --test-reporter=tap apps/desktop/src-tauri/sidecars/treehouse/treehouse-sidecar.test.mjs`
   and `node apps/desktop/scripts/prepare-treehouse-sidecar.mjs --dest <empty dir>`.
