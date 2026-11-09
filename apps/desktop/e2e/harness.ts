/**
 * Node e2e harness: spawns the REAL M5-02 daemon and implements the host side
 * of `DaemonConnector` (read token, framed handshake) over a Unix socket.
 *
 * Unix only. The Windows named pipe (M5-01b) and the Tauri host are NOT
 * exercised here; they need native Windows proof.
 */
import { spawn, execFileSync, type ChildProcess } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import net from "node:net";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { DaemonError } from "../src/daemon/errors";
import type { DaemonConnector, OpenedChannel, RpcChannel } from "../src/daemon/session";
import type { Capability, Welcome } from "../src/daemon/types";

const VERSION = 1;

export function daemonBinary(): string {
  const path = process.env.ORCH_DAEMON_BIN;
  if (!path) throw new Error("set ORCH_DAEMON_BIN to the orchestrator-daemon binary");
  return path;
}

/** Unix socket paths are limited to ~104 bytes, so work dirs live under a short home path. */
export function workRoot(): string {
  const root = process.env.E2E_WORK_DIR ?? join(homedir(), ".m5-03-e2e");
  mkdirSync(root, { recursive: true });
  return root;
}

function frame(message: unknown): Buffer {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.alloc(6);
  header.writeUInt16BE(VERSION, 0);
  header.writeUInt32BE(body.length, 2);
  return Buffer.concat([header, body]);
}

export class SocketChannel implements RpcChannel {
  private buffer = Buffer.alloc(0);
  private messageListeners: Array<(message: unknown) => void> = [];
  private closeListeners: Array<(reason: DaemonError | null) => void> = [];
  private closed = false;

  constructor(private readonly socket: net.Socket) {
    socket.on("data", (chunk: Buffer) => this.onData(chunk));
    socket.on("close", () => this.finish(null));
    socket.on("error", () => this.finish(new DaemonError("disconnected", "socket error")));
  }

  private onData(chunk: Buffer): void {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    while (this.buffer.length >= 6) {
      const length = this.buffer.readUInt32BE(2);
      if (this.buffer.length < 6 + length) return;
      const body = this.buffer.subarray(6, 6 + length);
      this.buffer = this.buffer.subarray(6 + length);
      let message: unknown;
      try {
        message = JSON.parse(body.toString("utf8"));
      } catch {
        this.socket.destroy();
        return;
      }
      for (const listener of this.messageListeners) listener(message);
    }
  }

  private finish(reason: DaemonError | null): void {
    if (this.closed) return;
    this.closed = true;
    for (const listener of this.closeListeners) listener(reason);
  }

  send(message: unknown): void {
    if (this.closed) throw new DaemonError("disconnected", "channel closed");
    this.socket.write(frame(message));
  }

  onMessage(listener: (message: unknown) => void): void {
    this.messageListeners.push(listener);
  }

  onClose(listener: (reason: DaemonError | null) => void): void {
    this.closeListeners.push(listener);
  }

  close(): void {
    this.socket.destroy();
    this.finish(null);
  }

  /** Test hook: drop the connection as a crash would. */
  sever(): void {
    this.socket.destroy();
  }
}

export class NodeConnector implements DaemonConnector {
  readonly opened: SocketChannel[] = [];
  /** Test hook: overrides the token sent, to prove auth failure handling. */
  tokenOverride: string | null = null;
  /** Test hook: overrides the versions offered, to prove incompatible handling. */
  versions: number[] = [VERSION];

  constructor(private readonly dataDir: string) {}

  async open(requested: readonly Capability[]): Promise<OpenedChannel> {
    const token = this.tokenOverride ?? readFileSync(join(this.dataDir, "daemon.token"), "utf8").trim();
    const socket = await new Promise<net.Socket>((resolve, reject) => {
      const s = net.createConnection(join(this.dataDir, "ipc", "daemon.sock"));
      s.once("connect", () => resolve(s));
      s.once("error", () => reject(new DaemonError("disconnected", "cannot reach the daemon socket")));
    });
    const channel = new SocketChannel(socket);
    this.opened.push(channel);
    const welcome = await new Promise<Welcome>((resolve, reject) => {
      channel.onMessage((message) => {
        const m = message as { result?: Welcome; error?: { code: number; message: string } };
        if (m.error) reject(DaemonError.fromRpc(m.error.code, m.error.message));
        else if (m.result) resolve(m.result);
      });
      channel.onClose(() => reject(new DaemonError("disconnected", "closed during handshake")));
      channel.send({
        jsonrpc: "2.0",
        id: 0,
        method: "handshake",
        params: { token, versions: this.versions, capabilities: requested },
      });
    }).catch((error) => {
      channel.close();
      throw error;
    });
    // The handshake listener stays attached; the session ignores id 0 responses.
    return { channel, welcome };
  }
}

