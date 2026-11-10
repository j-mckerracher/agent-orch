import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { removeDir } from "./remove-dir.mjs";

const busy = (code = "EBUSY") => Object.assign(new Error(`${code}: resource busy or locked, rmdir`), { code });
const noSleep = () => Promise.resolve();

test("a busy directory is retried until it can be removed", async () => {
  for (const code of ["EBUSY", "EPERM", "EACCES", "ENOTEMPTY"]) {
    let calls = 0;
    const used = await removeDir("d", {
      remove: async () => {
        calls += 1;
        if (calls <= 3) throw busy(code);
      },
      sleep: noSleep,
    });
    assert.equal(used, 4, code);
  }
});

test("the last busy error is thrown once the attempts are used up", async () => {
  let calls = 0;
  const slept = [];
  await assert.rejects(
    removeDir("d", {
      remove: async () => {
        calls += 1;
        throw busy();
      },
      sleep: async (ms) => slept.push(ms),
      attempts: 5,
      delayMs: 7,
    }),
    { code: "EBUSY" },
  );
  assert.equal(calls, 5);
  assert.deepEqual(slept, [7, 7, 7, 7]);
});

test("an error that is not a busy directory is thrown at once", async () => {
  let calls = 0;
  await assert.rejects(
    removeDir("d", {
      remove: async () => {
        calls += 1;
        throw Object.assign(new Error("no such device"), { code: "ENODEV" });
      },
      sleep: noSleep,
    }),
    { code: "ENODEV" },
  );
  assert.equal(calls, 1);
});

test("a real directory tree is removed on the first attempt and a missing one is not an error", async () => {
  const dir = mkdtempSync(join(os.tmpdir(), "remove-dir-"));
  mkdirSync(join(dir, "a", "b"), { recursive: true });
  writeFileSync(join(dir, "a", "b", "f.txt"), "x");
  assert.equal(await removeDir(dir), 1);
  assert.ok(!existsSync(dir));
  assert.equal(await removeDir(dir), 1);
});
