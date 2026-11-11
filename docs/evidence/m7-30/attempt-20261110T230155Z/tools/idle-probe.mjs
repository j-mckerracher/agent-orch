// Does a long-lived daemon stop accepting handshakes? node idle-probe.mjs <seconds> <step-seconds>
import { startDaemon, RawClient, sleep } from "./lib.mjs";
const total = +(process.argv[2] ?? 240);
const step = +(process.argv[3] ?? 30);
const daemon = await startDaemon();
try {
  const t0 = Date.now();
  for (;;) {
    const t = Math.round((Date.now() - t0) / 1000);
    try {
      const { client, welcome } = await RawClient.connect(daemon.dataDir, ["health", "projects"]);
      console.log(t, "ok", JSON.stringify(welcome).slice(0, 80));
      client.socket.destroy();
    } catch (e) {
      console.log(t, "FAIL", JSON.stringify(e));
    }
    if (t >= total) break;
    await sleep(step * 1000);
  }
  console.log("stderr:", daemon.stderr().slice(-600));
} finally {
  await daemon.stop();
}
