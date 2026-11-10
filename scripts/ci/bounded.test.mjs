import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import os from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { runBounded } from "./bounded.mjs";

const node = process.execPath;
const withDir = async (body) => {
  const dir = mkdtempSync(join(os.tmpdir(), "bounded-"));
  try {
    return await body(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};
const run = (dir, script, extra = {}) =>
  runBounded({
    exe: node, commandArgs: ["-e", script], cwd: dir, env: process.env, echo: false, timeoutMs: 20000,
    stdoutFile: join(dir, "o.txt"), stderrFile: join(dir, "e.txt"), ...extra,
  });
const alive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

test("captures stdout, stderr and the exit code, and also the file bytes", () =>
  withDir(async (dir) => {
    const r = await run(dir, "console.log('out'); console.error('err'); process.exit(3)");
    assert.deepEqual([r.code, r.timedOut, r.pipesAbandoned], [3, false, false]);
    assert.equal(r.stdout.trim(), "out");
    assert.equal(readFileSync(join(dir, "e.txt"), "utf8").trim(), "err");
  }));

test("a descendant holding the output pipes cannot stall the run after the process exits", () =>
  withDir(async (dir) => {
    const script = `
      const { spawn } = require("node:child_process");
      const g = spawn(process.execPath, ["-e", "setTimeout(() => {}, 60000)"], { detached: true, stdio: ["ignore", "inherit", "inherit"] });
      console.log("grandchild " + g.pid);
      g.unref();`;
    const started = Date.now();
    const r = await run(dir, script, { exitGraceMs: 300 });
    const pid = Number(/grandchild (\d+)/.exec(r.stdout)?.[1]);
    try {
      assert.equal(r.code, 0);
      assert.equal(r.pipesAbandoned, true);
      assert.ok(Date.now() - started < 15000, "returned long before the grandchild's 60 s");
      assert.ok(pid > 0 && alive(pid), "the descendant is still running, for the survivor scan to find");
    } finally {
      if (pid > 0) {
        try {
          process.kill(pid, "SIGKILL");
        } catch {
          // already gone
        }
      }
    }
  }));

test("a timeout kills the process and ends the run promptly", () =>
  withDir(async (dir) => {
    const started = Date.now();
    const r = await run(dir, "setInterval(() => {}, 1000)", { timeoutMs: 500 });
    assert.equal(r.timedOut, true);
    assert.notEqual(r.code, 0);
    assert.ok(Date.now() - started < 15000);
  }));

test("a command that cannot start reports the error", () =>
  withDir(async (dir) => {
    const r = await runBounded({
      exe: join(dir, "no-such-binary"), commandArgs: [], cwd: dir, env: process.env, echo: false, timeoutMs: 5000,
      stdoutFile: join(dir, "o.txt"), stderrFile: join(dir, "e.txt"),
    });
    assert.match(r.spawnError, /ENOENT/);
  }));
