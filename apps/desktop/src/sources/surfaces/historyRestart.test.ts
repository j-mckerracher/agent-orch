import { describe, expect, it, vi } from "vitest";
import { DaemonError } from "../../daemon/errors";
import { HistoryChanged, restartOnHistoryChange } from "./conversationLive";

describe("restartOnHistoryChange (DPES-1105)", () => {
  it("returns the first read when nothing was cleared", async () => {
    const read = vi.fn(async () => "log");
    await expect(restartOnHistoryChange(read)).resolves.toBe("log");
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("starts the read over after a clear landed mid-read", async () => {
    const read = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new HistoryChanged())
      .mockResolvedValueOnce("empty after clear");
    await expect(restartOnHistoryChange(read)).resolves.toBe("empty after clear");
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("treats a replay gap, a cursor past a cleared log, as a reset rather than an error", async () => {
    const read = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new DaemonError("replay-gap", "cursor ahead"))
      .mockResolvedValueOnce("reloaded from the start");
    await expect(restartOnHistoryChange(read)).resolves.toBe("reloaded from the start");
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("rethrows any other error at once", async () => {
    const failure = new DaemonError("not-found", "gone");
    const read = vi.fn<() => Promise<string>>().mockRejectedValue(failure);
    await expect(restartOnHistoryChange(read)).rejects.toBe(failure);
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("gives up after a bounded number of restarts instead of looping", async () => {
    const read = vi.fn<() => Promise<string>>().mockRejectedValue(new HistoryChanged());
    await expect(restartOnHistoryChange(read)).rejects.toBeInstanceOf(HistoryChanged);
    expect(read).toHaveBeenCalledTimes(4);
  });
});
