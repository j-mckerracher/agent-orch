// Does the daemon data dir survive idle time without any access? node dir-probe.mjs <seconds>
import { existsSync } from "node:fs";
import { join } from "node:path";
import { startDaemon, sleep } from "./lib.mjs";
const total = +(process.argv[2] ?? 150);
const daemon = await startDaemon();
try {
  const t0 = Date.now();
  while ((Date.now() - t0) / 1000 < total) {
    await sleep(10000);
    const t = Math.round((Date.now() - t0) / 1000);
    console.log(t, "token", existsSync(join(daemon.dataDir, "daemon.token")), "sock", existsSync(join(daemon.dataDir, "ipc", "daemon.sock")), "dir", existsSync(daemon.workDir));
  }
} finally {
  await daemon.stop();
}
