// Bounded execution of one gate command, with raw evidence files.
import { spawn, spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";

const win = process.platform === "win32";

export const killTree = (child) => {
  try {
    if (win) spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    else process.kill(-child.pid, "SIGKILL");
  } catch {
    // already gone
  }
};

// One command, one process: its exit status is the status of that process, never of a pipeline.
//
// Resolves when the process has exited and its output is drained, but never waits on a descendant
// that outlives it: a detached grandchild can hold the stdout/stderr pipes open, so `close` may
// never come. The pipes are abandoned `exitGraceMs` after `exit` (`pipesAbandoned: true`; the
// survivor scan reports the descendant). After a timeout kill the run also ends if neither event
// arrives within `killWaitMs`, with `signal: "SIGKILL"`.
export const runBounded = ({
  exe, commandArgs, cwd, env, timeoutMs, stdoutFile, stderrFile, echo = true, exitGraceMs = 5000, killWaitMs = 10000,
}) =>
  new Promise((done) => {
    const started = Date.now();
    const out = createWriteStream(stdoutFile);
    const err = createWriteStream(stderrFile);
    const chunks = { out: [], err: [] };
    const child = spawn(exe, commandArgs, {
      cwd, env, windowsHide: true, detached: !win, shell: win && /^np[mx]$/.test(exe),
    });
    let timedOut = false;
    let spawnError = null;
    let settled = false;
    let graceTimer = null;
    let killTimer = null;
    const timer = setTimeout(() => {
      timedOut = true;
      killTree(child);
      killTimer = setTimeout(() => settle(null, "SIGKILL", true), killWaitMs);
    }, timeoutMs);
    const settle = (code, signal, pipesAbandoned) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearTimeout(graceTimer);
      clearTimeout(killTimer);
      if (pipesAbandoned) {
        child.stdout?.destroy();
        child.stderr?.destroy();
      }
      out.end();
      err.end();
      Promise.all([new Promise((r) => out.on("finish", r)), new Promise((r) => err.on("finish", r))]).then(() =>
        done({
          code, signal, timedOut, pipesAbandoned: Boolean(pipesAbandoned), spawnError: spawnError ? String(spawnError) : null,
          pid: child.pid ?? null,
          durationMs: Date.now() - started,
          stdout: Buffer.concat(chunks.out).toString("utf8"),
          stderr: Buffer.concat(chunks.err).toString("utf8"),
        }));
    };
    child.stdout.on("data", (d) => { chunks.out.push(d); out.write(d); if (echo) process.stdout.write(d); });
    child.stderr.on("data", (d) => { chunks.err.push(d); err.write(d); if (echo) process.stderr.write(d); });
    child.on("error", (e) => {
      spawnError = e;
      if (!child.pid) settle(null, null, false);
    });
    child.on("exit", (code, signal) => {
      graceTimer = setTimeout(() => settle(code, signal, true), exitGraceMs);
    });
    child.on("close", (code, signal) => settle(code, signal, false));
  });
