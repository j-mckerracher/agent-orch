import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, test } from "node:test";
import {
  GateError, checkVersion, provenance, refusalsFor, scanTrivy, scanTruffleHog,
} from "./gate.mjs";

const dir = mkdtempSync(join(tmpdir(), "gate-test-"));
after(() => rmSync(dir, { recursive: true, force: true, maxRetries: 5 }));

let seq = 0;
const reportFile = (body) => {
  const file = join(dir, `report-${seq++}.json`);
  writeFileSync(file, typeof body === "string" ? body : JSON.stringify(body));
  return file;
};
const run = (code, extra = {}) => ({
  code, signal: null, timedOut: false, spawnError: null, pipesAbandoned: false, durationMs: 1, stdout: "", ...extra,
});
const header = { SchemaVersion: 2, ArtifactType: "filesystem", ArtifactName: "." };

const git = (repo, ...args) => {
  const r = spawnSync("git", ["-C", repo, ...args], { encoding: "utf8" });
  assert.equal(r.status, 0, `git ${args[0]} failed`);
  return r.stdout.trim();
};
const commitAll = (repo, message) => {
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", "commit", "-q", "-m", message);
  return git(repo, "rev-parse", "HEAD");
};
const makeRepo = (name, files) => {
  const repo = join(dir, name);
  mkdirSync(repo, { recursive: true });
  git(repo, "init", "-q", "-b", "main");
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(repo, dirname(path)), { recursive: true });
    writeFileSync(join(repo, path), content);
  }
  commitAll(repo, "init");
  return repo;
};

test("trivy: exit 0 with an empty report passes", () => {
  assert.deepEqual(scanTrivy(run(0), reportFile({ ...header, Results: [] })), { status: "PASS", findings: [] });
});

test("trivy: a finding fails and the matched literal never reaches the result", () => {
  const literal = "SYNTHETIC-MATCH-LITERAL";
  const result = scanTrivy(run(1), reportFile({
    ...header,
    Results: [{ Target: "a.env", Secrets: [{ RuleID: "github-pat", Severity: "CRITICAL", StartLine: 3, Match: literal }] }],
  }));
  assert.equal(result.status, "FAIL");
  assert.deepEqual(result.findings, [{ target: "a.env", id: "github-pat", severity: "CRITICAL", line: 3 }]);
  assert.equal(JSON.stringify(result).includes(literal), false);
});

test("trivy: exit code must agree with the evidence", () => {
  assert.throws(() => scanTrivy(run(1), reportFile({ ...header, Results: [] })), GateError);
  assert.throws(() => scanTrivy(run(0), reportFile({
    ...header, Results: [{ Target: "x", Misconfigurations: [{ ID: "DS-0002", Severity: "HIGH" }] }],
  })), GateError);
});

test("trivy: other exit codes, signals, timeouts, and abandoned pipes are errors", () => {
  const evidence = () => reportFile({ ...header, Results: [] });
  for (const r of [run(2), run(null, { signal: "SIGKILL" }), run(null, { timedOut: true }), run(0, { pipesAbandoned: true })]) {
    assert.throws(() => scanTrivy(r, evidence()), GateError);
  }
});

test("trivy: malformed or mismatched evidence is an error", () => {
  assert.throws(() => scanTrivy(run(0), reportFile("{not json")), GateError);
  assert.throws(() => scanTrivy(run(0), reportFile({ ...header, ArtifactName: "/abs/path", Results: [] })), GateError);
});

test("trivy: git-worktree evidence (artifact type repository) is accepted; other types are errors", () => {
  assert.deepEqual(scanTrivy(run(0), reportFile({ ...header, ArtifactType: "repository", Results: [] })), { status: "PASS", findings: [] });
  assert.throws(() => scanTrivy(run(0), reportFile({ ...header, ArtifactType: "image", Results: [] })), GateError);
});

