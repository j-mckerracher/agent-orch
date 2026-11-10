import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import os from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { runBounded } from "./bounded.mjs";
import { removeDir } from "./remove-dir.mjs";

const node = process.execPath;
// Windows keeps a directory busy for a moment after a process that had it as its working directory
// exited or was killed. `removeDir` retries that for up to 10 s; a directory still busy after that
// fails the test, but never hides the error the test body already threw.
const withDir = async (body) => {
  const dir = mkdtempSync(join(os.tmpdir(), "bounded-"));
  let failed = false;
  try {
    return await body(dir);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    try {
      await removeDir(dir, { attempts: 50, delayMs: 200 });
    } catch (error) {
      if (!failed) throw error;
    }
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
const waitUntilGone = async (pid, limitMs = 10000) => {
  const deadline = Date.now() + limitMs;
  while (alive(pid) && Date.now() < deadline) await new Promise((r) => setTimeout(r, 50));
  assert.ok(!alive(pid), `process ${pid} is still running ${limitMs} ms after it was killed`);
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
      const g = spawn(process.execPath, ["-e", "setTimeout(() => {}, 60000)"], { cwd: require("node:os").tmpdir(), detached: true, stdio: ["ignore", "inherit", "inherit"] });
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
      // The grandchild is started outside `dir`, but is still killed and awaited so no process of
      // this test outlives it, and so the pipes it inherited are closed before `dir` is removed.
      if (pid > 0) {
        try {
          process.kill(pid, "SIGKILL");
        } catch {
          // already gone
        }
        await waitUntilGone(pid);
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
    assert.ok(r.pid > 0);
    await waitUntilGone(r.pid);
  }));

test("a command that cannot start reports the error", () =>
  withDir(async (dir) => {
    const r = await runBounded({
      exe: join(dir, "no-such-binary"), commandArgs: [], cwd: dir, env: process.env, echo: false, timeoutMs: 5000,
      stdoutFile: join(dir, "o.txt"), stderrFile: join(dir, "e.txt"),
    });
    assert.match(r.spawnError, /ENOENT/);
  }));
