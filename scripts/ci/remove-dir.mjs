// Removes a directory that a process may still be releasing.
//
// Windows keeps a directory busy (EBUSY, EPERM, EACCES, ENOTEMPTY) for a moment after the process
// that had it as its working directory exited or was killed. Node's synchronous `rmSync` ignores
// `maxRetries` for EBUSY and throws at once, so the retry loop is here, around the async `rm`.
import { rm } from "node:fs/promises";

const RETRYABLE = new Set(["EBUSY", "EPERM", "EACCES", "ENOTEMPTY"]);

/** Resolves with the number of attempts used. Any other error, or the last busy one, is thrown. */
export const removeDir = async (
  dir,
  {
    remove = (path) => rm(path, { recursive: true, force: true }),
    sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    attempts = 50,
    delayMs = 200,
  } = {},
) => {
  for (let attempt = 1; ; attempt += 1) {
    try {
      await remove(dir);
      return attempt;
    } catch (error) {
      if (!RETRYABLE.has(error?.code) || attempt >= attempts) throw error;
      await sleep(delayMs);
    }
  }
};