test("trufflehog: a finding fails and raw or redacted values are not kept", () => {
  const line = JSON.stringify({
    DetectorName: "Github",
    Verified: false,
    Raw: "RAW-VALUE-LITERAL",
    RawV2: "RAW-V2-LITERAL",
    Redacted: "ghp_RED-LITERAL",
    SourceMetadata: { Data: { git: { file: "config/seed.env", line: 1, commit: "abc123", email: "dev@example.invalid" } } },
  });
  const result = scanTruffleHog(run(183, { stdout: `${line}\n` }));
  assert.equal(result.status, "FAIL");
  assert.deepEqual(result.findings, [{ detector: "Github", verified: false, file: "config/seed.env", line: 1, commit: "abc123" }]);
  assert.equal(JSON.stringify(result).includes("LITERAL"), false);
});

test("trufflehog: exit code must agree with output; other codes are errors", () => {
  assert.equal(scanTruffleHog(run(0)).status, "PASS");
  assert.throws(() => scanTruffleHog(run(183, { stdout: "" })), GateError);
  assert.throws(() => scanTruffleHog(run(0, { stdout: '{"DetectorName":"x"}\n' })), GateError);
  assert.throws(() => scanTruffleHog(run(1)), GateError);
  assert.throws(() => scanTruffleHog(run(0, { stdout: "not json\n" })), GateError);
});

test("trivy: exit-zero analysis-error diagnostics refuse a clean report", () => {
  for (const stderr of [
    '2026-11-16T05:00:00Z\tERROR\t[dockerfile scanner] Failed to parse file',
    '{"level":"fatal","msg":"synthetic diagnostic"}',
  ]) {
    assert.throws(() => scanTrivy(run(0, { stderr }), reportFile({ ...header, Results: [] })), GateError);
  }
  assert.equal(scanTrivy(run(0, { stderr: '{"level":"info","msg":"error count is zero"}' }),
    reportFile({ ...header, Results: [] })).status, "PASS");
});

test("trufflehog: exit-zero error diagnostics refuse empty findings without leaking the message", () => {
  const stderr = '{"level":"error","msg":"SYNTHETIC-DIAGNOSTIC-LITERAL"}';
  assert.throws(() => scanTruffleHog(run(0, { stderr })), (error) =>
    error instanceof GateError && !error.message.includes("LITERAL"));
  assert.equal(scanTruffleHog(run(0, { stderr: '{"level":"info","msg":"error count is zero"}' })).status, "PASS");
});

test("version: observed output must match the pin exactly", () => {
  assert.equal(checkVersion("trivy", "Version: 0.76.2\nVulnerability DB:"), "0.76.2");
  assert.equal(checkVersion("trufflehog", "trufflehog 3.99.9\n"), "3.99.9");
  assert.throws(() => checkVersion("trivy", "Version: 0.76.1"), GateError);
  assert.throws(() => checkVersion("trufflehog", "trufflehog 3.99.90"), GateError);
  assert.throws(() => checkVersion("trufflehog", "no version here"), GateError);
});

test("provenance: refuses a head that is not HEAD and a dirty working tree", () => {
  const repo = makeRepo("provenance", { "a.txt": "one\n" });
  const first = git(repo, "rev-parse", "HEAD");
  writeFileSync(join(repo, "a.txt"), "two\n");
  const second = commitAll(repo, "second");
  assert.throws(() => provenance(repo, first, first), GateError);
  writeFileSync(join(repo, "untracked.txt"), "x\n");
  assert.throws(() => provenance(repo, first, second), GateError);
  rmSync(join(repo, "untracked.txt"));
  assert.equal(provenance(repo, first, second).mergeBase, first);
});

test("refusals: tracked waiver files and inline ignore markers refuse; a clean repo does not", () => {
  assert.deepEqual(refusalsFor(makeRepo("clean", { "README.md": "ok\n" })), []);
  assert.equal(refusalsFor(makeRepo("ignorefile", { ".trivyignore": "DS-0002\n" })).length, 1);
  assert.equal(refusalsFor(makeRepo("marker", { "x.txt": `# ${"trivy" + ":ignore"}:DS-0002\n` })).length, 1);
});
