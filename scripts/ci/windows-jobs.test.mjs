import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { gatesFor } from "./gates.mjs";
import { GROUPS, JOBS, selectGroups, windowsVerdict } from "./windows-jobs.mjs";

const ALL = { engine: true, tauri: true, desktop: true, package: true };
const NONE = { engine: false, tauri: false, desktop: false, package: false };
const only = (...groups) => Object.fromEntries(Object.keys(NONE).map((g) => [g, groups.includes(g)]));
const pick = (result) => ({ engine: result.engine, tauri: result.tauri, desktop: result.desktop, package: result.package });
const select = (changedFiles) => pick(selectGroups({ eventName: "pull_request", changedFiles }));

test("an event other than a pull request runs every Windows gate", () => {
  for (const eventName of ["push", "workflow_dispatch", "workflow_call", "schedule", undefined]) {
    assert.deepEqual(pick(selectGroups({ eventName, changedFiles: [] })), ALL, String(eventName));
  }
});

test("a pull request that changes only documentation runs no group", () => {
  for (const files of [[], ["docs/a.md"], ["README.md"], ["docs/a.md", "README.md", "docs/evidence/x.jsonl"]]) {
    assert.deepEqual(select(files), NONE, files.join(","));
  }
});

test("each area runs the groups that consume it", () => {
  const table = [
    [["apps/desktop/src/ui/shell/Panel.tsx"], only("desktop", "package")],
    [["apps/desktop/src/ui/shell/Panel.tsx", "apps/desktop/e2e/x.ts"], only("desktop", "package")],
    [["apps/desktop/src-tauri/src/main.rs"], only("tauri", "package")],
    [["crates/orchestrator-git/src/lib.rs"], only("engine", "tauri", "package")],
    [["docs/a.md", "crates/orchestrator-git/src/lib.rs"], only("engine", "tauri", "package")],
    [["apps/desktop/src/x.ts", "apps/desktop/src-tauri/src/y.rs"], only("desktop", "tauri", "package")],
  ];
  for (const [files, expected] of table) assert.deepEqual(select(files), expected, files.join(","));
});

test("a harness, manifest, lockfile, toolchain or Node tooling change runs every gate", () => {
  const files = [
    "scripts/ci/gates.mjs",
    "scripts/release/package.mjs",
    ".github/workflows/ci.yml",
    "Cargo.toml",
    "Cargo.lock",
    "crates/orchestrator-git/Cargo.toml",
    "apps/desktop/src-tauri/Cargo.toml",
    "apps/desktop/src-tauri/Cargo.lock",
    "apps/desktop/package.json",
    "apps/desktop/package-lock.json",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    "deny.toml",
  ];
  for (const file of files) assert.deepEqual(select([file]), ALL, file);
});

test("an input the table does not know runs every gate, even beside documentation", () => {
  for (const files of [["board.json"], ["tests/fixtures/x.json"], ["docs/a.md", "utilities/tool.py"]]) {
    assert.deepEqual(select(files), ALL, files.join(","));
  }
});

test("an unreadable change set runs every gate", () => {
  assert.deepEqual(pick(selectGroups({ eventName: "pull_request", changedFiles: null })), ALL);
});

// Every Windows gate must run in some job, no job may name a gate that is not a Windows gate, and a gate's
// prerequisite (`needs` in gates.mjs) must run in the same job as the gate.
test("the jobs cover every Windows gate once, each with its prerequisites", () => {
  const windowsGates = gatesFor("win32");
  const names = windowsGates.map((g) => g.name);
  const placed = JOBS.flatMap((job) => job.gates);
  for (const name of names) assert.ok(placed.includes(name), `${name} runs in no job`);
  for (const name of placed) assert.ok(names.includes(name), `${name} is not a Windows gate`);
  assert.equal(new Set(JOBS.map((j) => j.id)).size, JOBS.length, "job ids are not unique");
  for (const job of JOBS) {
    assert.equal(new Set(job.gates).size, job.gates.length, `${job.id} lists a gate twice`);
    assert.ok(job.group === "harness" || GROUPS.includes(job.group), `${job.id} has an unknown group`);
    for (const gate of windowsGates.filter((g) => job.gates.includes(g.name))) {
      for (const need of gate.needs ?? []) {
        if (names.includes(need)) assert.ok(job.gates.includes(need), `${job.id}: ${gate.name} needs ${need}`);
      }
    }
  }
});

