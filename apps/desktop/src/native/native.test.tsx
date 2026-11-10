import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "../App";
import { DaemonError } from "../daemon/errors";
import { FakeDaemon, mkApproval, mkEvent, mkTask, until } from "../test/fakeDaemon";
import { ReviewRoute } from "../review/ReviewRoute";
import { DraftsStore, DRAFTS_STORAGE_KEY } from "../ui/drafts";
import { bootServices, selectMode } from "./boot";
import { createTauriConnector, hostError, insideTauri, type TauriApi } from "./tauriConnector";

/** The Rust host, played by a FakeDaemon: frames out on the channel, commands in. */
class FakeHost implements TauriApi {
  readonly calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];
  openFailure: { code: string; message: string } | null = null;
  welcome: unknown | undefined;
  connections = new Map<number, { close(): void; send(message: unknown): void }>();
  private nextId = 1;

  constructor(readonly daemon: FakeDaemon) {}

  createChannel(onMessage: (payload: unknown) => void): unknown {
    return onMessage;
  }

  async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
    this.calls.push({ command, args });
    if (command === "daemon_open") {
      if (this.openFailure) throw this.openFailure;
      const sink = args.onFrame as (payload: unknown) => void;
      const opened = await this.daemon.open(args.requested as never);
      const id = this.nextId++;
      opened.channel.onMessage((message) => sink({ kind: "message", message }));
      opened.channel.onClose((reason) => sink({ kind: "closed", reason: reason ? "remote" : "local" }));
      this.connections.set(id, opened.channel);
      return { connectionId: id, welcome: this.welcome === undefined ? opened.welcome : this.welcome, launchedByHost: false };
    }
    const connection = this.connections.get(args.connectionId as number);
    if (!connection) throw { code: "closed", message: "no such connection" };
    if (command === "daemon_send") connection.send(args.message);
    else if (command === "daemon_close") connection.close();
    return undefined;
  }

  get ids(): number[] {
    return [...this.connections.keys()];
  }
}

const memoryStorage = () => {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
  };
};

const tauriWindow = (hash = "", search = "") => ({ __TAURI_INTERNALS__: {}, location: { hash, search } });
const browserWindow = (hash = "", search = "") => ({ location: { hash, search } });

const FAST = { backoffMs: [5], requestTimeoutMs: 250 };

const status = () => screen.getByTestId("connection-status");