export interface SeededReview {
  project_id: string;
  task_id: string;
  workspace_id: string;
  verification_run_id: string;
  base_commit: string;
  candidate_commit: string;
  worktree: string;
}

export interface RunningDaemon {
  dataDir: string;
  repo: string;
  process: ChildProcess;
  stderr: () => string;
  stop(): Promise<void>;
  workDir: string;
  /** Stops the process but keeps the data directory. */
  halt(): Promise<void>;
  /** Starts a new process over the same data directory; `resetStore` first deletes the store files. */
  restart(options?: { resetStore?: boolean }): Promise<void>;
  /** Seeds a reviewable task (see the `seed_review` example) while the daemon is stopped, then restarts. */
  seedReview(taskId?: string): Promise<SeededReview>;
}

/** Defaults to the `seed_review` example next to the daemon binary (`cargo build --examples`). */
export function seedBinary(): string {
  return process.env.ORCH_SEED_BIN ?? join(dirname(daemonBinary()), "examples", "seed_review");
}

function git(cwd: string, ...args: string[]): void {
  execFileSync(
    "git",
    ["-c", "user.name=e2e", "-c", "user.email=e2e@example.invalid", "-c", "commit.gpgsign=false", ...args],
    { cwd, stdio: "pipe" },
  );
}

export function initRepo(path: string): void {
  mkdirSync(path, { recursive: true });
  git(path, "init", "-q", "-b", "main");
  writeFileSync(join(path, "README.md"), "e2e fixture repository\n");
  git(path, "add", "README.md");
  git(path, "commit", "-q", "-m", "init");
}

function probe(dataDir: string): Promise<boolean> {
  return new Promise((resolve) => {
    const s = net.createConnection(join(dataDir, "ipc", "daemon.sock"));
    s.once("connect", () => {
      s.destroy();
      resolve(true);
    });
    s.once("error", () => resolve(false));
  });
}

async function launch(dataDir: string): Promise<{ child: ChildProcess; stderr: () => string }> {
  const child = spawn(daemonBinary(), ["--data-dir", dataDir], { stdio: ["ignore", "pipe", "pipe"] });
  let stderr = "";
  child.stderr?.on("data", (chunk) => (stderr += String(chunk)));
  const deadline = Date.now() + 15_000;
  while (!(await probe(dataDir))) {
    if (child.exitCode !== null) throw new Error(`daemon exited early: ${stderr}`);
    if (Date.now() > deadline) throw new Error(`daemon did not become ready: ${stderr}`);
    await new Promise((r) => setTimeout(r, 50));
  }
  return { child, stderr: () => stderr };
}

async function terminate(child: ChildProcess): Promise<void> {
  if (child.exitCode === null) {
    child.kill("SIGTERM");
    await new Promise((r) => child.once("exit", r));
  }
}

export async function startDaemon(): Promise<RunningDaemon> {
  const workDir = mkdtempSync(join(workRoot(), "r-"));
  const dataDir = join(workDir, "d");
  const repo = join(workDir, "repo");
  mkdirSync(dataDir, { recursive: true });
  initRepo(repo);
  let running = await launch(dataDir);
  const daemon: RunningDaemon = {
    dataDir,
    repo,
    workDir,
    get process() {
      return running.child;
    },
    stderr: () => running.stderr(),
    async halt() {
      await terminate(running.child);
    },
    async restart(options = {}) {
      await terminate(running.child);
      if (options.resetStore) {
        for (const suffix of ["", "-wal", "-shm"]) rmSync(join(dataDir, `store.sqlite${suffix}`), { force: true });
      }
      running = await launch(dataDir);
    },
    async seedReview(taskId = "e2e-task") {
      await terminate(running.child);
      const out = execFileSync(
        seedBinary(),
        [dataDir, realpathSync(repo), join(workDir, "worktrees"), taskId],
        { encoding: "utf8" },
      );
      running = await launch(dataDir);
      return JSON.parse(out) as SeededReview;
    },
    async stop() {
      await terminate(running.child);
      rmSync(workDir, { recursive: true, force: true });
    },
  };
  return daemon;
}
