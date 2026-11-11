// M7-30 verification helpers: headless Chrome, axe-core, and a host-bridge shim that lets the
// built web app talk to a REAL orchestrator-daemon over its Unix socket (the Tauri host is absent).
import puppeteer from "puppeteer-core";
import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import net from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
export const AXE_SOURCE = readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");
export const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
export const APP = process.env.M730_APP ?? "http://localhost:4173/";
export const TARGET = process.env.CARGO_TARGET_DIR ?? "";

export const launchBrowser = () =>
  puppeteer.launch({ executablePath: CHROME, headless: true, args: ["--no-first-run", "--disable-gpu"] });

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function frame(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.alloc(6);
  header.writeUInt16BE(1, 0);
  header.writeUInt32BE(body.length, 2);
  return Buffer.concat([header, body]);
}

// Minimal framed JSON-RPC client over the daemon socket (handshake + request/response).
export class RawClient {
  constructor(socket) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.listeners = [];
    this.closeListeners = [];
    this.closed = false;
    socket.on("data", (chunk) => {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      while (this.buffer.length >= 6) {
        const length = this.buffer.readUInt32BE(2);
        if (this.buffer.length < 6 + length) return;
        const body = this.buffer.subarray(6, 6 + length);
        this.buffer = this.buffer.subarray(6 + length);
        let message;
        try {
          message = JSON.parse(body.toString("utf8"));
        } catch {
          socket.destroy();
          return;
        }
        for (const l of this.listeners) l(message);
      }
    });
    const fin = (reason) => {
      if (this.closed) return;
      this.closed = true;
      for (const l of this.closeListeners) l(reason);
    };
    socket.on("close", () => fin("remote"));
    socket.on("error", () => fin("io"));
  }
  send(message) {
    this.socket.write(frame(message));
  }
  static async connect(dataDir, requested, tokenOverride) {
    const token = tokenOverride ?? readFileSync(join(dataDir, "daemon.token"), "utf8").trim();
    const socket = await new Promise((resolve, reject) => {
      const s = net.createConnection(join(dataDir, "ipc", "daemon.sock"));
      s.once("connect", () => resolve(s));
      s.once("error", () => reject({ code: "daemon-unavailable", message: "cannot reach the daemon socket" }));
    });
    const client = new RawClient(socket);
    const welcome = await new Promise((resolve, reject) => {
      client.listeners.push((m) => {
        if (m.error) {
          const code = { [-32001]: "auth", [-32002]: "capability-unavailable", [-32003]: "version" }[m.error.code] ?? "protocol";
          reject({ code, message: m.error.message });
        } else if (m.result && m.id === 0) resolve(m.result);
      });
      client.closeListeners.push(() => reject({ code: "closed", message: "closed during handshake" }));
      client.send({ jsonrpc: "2.0", id: 0, method: "handshake", params: { token, versions: [1], capabilities: requested } });
    }).catch((e) => {
      socket.destroy();
      throw e;
    });
    return { client, welcome };
  }
}

// One-shot RPC for seeding data outside the UI.
export async function rpc(dataDir, method, params = {}) {
  const { client } = await RawClient.connect(dataDir, ["health", "projects", "tasks", "sessions", "approvals", "agents", "coordinator", "review", "workstreams"]);
  const result = await new Promise((resolve) => {
    client.listeners.push((m) => {
      if (m.id === 1) resolve(m);
    });
    client.send({ jsonrpc: "2.0", id: 1, method, params });
  });
  client.socket.destroy();
  return result;
}

// ---- daemon lifecycle (Unix) ----
function probe(dataDir) {
  return new Promise((resolve) => {
    const s = net.createConnection(join(dataDir, "ipc", "daemon.sock"));
    s.once("connect", () => {
      s.destroy();
      resolve(true);
    });
    s.once("error", () => resolve(false));
  });
}