describe("Tauri host connector", () => {
  it("opens an authenticated connection through the host and forwards JSON-RPC", async () => {
    const fake = new FakeDaemon();
    const host = new FakeHost(fake);
    const opened = await createTauriConnector(host).open(["health", "events"]);
    expect(opened.welcome).toEqual({ version: 1, capabilities: ["health", "events"], journal_epoch: "epoch-1" });
    const replies: unknown[] = [];
    opened.channel.onMessage((message) => replies.push(message));
    opened.channel.send({ jsonrpc: "2.0", id: 1, method: "health" });
    await until(() => replies.length === 1, "reply");
    expect(host.calls.map((call) => call.command)).toEqual(["daemon_open", "daemon_send"]);
    expect(host.calls[0]?.args?.requested).toEqual(["health", "events"]);
    expect(JSON.stringify(host.calls)).not.toMatch(/token/i);
  });

  it("maps host failures to distinct typed errors", async () => {
    const fake = new FakeDaemon();
    const host = new FakeHost(fake);
    const kinds: Record<string, string> = {
      "daemon-unavailable": "daemon-unavailable",
      auth: "auth",
      version: "version",
      "capability-unavailable": "capability-unavailable",
      protocol: "protocol",
      io: "disconnected",
      closed: "disconnected",
    };
    for (const [code, kind] of Object.entries(kinds)) {
      host.openFailure = { code, message: `host says ${code}` };
      await expect(createTauriConnector(host).open(["health"])).rejects.toMatchObject({ kind, message: `host says ${code}` });
    }
    expect(hostError("boom")).toMatchObject({ kind: "protocol", message: "boom" });
    expect(hostError({ code: "unheard-of" }).kind).toBe("protocol");
  });

  it("rejects a malformed welcome and closes that connection", async () => {
    const fake = new FakeDaemon();
    const host = new FakeHost(fake);
    host.welcome = { nope: true };
    await expect(createTauriConnector(host).open(["health"])).rejects.toMatchObject({ kind: "protocol" });
    expect(host.calls.at(-1)?.command).toBe("daemon_close");
  });

  it("reports a daemon-side close as disconnected and a local close as no error", async () => {
    const fake = new FakeDaemon();
    const host = new FakeHost(fake);
    const remote = await createTauriConnector(host).open(["health"]);
    const reasons: Array<DaemonError | null> = [];
    remote.channel.onClose((reason) => reasons.push(reason));
    fake.drop();
    await until(() => reasons.length === 1, "remote close");
    expect(reasons[0]).toMatchObject({ kind: "disconnected" });
    expect(() => remote.channel.send({})).toThrow(/closed/);

    const local = await createTauriConnector(host).open(["health"]);
    const localReasons: Array<DaemonError | null> = [];
    local.channel.onClose((reason) => localReasons.push(reason));
    local.channel.close();
    expect(localReasons).toEqual([null]);
    expect(host.calls.at(-1)?.command).toBe("daemon_close");
  });

  it("surfaces a send the host rejects as a closed channel", async () => {
    const fake = new FakeDaemon();
    const host = new FakeHost(fake);
    const opened = await createTauriConnector(host).open(["health"]);
    const reasons: Array<DaemonError | null> = [];
    opened.channel.onClose((reason) => reasons.push(reason));
    host.connections.clear();
    opened.channel.send({ jsonrpc: "2.0", id: 1, method: "health" });
    await until(() => reasons.length === 1, "send failure");
    expect(reasons[0]).toMatchObject({ kind: "disconnected" });
  });
});

describe("mode selection", () => {
  it("uses the live daemon inside Tauri, even when fixture=1 is asked for", () => {
    expect(insideTauri(tauriWindow())).toBe(true);
    expect(selectMode(tauriWindow("", "?fixture=1"))).toBe("native");
    expect(bootServices(tauriWindow("", "?fixture=1"), { nativeConnector: new FakeDaemon() }).services.bridgeBound).toBe(true);
  });

  it("uses fixtures only for an explicit fixture=1 outside Tauri", () => {
    expect(selectMode(browserWindow("", "?fixture=1"))).toBe("fixture");
    expect(selectMode(browserWindow("", "?fixture=0"))).toBe("no-bridge");
    expect(selectMode(browserWindow())).toBe("no-bridge");
    expect(bootServices(browserWindow("", "?fixture=1")).services.bridgeBound).toBe(false);
    expect(bootServices(browserWindow()).services.bridgeBound).toBe(true);
  });
});

