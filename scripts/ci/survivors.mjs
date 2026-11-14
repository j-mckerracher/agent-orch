// Survivor scan: after a gate, no process of this run may still be running.
//
// Mirrors the per-gate check of the Priority 11r3 Windows runbook (Find-OwnedSurvivors): a survivor
// is a process whose image or command line lies under a path unique to this run (the target and
// test-temp directories). The listing is read-only; nothing is stopped. The harness itself and its
// ancestors, whose command lines name those directories, are never survivors. When the caller passes
// the pid it launched, a process also has to descend from it or be an orphan: an operator's monitor
// shell that names the run directory (Priority 12 round 3) is not the gate's process.
import { spawnSync } from "node:child_process";

const norm = (s) => s.replaceAll("\\", "/").toLowerCase();

// macOS: `ps -axww -o pid=,ppid=,command=`.
export function parsePs(text) {
  const processes = [];
  for (const line of text.split(/\r?\n/)) {
    const m = /^\s*(\d+)\s+(\d+)\s+(.*\S)\s*$/.exec(line);
    if (m) processes.push({ pid: Number(m[1]), ppid: Number(m[2]), name: m[3].split(/\s/)[0], command: m[3] });
  }
  return processes;
}

// Windows: Get-CimInstance Win32_Process | ConvertTo-Json (one object, not an array, for one process).
export function parseCim(json) {
  const parsed = JSON.parse(json || "[]");
  return (Array.isArray(parsed) ? parsed : [parsed]).map((p) => ({
    pid: p.ProcessId, ppid: p.ParentProcessId, name: p.Name ?? "",
    command: [p.ExecutablePath, p.CommandLine].filter(Boolean).join(" "),
  }));
}

export function listProcesses(platform = process.platform, run = spawnSync) {
  const r = platform === "win32"
    ? run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
      "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,ExecutablePath,CommandLine | ConvertTo-Json -Compress"],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 })
    : run("ps", ["-axww", "-o", "pid=,ppid=,command="], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (r.error || r.status !== 0) throw new Error(`process listing failed: ${r.error ?? (r.stderr || `exit ${r.status}`)}`);
  return platform === "win32" ? parseCim(r.stdout) : parsePs(r.stdout);
}

export const ancestorsOf = (processes, pid) => {
  const byPid = new Map(processes.map((p) => [p.pid, p]));
  const chain = new Set([pid]);
  for (let p = byPid.get(pid); p && !chain.has(p.ppid) && p.ppid > 0; p = byPid.get(p.ppid)) chain.add(p.ppid);
  return chain;
};

// A process is part of the gate when it is the launched child, one of its descendants, or an orphan:
// its parent is gone from the table (Windows keeps the dead parent's pid; macOS reparents to 1), so
// a detached grandchild of the gate is recognised after the child exited. A process whose parent is
// alive and not part of the gate, such as an operator's monitor shell that happens to name the run
// directory on its command line, is not.
const belongsToGate = (processes, launched) => {
  const byPid = new Map(processes.map((p) => [p.pid, p]));
  const roots = new Set(launched);
  return (p) => {
    if (!byPid.has(p.ppid) || p.ppid <= 1) return true;
    const seen = new Set();
    for (let q = p; q && !seen.has(q.pid); q = byPid.get(q.ppid)) {
      if (roots.has(q.pid)) return true;
      seen.add(q.pid);
    }
    return false;
  };
};

// Without `launched` every process naming a root counts (the original rule). With it, a process
// naming a root must also belong to the gate (see `belongsToGate`).
export function findSurvivors(processes, roots, ownPid = process.pid, { launched } = {}) {
  const ignore = ancestorsOf(processes, ownPid);
  const needles = roots.filter(Boolean).map(norm);
  const named = processes.filter((p) => !ignore.has(p.pid) && needles.some((n) => norm(p.command).includes(n)));
  if (!launched) return named;
  const inGate = belongsToGate(processes, launched.filter(Boolean));
  return named.filter(inGate);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// A process that is exiting can still be listed for a moment, so the scan retries before judging.
export async function scanSurvivors(roots, { list = listProcesses, attempts = 5, delayMs = 1000, ownPid = process.pid, launched } = {}) {
  let survivors = [];
  for (let i = 0; i < attempts; i++) {
    survivors = findSurvivors(list(), roots, ownPid, { launched });
    if (survivors.length === 0) break;
    if (i < attempts - 1) await sleep(delayMs);
  }
  return survivors;
}

export const describeSurvivors = (survivors) =>
  survivors.map((p) => `pid ${p.pid} ppid ${p.ppid} ${p.name}: ${p.command}`).join("\n");