async function launch(bin, dataDir) {
  const child = spawn(bin, ["--data-dir", dataDir], { stdio: ["ignore", "pipe", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (c) => (stderr += String(c)));
  const deadline = Date.now() + 15000;
  while (!(await probe(dataDir))) {
    if (child.exitCode !== null) throw new Error(`daemon exited early: ${stderr}`);
    if (Date.now() > deadline) throw new Error("daemon not ready");
    await sleep(50);
  }
  return { child, stderr: () => stderr };
}

async function terminate(child) {
  if (child.exitCode === null) {
    child.kill("SIGTERM");
    await new Promise((r) => child.once("exit", r));
  }
}

function git(cwd, ...args) {
  execFileSync("git", ["-c", "user.name=e2e", "-c", "user.email=e2e@example.invalid", "-c", "commit.gpgsign=false", ...args], { cwd, stdio: "pipe" });
}

export async function startDaemon({ seed = true } = {}) {
  const bin = join(TARGET, "debug", "orchestrator-daemon");
  const root = join(homedir(), ".m7-30-live");
  mkdirSync(root, { recursive: true });
  const workDir = mkdtempSync(join(root, "r-"));
  const dataDir = join(workDir, "d");
  const repo = join(workDir, "repo");
  mkdirSync(dataDir, { recursive: true });
  mkdirSync(repo, { recursive: true });
  git(repo, "init", "-q", "-b", "main");
  writeFileSync(join(repo, "README.md"), "m7-30 live fixture repository\n");
  git(repo, "add", "README.md");
  git(repo, "commit", "-q", "-m", "init");
  let running = await launch(bin, dataDir);
  let seeded = null;
  const d = {
    dataDir,
    workDir,
    get seeded() {
      return seeded;
    },
    stderr: () => running.stderr(),
    async halt() {
      await terminate(running.child);
    },
    async start() {
      running = await launch(bin, dataDir);
    },
    async seedReview(taskId = "live-task-1") {
      await terminate(running.child);
      const out = execFileSync(
        join(TARGET, "debug", "examples", "seed_review"),
        [dataDir, realpathSync(repo), join(workDir, "worktrees"), taskId],
        { encoding: "utf8" },
      );
      running = await launch(bin, dataDir);
      seeded = JSON.parse(out);
      return seeded;
    },
    async stop() {
      await terminate(running.child);
      rmSync(workDir, { recursive: true, force: true });
    },
  };
  if (seed) await d.seedReview();
  return d;
}

// ---- host bridge shim ----
const SHIM = `
(() => {
  let next = 1;
  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) { const id = next++; window['_' + id] = cb; return id; },
    unregisterCallback(id) { delete window['_' + id]; },
    convertFileSrc(p) { return p; },
    async invoke(cmd, args) {
      const res = await window.__bridgeInvoke(cmd, JSON.stringify(args ?? {}));
      if (res.ok) return res.value;
      throw res.error;
    },
  };
})();
`;

export async function installBridge(page, daemonRef) {
  // daemonRef: { dataDir } read lazily so the daemon can be restarted underneath the page
  const conns = new Map();
  let nextConn = 1;
  const log = [];
  const sent = [];
  await page.evaluateOnNewDocument(SHIM);
  await page.exposeFunction("__bridgeInvoke", async (cmd, argsJson) => {
    const args = JSON.parse(argsJson);
    try {
      if (cmd === "daemon_open") {
        const channelId = Number(String(args.onFrame).replace("__CHANNEL__:", ""));
        const { client, welcome } = await RawClient.connect(daemonRef.dataDir, args.requested);
        const id = nextConn++;
        let index = 0;
        const deliver = (payload) => page.evaluate((c, i, p) => window["_" + c]?.({ index: i, message: p }), channelId, index++, payload).catch(() => {});
        client.listeners.push((message) => {
          if (message.id === 0) return;
          deliver({ kind: "message", message });
        });
        client.closeListeners.push((reason) => {
          conns.delete(id);
          deliver({ kind: "closed", reason });
        });
        conns.set(id, client);
        log.push(["open", id]);
        return { ok: true, value: { connectionId: id, welcome, launchedByHost: false } };
      }
      if (cmd === "daemon_send") {
        const client = conns.get(args.connectionId);
        if (!client) return { ok: false, error: { code: "closed", message: "no such connection" } };
        if (["handshake", "shutdown"].includes(args.message?.method)) return { ok: false, error: { code: "protocol", message: "forbidden method" } };
        client.send(args.message);
        log.push(["send", args.message?.method]);
        sent.push(args.message);
        return { ok: true, value: null };
      }
      if (cmd === "daemon_close") {
        conns.get(args.connectionId)?.socket.destroy();
        conns.delete(args.connectionId);
        return { ok: true, value: null };
      }
      return { ok: false, error: { code: "protocol", message: `unknown command ${cmd}` } };
    } catch (e) {
      log.push(["error", cmd, e.code, e.message]);
      return { ok: false, error: { code: e.code ?? "protocol", message: e.message ?? String(e) } };
    }
  });
  return {
    log,
    sent,
    // Simulates a connection drop (as a crashed daemon would) without stopping the daemon.
    severAll() {
      for (const c of conns.values()) c.socket.destroy();
    },
    open: () => conns.size,
  };
}

// ---- page helpers ----
export function url({ mode, route }) {
  const q = mode === "fixture" ? "?fixture=1" : "";
  return `${APP}${q}#/${route}`;
}

export async function newPage(browser, { width, height = 900, theme, reduceMotion = false, scheme, mode, daemon, bridgeRef }) {
  const context = await browser.createBrowserContext();
  const page = await context.newPage();
  await page.setViewport({ width, height, deviceScaleFactor: 1 });
  const features = [];
  if (reduceMotion) features.push({ name: "prefers-reduced-motion", value: "reduce" });
  if (scheme) features.push({ name: "prefers-color-scheme", value: scheme });
  if (features.length) await page.emulateMediaFeatures(features);
  const errors = [];
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  if (theme) await page.evaluateOnNewDocument((t) => localStorage.setItem("agent-orch.theme", t), theme);
  let bridge = null;
  if (mode === "live") bridge = await installBridge(page, bridgeRef ?? daemon);
  return { page, context, errors, bridge };
}

export async function runAxe(page) {
  await page.evaluate(AXE_SOURCE);
  const result = await page.evaluate(() =>
    // eslint-disable-next-line no-undef
    axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"] } }),
  );
  return result.violations.map((v) => ({
    id: v.id,
    impact: v.impact,
    help: v.help,
    nodes: v.nodes.slice(0, 4).map((n) => ({ target: n.target, summary: n.failureSummary?.split("\n").slice(0, 3).join(" | ") })),
    count: v.nodes.length,
  }));
}

export const pageMetrics = (page) =>
  page.evaluate(() => {
    const de = document.documentElement;
    const strip = document.querySelector(".shell-strip, [data-testid='resource-strip']");
    const r = strip?.getBoundingClientRect();
    const main = document.getElementById("shell-main");
    const res = document.querySelector(".shell-resources");
    // Descendants of main whose boxes extend below main's own box overlap the next grid row (only meaningful when main does not scroll).
    const mainScrolls = main ? getComputedStyle(main).overflowY !== "visible" : false;
    return {
      scrollW: de.scrollWidth,
      docOverflowY: document.scrollingElement.scrollHeight - document.scrollingElement.clientHeight,
      clientW: de.clientWidth,
      hScroll: de.scrollWidth > de.clientWidth + 1,
      theme: de.getAttribute("data-theme"),
      title: document.title,
      stripHeight: r ? Math.round(r.height) : null,
      resourcesHeight: res ? Math.round(res.getBoundingClientRect().height) : null,
      vh: window.innerHeight,
      mainOverflowsBox: main && !mainScrolls ? main.scrollHeight - main.clientHeight : 0,
      activityOverlapsMain: (() => {
        const a = document.querySelector(".shell-activity")?.getBoundingClientRect();
        const m = main?.getBoundingClientRect();
        if (!a || !m || a.height === 0) return false;
        const last = [...main.querySelectorAll("*")].reduce((b, el) => Math.max(b, el.getBoundingClientRect().bottom), 0);
        return last > a.top + 1 && a.top >= m.top && a.left < m.right && a.right > m.left && last - a.top > 1 && a.top > m.top + 5 && getComputedStyle(main).overflowY === "visible";
      })(),
      conn: document.querySelector("[data-testid='shell-connection']")?.innerText.replace(/\s+/g, " ").slice(0, 200) ?? null,
    };
  });