describe("shipped app wiring", () => {
  it("labels fixture demo mode", async () => {
    const { services } = bootServices(browserWindow("", "?fixture=1"));
    render(<App services={services} />);
    expect(await screen.findByTestId("fixture-banner")).toHaveTextContent("FIXTURE DEMO MODE");
  });

  it("without a bridge reports the daemon unavailable and never shows sample data", async () => {
    const { services } = bootServices(browserWindow(), { connection: FAST });
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Daemon unavailable"));
    expect(screen.queryByTestId("fixture-banner")).toBeNull();
    expect(screen.queryByText(/PLACEHOLDER DATA \(D1\)/)).toBeNull();
    expect(screen.getByTestId("recovery-unknown")).toHaveTextContent("Recovery: unknown");
    expect(screen.getByTestId("badge-D3")).toHaveTextContent("PLACEHOLDER DATA");
  });

  it("native: live daemon data, connected note, and no fixture banner", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "p1", repo_path: "/work/repo", created_at: "2026-01-01T00:00:00Z" } as never];
    const host = new FakeHost(fake);
    const { services } = bootServices(tauriWindow(), { nativeConnector: createTauriConnector(host), connection: FAST });
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    expect(screen.queryByTestId("fixture-banner")).toBeNull();
    expect(screen.getByTestId("connected-note")).toHaveTextContent("does not prove any agent is running");
    expect(host.calls[0]?.command).toBe("daemon_open");
    expect(fake.callsTo("projects.list").length).toBeGreaterThan(0);
  });

  it("native S6 uses the daemon review methods, not the fixture daemon", async () => {
    const fake = new FakeDaemon();
    fake.approvals = [mkApproval("a1", "t1")];
    const host = new FakeHost(fake);
    const { services } = bootServices(tauriWindow("#/project/p1/review/t1"), {
      nativeConnector: createTauriConnector(host),
      connection: FAST,
      renderReview: (props) => <ReviewRoute {...props} />,
    });
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    await until(() => fake.callsTo("approvals.list").length > 0, "approvals.list on the daemon");
    expect(screen.queryByText(/FIXTURE DAEMON/)).toBeNull();
  });

  it("native S6 ignores ?fixture=1 on the review route and never renders the fixture daemon", async () => {
    const fake = new FakeDaemon();
    fake.approvals = [mkApproval("a1", "t1")];
    const host = new FakeHost(fake);
    const { services } = bootServices(tauriWindow("#/project/p1/review/t1?fixture=1"), {
      nativeConnector: createTauriConnector(host),
      connection: FAST,
      renderReview: (props) => <ReviewRoute {...props} />,
    });
    expect(services.bridgeBound).toBe(true);
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    await until(() => fake.callsTo("approvals.list").length > 0, "approvals.list on the daemon");
    expect(screen.queryByText(/FIXTURE DAEMON/)).toBeNull();
    expect(screen.queryByText(/PLACEHOLDER DATA — fixture/i)).toBeNull();
  });

  it("native task detail shows recovery as unavailable, never canned fixture facts", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "p1", repo_path: "/work/repo", created_at: "2026-01-01T00:00:00Z" } as never];
    fake.tasks = [mkTask("t1", "p1", { state: "Running" })];
    const host = new FakeHost(fake);
    const { services } = bootServices(tauriWindow("#/project/p1/task/t1"), {
      nativeConnector: createTauriConnector(host),
      connection: FAST,
    });
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    const region = await screen.findByRole("region", { name: "Recovery classification (placeholder)" });
    await waitFor(() => expect(region).toHaveTextContent(/Unavailable: the daemon exposes no recovery status yet/));
    expect(region).toHaveTextContent("unknown");
    expect(region).toHaveTextContent("PLACEHOLDER DATA (D3)");
    expect(region).not.toHaveTextContent(/worktree present|no delivery recorded|reconciling/);
  });

  it("native S6 shows the live failure instead of falling back to fixture data", async () => {
    const fake = new FakeDaemon();
    fake.failNext("approvals.list", { code: -32603, message: "store busy" }, 5);
    const host = new FakeHost(fake);
    const { services } = bootServices(tauriWindow("#/project/p1/review/t1"), {
      nativeConnector: createTauriConnector(host),
      connection: FAST,
      renderReview: (props) => <ReviewRoute {...props} />,
    });
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    await until(() => fake.callsTo("approvals.list").length > 0, "approvals.list attempted");
    expect(screen.queryByText(/FIXTURE DAEMON/)).toBeNull();
    expect(screen.queryByText(/PLACEHOLDER DATA — fixture/i)).toBeNull();
  });
});

