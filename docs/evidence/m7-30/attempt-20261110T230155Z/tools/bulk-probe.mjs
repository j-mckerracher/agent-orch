import { startDaemon, RawClient } from "./lib.mjs";
const CAPS = ["health", "projects", "tasks", "sessions", "events", "approvals", "queue", "workspaces", "delivery", "agents", "host", "coordinator", "review", "workstreams"];
const d = await startDaemon();
const { client } = await RawClient.connect(d.dataDir, CAPS);
let id = 100;
const call = (method, params) =>
  Promise.race([
    new Promise((res) => {
      const my = id++;
      client.listeners.push((m) => m.id === my && res(m));
      client.send({ jsonrpc: "2.0", id: my, method, params });
    }),
    new Promise((res) => setTimeout(() => res({ timeout: method }), 5000)),
  ]);
try {
  const t0 = Date.now();
  for (let i = 0; i < 12; i++) {
    const r = await call("workstreams.create", { title: `Bulk ${i}`, idempotency_key: `bulk-${i}` });
    console.log(i, Date.now() - t0, JSON.stringify(r).slice(0, 160));
  }
} finally {
  client.socket.destroy();
  await d.stop();
}
