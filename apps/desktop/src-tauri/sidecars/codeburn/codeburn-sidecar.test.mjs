// Keeps the codeburn scan record (docs/third-party/codeburn.md) true to the committed files: the
// lockfiles, the SBOM, the Node.js runtime pins and the pin file. Run by audit.yml on every change to
// the sidecar and nightly. It never runs Trivy or npm; it checks what was recorded at scan time.
//
//   node --test --test-reporter=tap apps/desktop/src-tauri/sidecars/codeburn/codeburn-sidecar.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  lockEntry,
  lockfileProblems,
  readLockfile,
  readNodeRuntime,
  readPin,
  repoRoot,
  runtimeProblems,
  sha256OfFile,
  sidecarDir,
} from "./lib.mjs";
import { newerNode22 } from "./check-node-updates.mjs";
import { checkTapTotals } from "./require-tap-totals.mjs";

const recordPath = join(repoRoot, "docs", "third-party", "codeburn.md");

/** The machine-readable scan record: the JSON block between the two markers in the document. */
function readRecord() {
  const text = readFileSync(recordPath, "utf8");
  const match = /<!-- scan-record:begin -->\s*```json\s*([\s\S]*?)```\s*<!-- scan-record:end -->/.exec(text);
  assert.ok(match, "docs/third-party/codeburn.md has a scan-record block");
  return JSON.parse(match[1]);
}

const record = readRecord();
const pin = readPin();
const runtime = readNodeRuntime();
const relative = (path) => join(repoRoot, path);

test("the pin, the runtime lockfile and package.json agree on codeburn", () => {
  const manifest = JSON.parse(readFileSync(join(sidecarDir, "package.json"), "utf8"));
  assert.equal(manifest.dependencies.codeburn, pin.version, "package.json pins codeburn exactly, with no range");
  const lock = readLockfile(join(sidecarDir, "package-lock.json"));
  const entry = lockEntry(lock, "codeburn");
  assert.equal(entry.version, pin.version, "the lockfile pins the same version as the pin file");
  assert.equal(entry.integrity, pin.tarball_integrity, "the lockfile's sha512 is the pinned tarball integrity");
  assert.deepEqual(lockfileProblems(lock), [], "every runtime lockfile entry has sha512 integrity from the npm registry");
});

test("the bundler lockfile pins every package by sha512 from the npm registry", () => {
  const bundlerLock = readLockfile(join(sidecarDir, "bundler", "package-lock.json"));
  assert.deepEqual(lockfileProblems(bundlerLock), []);
  assert.equal(JSON.parse(readFileSync(join(sidecarDir, "bundler", "package.json"), "utf8")).devDependencies.esbuild, "0.28.2");
});

test("the scan record names the committed lockfiles, SBOM and Node.js files by SHA-256", () => {
  assert.equal(record.lockfile, "apps/desktop/src-tauri/sidecars/codeburn/package-lock.json");
  assert.equal(sha256OfFile(relative(record.lockfile)), record.lockfile_sha256, "runtime lockfile SHA-256 matches the record");
  assert.equal(sha256OfFile(relative(record.bundler_lockfile)), record.bundler_lockfile_sha256, "bundler lockfile SHA-256 matches the record");
  assert.equal(sha256OfFile(relative(record.sbom)), record.sbom_sha256, "SBOM SHA-256 matches the record");
  assert.equal(sha256OfFile(join(sidecarDir, runtime.shasums)), record.node.shasums_sha256, "SHASUMS256.txt SHA-256 matches the record");
  assert.equal(sha256OfFile(join(sidecarDir, runtime.shasumsSignature)), record.node.shasums_signature_sha256, "the signature file matches the record");
});

test("the Node.js archive pins agree with the committed SHASUMS256.txt and the record", () => {
  const shasums = readFileSync(join(sidecarDir, runtime.shasums), "utf8");
  assert.deepEqual(runtimeProblems(runtime, shasums), [], "each pinned archive hash is the one Node.js publishes");
  assert.equal(record.node.version, runtime.version);
  for (const [platform, entry] of Object.entries(runtime.platforms)) {
    assert.equal(record.node.platforms[platform]?.archive_sha256, entry.archiveSha256, `${platform} archive recorded`);
    assert.equal(record.node.platforms[platform]?.binary_sha256, entry.binarySha256, `${platform} binary recorded`);
  }
});

test("the scan record reports zero HIGH and CRITICAL findings from the official Trivy command", () => {
  assert.equal(record.trivy.version, "0.76.2");
  assert.match(record.trivy.vulnerability_db_updated_at, /^\d{4}-\d{2}-\d{2}T/, "the Trivy DB date is recorded");
  assert.equal(record.trivy.official_command_exit, 0, "trivy fs ... --exit-code 1 exited 0");
  assert.equal(record.trivy_bundler.exit, 0, "the bundler scan with --include-dev-deps exited 0");
  assert.equal(record.trivy_bundler.HIGH, 0);
  assert.equal(record.trivy_bundler.CRITICAL, 0);
  assert.equal(record.trivy.results.HIGH, 0);
  assert.equal(record.trivy.results.CRITICAL, 0);
  assert.equal(record.trivy.results.secrets, 0);
  assert.equal(record.trivy.results.misconfigurations, 0);
});

test("the SBOM lists every package of the runtime lockfile at its locked version", () => {
  const lock = readLockfile(join(sidecarDir, "package-lock.json"));
  const locked = new Set(
    Object.entries(lock.packages)
      .filter(([key]) => key !== "")
      .map(([key, value]) => `${key.replace(/^.*node_modules\//, "")}@${value.version}`),
  );
  const sbom = JSON.parse(readFileSync(join(sidecarDir, "sbom.cdx.json"), "utf8"));
  // CycloneDX keeps the npm scope in `group`, so "@scope/name" is group plus name.
  const listed = new Set(
    sbom.components.map((component) => `${component.group ? `${component.group}/` : ""}${component.name}@${component.version}`),
  );
  const missing = [...locked].filter((id) => !listed.has(id));
  assert.deepEqual(missing, [], "every locked package is in the SBOM");
});

test("a newer Node.js 22 release is reported, and the pinned one is not", () => {
  const index = [
    { version: "v22.23.5", security: true },
    { version: "v22.23.6", security: false },
    { version: "v22.24.0", security: true },
    { version: "v23.0.0", security: false },
    { version: "v22.23.4", security: true },
  ];
  assert.deepEqual(
    newerNode22(index, "22.23.5").map((release) => release.version),
    ["v22.24.0", "v22.23.6"],
    "only newer 22.x releases, newest first",
  );
  assert.deepEqual(newerNode22(index.slice(0, 1), "22.23.5"), []);
});

test("a TAP report with a skip, a failure or a count mismatch is refused", () => {
  const passing = "# tests 4\n# suites 0\n# pass 4\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n";
  assert.deepEqual(checkTapTotals(passing, 4), []);
  assert.ok(checkTapTotals(passing.replace("# skipped 0", "# skipped 1"), 4).length > 0, "a skip is not a pass");
  assert.ok(checkTapTotals(passing.replace("# fail 0", "# fail 1"), 4).length > 0, "a failure is refused");
  assert.ok(checkTapTotals(passing, 5).length > 0, "a count mismatch is refused");
  assert.ok(checkTapTotals("# tests 4\n# pass 4\n", 4).length > 0, "missing totals are refused");
});