describe("C1 window close and reopen", () => {
  it("a closed window leaves the daemon serving and a reopened one reconnects and reloads", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t1", null), mkEvent(2, "t1", null)];
    fake.tasks = [mkTask("t1", "p1")];
    const host = new FakeHost(fake);
    const storage = memoryStorage();

    const first = bootServices(tauriWindow(), { nativeConnector: createTauriConnector(host), storage, connection: FAST }).services;
    const view = render(<App services={first} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    await waitFor(() => expect(screen.getByTestId("last-sequence")).toHaveTextContent("Last durable sequence: 2"));
    first.drafts.setRegisterPath("/work/unsent");
    first.drafts.patchSubmit("p1", { objective: "half-written objective" });

    // pagehide: only this window's connection ends.
    first.connection.stop();
    view.unmount();
    expect(fake.live).toHaveLength(0);
    expect(host.calls.some((call) => call.command === "daemon_close")).toBe(true);
    expect(host.calls.every((call) => call.command !== "shutdown")).toBe(true);

    fake.events.push(mkEvent(3, "t1", null));
    const second = bootServices(tauriWindow(), { nativeConnector: createTauriConnector(host), storage, connection: FAST }).services;
    render(<App services={second} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    // A new connection authenticated again and replayed authoritative state from the start.
    expect(host.calls.filter((call) => call.command === "daemon_open")).toHaveLength(2);
    await waitFor(() => expect(screen.getByTestId("last-sequence")).toHaveTextContent("Last durable sequence: 3"));
    expect(second.drafts.getSnapshot().registerPath).toBe("/work/unsent");
    expect(second.drafts.submitDraft("p1").objective).toBe("half-written objective");
  });

  it("distinguishes unavailable, auth, incompatible and a lost connection", async () => {
    const cases: Array<[string, RegExp]> = [
      ["daemon-unavailable", /Daemon unavailable/],
      ["auth", /Authentication failed/],
      ["version", /Incompatible version or capability/],
    ];
    for (const [code, label] of cases) {
      const host = new FakeHost(new FakeDaemon());
      host.openFailure = { code, message: `host says ${code}` };
      const services = bootServices(tauriWindow(), { nativeConnector: createTauriConnector(host), connection: FAST }).services;
      const view = render(<App services={services} />);
      await waitFor(() => expect(status()).toHaveTextContent(label), { timeout: 15_000 });
      if (code === "daemon-unavailable") {
        expect(screen.getByTestId("connection-error")).toHaveTextContent("not running and could not be started");
        expect(screen.getByTestId("connection-error")).toHaveTextContent("Nothing is known about running work");
      }
      view.unmount();
    }

    const fake = new FakeDaemon();
    const host = new FakeHost(fake);
    const services = bootServices(tauriWindow(), {
      nativeConnector: createTauriConnector(host),
    connection: FAST,
    }).services;
    render(<App services={services} />);
    await waitFor(() => expect(status()).toHaveTextContent("Connected"));
    host.openFailure = { code: "io", message: "pipe broke" };
    fake.drop();
    await waitFor(() => expect(status()).toHaveTextContent(/Reconnecting|Disconnected/));
    expect(screen.getByTestId("offline-note")).toHaveTextContent("never stops the daemon");
  }, 60_000);
});

describe("draft persistence", () => {
  it("restores versioned drafts and discards anything else", () => {
    const storage = memoryStorage();
    const writer = new DraftsStore(storage);
    writer.setRegisterPath("/r");
    writer.setCoordinator("p1", "hello");
    writer.patchSubmit("p1", { objective: "obj", maxRetries: "3" });
    expect(JSON.parse(storage.map.get(DRAFTS_STORAGE_KEY) ?? "{}").registerPath).toBe("/r");

    const reader = new DraftsStore(storage);
    expect(reader.getSnapshot().coordinator).toEqual({ p1: "hello" });
    expect(reader.submitDraft("p1")).toMatchObject({ objective: "obj", maxRetries: "3", kind: "Implement" });

    for (const bad of ["not json", "[]", '{"registerPath":1,"submit":{},"coordinator":{}}', '{"registerPath":""}']) {
      storage.map.set(DRAFTS_STORAGE_KEY, bad);
      expect(new DraftsStore(storage).getSnapshot()).toEqual({ registerPath: "", submit: {}, coordinator: {} });
    }
  });

  it("keeps working when storage throws", () => {
    const store = new DraftsStore({
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("full");
      },
    });
    store.setRegisterPath("/still-here");
    expect(store.getSnapshot().registerPath).toBe("/still-here");
  });
});
