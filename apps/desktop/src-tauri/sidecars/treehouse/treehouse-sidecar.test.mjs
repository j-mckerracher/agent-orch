// Keeps the treehouse scan record (docs/third-party/treehouse.md) true to the committed files: the pin, the upstream
// checksums file and the binary hashes. Run by audit.yml with the codeburn record test. It never runs Trivy or
// downloads anything; it checks what was recorded at scan time.
//
//   node --test --test-reporter=tap apps/desktop/src-tauri/sidecars/treehouse/treehouse-sidecar.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseShasums, repoRoot, sha256OfFile } from "../codeburn/lib.mjs";

const sidecarDir = dirname(fileURLToPath(import.meta.url));
const recordPath = join(repoRoot, "docs", "third-party", "treehouse.md");

/** The machine-readable scan record: the JSON block between the two markers in the document. */
function readRecord() {
  const text = readFileSync(recordPath, "utf8");
  const match = /<!-- scan-record:begin -->\s*```json\s*([\s\S]*?)```\s*<!-- scan-record:end -->/.exec(text);
  assert.ok(match, "docs/third-party/treehouse.md has a scan-record block");
  return JSON.parse(match[1]);
}

const record = readRecord();
const pin = JSON.parse(readFileSync(join(sidecarDir, "treehouse-pin.json"), "utf8"));

test("the pin, the upstream checksums file and the scan record agree on treehouse", () => {
  const checksums = join(sidecarDir, pin.checksums);
  assert.equal(sha256OfFile(checksums), pin.checksumsSha256, "checksums.txt SHA-256 matches the pin");
  assert.equal(record.checksums_sha256, pin.checksumsSha256, "checksums.txt SHA-256 matches the record");
  const listed = parseShasums(readFileSync(checksums, "utf8"));
  assert.equal(record.version, pin.version);
  assert.equal(record.commit, pin.commit);
  for (const [platform, entry] of Object.entries(pin.platforms)) {
    assert.equal(listed.get(entry.archive), entry.archiveSha256, `${platform} archive hash is the one upstream publishes`);
    assert.equal(record.platforms[platform]?.archive_sha256, entry.archiveSha256, `${platform} archive recorded`);
    assert.equal(record.platforms[platform]?.binary_sha256, entry.binarySha256, `${platform} binary recorded`);
  }
  assert.equal(record.trivy.official_command_exit, 0, "trivy fs ... --exit-code 1 exited 0 on the upstream source");
  assert.equal(record.trivy.results.HIGH, 0);
  assert.equal(record.trivy.results.CRITICAL, 0);
  assert.equal(record.trivy.results.secrets, 0);
  assert.equal(record.trivy.results.misconfigurations, 0);
});