test("the workflow has a job for each table entry and an aggregator that waits for all of them", () => {
  const yml = readFileSync(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");
  for (const job of JOBS) {
    assert.match(yml, new RegExp(`^  ${job.id}:$`, "m"), `ci.yml has no job ${job.id}`);
    if (job.group !== "harness") {
      assert.match(yml, new RegExp(`if: needs\\.changes\\.outputs\\.${job.group} == 'true'`), `no if for ${job.group}`);
    }
  }
  for (const [, id] of yml.matchAll(/windows-jobs\.mjs gates ([\w-]+)/g)) {
    assert.ok(JOBS.some((j) => j.id === id), `ci.yml asks for unknown job ${id}`);
  }
  for (const group of GROUPS) {
    assert.match(yml, new RegExp(`${group}: \\$\\{\\{ steps\\.select\\.outputs\\.${group} \\}\\}`), `changes lacks ${group}`);
  }
  const waits = /^  gates-windows:[\s\S]*?^    needs: \[([^\]]*)\]/m.exec(yml);
  assert.ok(waits, "ci.yml has no gates-windows aggregator with a needs list");
  const needed = waits[1].split(",").map((s) => s.trim());
  for (const job of [...JOBS.map((j) => j.id), "changes"]) assert.ok(needed.includes(job), `aggregator skips ${job}`);
});

const needsFor = ({ selected = {}, results = {}, changes = "success" }) => {
  const outputs = Object.fromEntries(GROUPS.map((g) => [g, selected[g] ? "true" : "false"]));
  const needs = { changes: { result: changes, outputs } };
  for (const job of JOBS) {
    const isSelected = job.group === "harness" || selected[job.group] === true;
    needs[job.id] = { result: results[job.id] ?? (isSelected ? "success" : "skipped") };
  }
  return needs;
};

test("the verdict passes when every selected job succeeded and every other job was skipped", () => {
  assert.equal(windowsVerdict(needsFor({ selected: ALL })).ok, true);
  assert.equal(windowsVerdict(needsFor({ selected: {} })).ok, true, "docs-only: only the harness ran");
  assert.equal(windowsVerdict(needsFor({ selected: { desktop: true, package: true } })).ok, true);
});

test("a selected job that fails, is cancelled or is skipped fails the verdict", () => {
  for (const result of ["failure", "cancelled", "skipped"]) {
    const verdict = windowsVerdict(needsFor({ selected: { engine: true }, results: { "engine-build": result } }));
    assert.equal(verdict.ok, false, result);
    assert.equal(verdict.rows.find((r) => r.job === "engine-build").pass, false, result);
  }
});

test("an unselected job that ran and succeeded is accepted; one that failed is not", () => {
  assert.equal(windowsVerdict(needsFor({ results: { "tauri-tests": "success" } })).ok, true);
  assert.equal(windowsVerdict(needsFor({ results: { "tauri-tests": "failure" } })).ok, false);
});

test("the harness always runs, so a failed harness fails the verdict even with nothing selected", () => {
  assert.equal(windowsVerdict(needsFor({ selected: {}, results: { harness: "failure" } })).ok, false);
  assert.equal(windowsVerdict(needsFor({ selected: {}, results: { harness: "skipped" } })).ok, false);
});

test("a failed change selection or a missing job fails the verdict", () => {
  assert.equal(windowsVerdict(needsFor({ selected: ALL, changes: "failure" })).ok, false);
  const needs = needsFor({ selected: ALL });
  delete needs["desktop-package"];
  assert.equal(windowsVerdict(needs).ok, false);
});
