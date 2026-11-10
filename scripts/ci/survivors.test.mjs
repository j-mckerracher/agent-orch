import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { test } from "node:test";
import {
  ancestorsOf, describeSurvivors, findSurvivors, listProcesses, parseCim, parsePs, scanSurvivors,
} from "./survivors.mjs";

const PS = `
    1     0 /sbin/launchd
  100     1 /bin/bash -c node scripts/ci/run-gates.mjs --target-dir /w/target --test-tmp /w/tt
  101   100 node scripts/ci/run-gates.mjs --target-dir /w/target --test-tmp /w/tt
  200     1 /w/target/debug/orchestrator-daemon --socket /w/tt/d.sock
  201     1 /w/target/release/bundle/macos/Agent Orchestrator.app/Contents/MacOS/agent-orch-desktop
  300     1 /usr/bin/other --path /elsewhere/target
`;

test("parsePs reads pid, ppid and the whole command line, spaces included", () => {
  const procs = parsePs(PS);
  assert.equal(procs.length, 6);
  assert.deepEqual(procs[3], { pid: 200, ppid: 1, name: "/w/target/debug/orchestrator-daemon", command: "/w/target/debug/orchestrator-daemon --socket /w/tt/d.sock" });
  assert.match(procs[4].command, /Agent Orchestrator\.app/);
});

test("survivors are processes under this run's directories; the harness and its ancestors are not", () => {
  const procs = parsePs(PS);
  const found = findSurvivors(procs, ["/w/target", "/w/tt"], 101);
  assert.deepEqual(found.map((p) => p.pid), [200, 201]);
  assert.deepEqual([...ancestorsOf(procs, 101)].sort((a, b) => a - b), [1, 100, 101]);
  assert.deepEqual(findSurvivors(procs, ["/nowhere"], 101), []);
  assert.match(describeSurvivors(found), /pid 200 ppid 1 .*orchestrator-daemon/);
});

test("Windows listing: path separators and case do not matter, a single object is accepted", () => {
  const cim = JSON.stringify([
    { ProcessId: 4, ParentProcessId: 0, Name: "System", ExecutablePath: null, CommandLine: null },
    { ProcessId: 9, ParentProcessId: 4, Name: "daemon.exe", ExecutablePath: "D:\\a\\_temp\\target\\debug\\daemon.exe", CommandLine: "\"D:\\a\\_temp\\target\\debug\\daemon.exe\"" },
    { ProcessId: 10, ParentProcessId: 4, Name: "x.exe", ExecutablePath: null, CommandLine: "x --out d:/A/_TEMP/tt/file" },
  ]);
  const found = findSurvivors(parseCim(cim), ["D:/a/_temp/target", "D:\\a\\_temp\\tt"], 1);
  assert.deepEqual(found.map((p) => p.pid), [9, 10]);
  assert.equal(parseCim(JSON.stringify({ ProcessId: 5, ParentProcessId: 1, Name: "n" })).length, 1);
  assert.deepEqual(parseCim(""), []);
});

test("scanSurvivors retries while a process is still listed, then reports what remains", async () => {
  const procs = parsePs(PS);
  let calls = 0;
  const gone = await scanSurvivors(["/w/target"], { list: () => (++calls < 3 ? procs : parsePs("1 0 /sbin/launchd")), attempts: 5, delayMs: 1, ownPid: 101 });
  assert.deepEqual([gone.length, calls], [0, 3]);
  const stuck = await scanSurvivors(["/w/target"], { list: () => procs, attempts: 3, delayMs: 1, ownPid: 101 });
  assert.equal(stuck.length, 2);
});

test("a listing failure is an error, not an empty result", () => {
  assert.throws(() => listProcesses("darwin", () => ({ status: 1, stderr: "boom" })), /process listing failed: boom/);
});

test("the real listing finds a live process by a path unique to it, and not this harness", async () => {
  const marker = `survivor-probe-${process.pid}-${Date.now()}`;
  const child = spawn(process.execPath, ["-e", "setTimeout(() => {}, 30000)", marker], { stdio: "ignore" });
  try {
    await new Promise((r) => setTimeout(r, 300));
    const found = findSurvivors(listProcesses(), [marker]);
    assert.deepEqual(found.map((p) => p.pid), [child.pid]);
  } finally {
    child.kill("SIGKILL");
  }
});
