import { startDaemon, RawClient } from "./lib.mjs";
const d = await startDaemon({ seed: false });
try {
  const { client } = await RawClient.connect(d.dataDir, ["health", "host"]);
  const r = await new Promise((res) => { client.listeners.push((m) => m.id === 1 && res(m)); client.send({ jsonrpc: "2.0", id: 1, method: "host.metrics", params: {} }); });
  console.log(JSON.stringify(r, null, 1));
  client.socket.destroy();
} finally { await d.stop(); }
