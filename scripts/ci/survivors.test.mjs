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

// The Priority 12 round 3 Windows survivor: the operator's own monitor shell named the run directory.
const MONITOR_COMMAND = String.raw`"C:\Program Files (x86)\PowerShell\7\pwsh.exe" -NoProfile -Command "$ev='C:\Windows\Temp\p12r3-bfe455a7\51b5f79f\evidence'; Get-ChildItem -LiteralPath $ev -File"`;
const WINDOWS_TABLE = JSON.stringify([
  { ProcessId: 4, ParentProcessId: 0, Name: "System", ExecutablePath: null, CommandLine: null },
  { ProcessId: 30392, ParentProcessId: 4, Name: "pwsh.exe", ExecutablePath: null, CommandLine: "pwsh.exe" },
  { ProcessId: 50416, ParentProcessId: 30392, Name: "pwsh.exe", ExecutablePath: null, CommandLine: MONITOR_COMMAND },
  { ProcessId: 700, ParentProcessId: 30392, Name: "node.exe", ExecutablePath: null, CommandLine: "node run-gates.mjs --target-dir C:\\Windows\\Temp\\p12r3-bfe455a7" },
  { ProcessId: 701, ParentProcessId: 700, Name: "cargo.exe", ExecutablePath: null, CommandLine: "cargo test C:\\Windows\\Temp\\p12r3-bfe455a7\\target" },
  { ProcessId: 702, ParentProcessId: 701, Name: "daemon.exe", ExecutablePath: null, CommandLine: "C:\\Windows\\Temp\\p12r3-bfe455a7\\target\\debug\\daemon.exe" },
  { ProcessId: 703, ParentProcessId: 99999, Name: "daemon.exe", ExecutablePath: null, CommandLine: "C:\\Windows\\Temp\\p12r3-bfe455a7\\target\\debug\\daemon.exe --detached" },
]);

test("with a launched child, the operator's monitor that names the run directory is not a survivor", () => {
  const procs = parseCim(WINDOWS_TABLE);
  const roots = ["C:\\Windows\\Temp\\p12r3-bfe455a7"];
  assert.deepEqual(findSurvivors(procs, roots, 700).map((p) => p.pid).sort((a, b) => a - b), [701, 702, 703, 50416]);
  assert.deepEqual(findSurvivors(procs, roots, 700, { launched: [701] }).map((p) => p.pid), [701, 702, 703]);
  assert.deepEqual(findSurvivors(procs, roots, 700, { launched: [999] }).map((p) => p.pid), [703]);
});

test("a descendant of the launched child and an orphan of it are survivors; a live unrelated parent is not", () => {
  const procs = parsePs(`
    1     0 /sbin/launchd
  100     1 /bin/zsh
  101   100 node /w/run-gates.mjs --target-dir /w/target
  102   100 sh -c watch /w/target/evidence
  200   101 cargo test /w/target
  201   200 /w/target/debug/daemon
  300     1 /w/target/debug/daemon --orphan
  400   102 tail -f /w/target/evidence/log
`);
  assert.deepEqual(findSurvivors(procs, ["/w/target"], 101, { launched: [200] }).map((p) => p.pid), [200, 201, 300]);
  assert.deepEqual(findSurvivors(procs, ["/w/target"], 101, { launched: [] }).map((p) => p.pid), [300]);
});

test("scanSurvivors passes the launched pids to the scan", async () => {
  const procs = parsePs("1 0 /sbin/launchd\n50 1 /bin/zsh\n100 50 sh -c watch /w/target\n200 1 /w/target/bin\n201 200 /w/target/bin2");
  const found = await scanSurvivors(["/w/target"], { list: () => procs, attempts: 1, ownPid: 5, launched: [200] });
  assert.deepEqual(found.map((p) => p.pid), [200, 201]);
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
