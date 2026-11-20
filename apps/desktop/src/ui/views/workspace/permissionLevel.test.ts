import { describe, expect, it, vi } from "vitest";
import type { TaskRecord } from "../../../daemon/types";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import {
  DEFAULT_PERMISSION_LEVEL,
  FULL_AGENT_TEXT,
  FULL_REVIEW_TEXT,
  RESTRICTED_PREFERENCE_KEY,
  TASK_PREFERENCE_KEY,
  agentTexts,
  choiceBlock,
  permissionForSend,
  readSavedLevel,
  saveLevel,
  taskPermissionLevel,
} from "./permissionLevel";

const LEVELS: PermissionLevel[] = ["restricted", "extended", "auto", "full"];

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => void data.set(key, value)),
    removeItem: vi.fn((key: string) => void data.delete(key)),
  };
}

describe("the payload per level and capability", () => {
  it.each(["restricted", "auto", "full"] as const)("sends %s when the daemon advertises permission_levels", (level) => {
    expect(permissionForSend(level, true)).toBe(level);
  });

  it.each(LEVELS)("sends no field at %s when the daemon does not advertise permission_levels", (level) => {
    expect(permissionForSend(level, false)).toBeUndefined();
  });

  it("starts at Auto, the default for chats", () => {
    expect(DEFAULT_PERMISSION_LEVEL).toBe("auto");
    expect(permissionForSend(DEFAULT_PERMISSION_LEVEL, true)).toBe("auto");
  });
});

describe("which levels a chat can choose", () => {
  it("refuses Extended for chat on every daemon, since the daemon refuses it too", () => {
    expect(choiceBlock("extended", true, "chat")).toBe("not available for chat yet");
    expect(choiceBlock("extended", false, "chat")).toBe("not available for chat yet");
  });

  it("offers Restricted, Auto and Full on a daemon with permission levels", () => {
    expect(choiceBlock("restricted", true, "chat")).toBeNull();
    expect(choiceBlock("auto", true, "chat")).toBeNull();
    expect(choiceBlock("full", true, "chat")).toBeNull();
  });

  it("offers only Restricted on an older daemon, and names why the others cannot be chosen", () => {
    expect(choiceBlock("restricted", false, "chat")).toBeNull();
    expect(choiceBlock("auto", false, "chat")).toBe("needs an updated daemon");
    expect(choiceBlock("full", false, "chat")).toBe("needs an updated daemon");
  });

  it("refuses Extended for tasks too, and names tasks in the reason", () => {
    expect(choiceBlock("extended", true, "task")).toBe("read-only git only, so it can't build, test or commit");
    expect(choiceBlock("auto", true, "task")).toBeNull();
  });
});

describe("the level a task ran with", () => {
  const task = (permission_level: string | undefined, permission?: PermissionLevel) =>
    ({ contract: { permission }, permission_level }) as TaskRecord;

  it("reads the explicit level, and falls back to the contract's, then to Restricted for an older task", () => {
    expect(taskPermissionLevel(task("auto", "full"))).toBe("auto");
    expect(taskPermissionLevel(task(undefined, "full"))).toBe("full");
    expect(taskPermissionLevel(task(undefined))).toBe("restricted");
    expect(taskPermissionLevel(task("not-a-level", "auto"))).toBe("auto");
  });
});

describe("the task form's saved Restricted choice", () => {
  it("keeps its own key, so a chat's Restricted choice never reaches a task", () => {
    expect(readSavedLevel(memoryStorage({ [RESTRICTED_PREFERENCE_KEY]: "restricted" }), TASK_PREFERENCE_KEY)).toBe("auto");
    expect(readSavedLevel(memoryStorage({ [TASK_PREFERENCE_KEY]: "restricted" }), TASK_PREFERENCE_KEY)).toBe("restricted");
  });

  it("saves Restricted under the task key only, and removes it for any other level", () => {
    const storage = memoryStorage({ [RESTRICTED_PREFERENCE_KEY]: "restricted" });
    saveLevel("restricted", storage, TASK_PREFERENCE_KEY);
    expect(storage.data.get(TASK_PREFERENCE_KEY)).toBe("restricted");
    expect(storage.data.get(RESTRICTED_PREFERENCE_KEY)).toBe("restricted");
    saveLevel("auto", storage, TASK_PREFERENCE_KEY);
    expect(storage.data.has(TASK_PREFERENCE_KEY)).toBe(false);
  });
});

describe("the wording", () => {
  it("says Full has no review step and that automatic review is not a guarantee", () => {
    expect(FULL_REVIEW_TEXT).toContain("no automatic review step");
    expect(FULL_REVIEW_TEXT).toContain("not a guarantee");
  });

  it("names each agent's own Full consequences", () => {
    expect(FULL_AGENT_TEXT.codex).toContain("~/.codex");
    expect(FULL_AGENT_TEXT["claude-code"]).toContain("skips its permission checks");
    expect(FULL_AGENT_TEXT["github-copilot"]).toContain("No OS sandbox is applied");
  });

  it("names one agent when the message names it, and every agent that could answer when the daemon routes it", () => {
    expect(agentTexts(FULL_AGENT_TEXT, "codex").map((entry) => entry.agent)).toEqual(["codex"]);
    expect(agentTexts(FULL_AGENT_TEXT, undefined).map((entry) => entry.agent)).toEqual([
      "codex",
      "claude-code",
      "github-copilot",
    ]);
    expect(agentTexts(FULL_AGENT_TEXT, "not-an-agent").map((entry) => entry.agent)).toHaveLength(3);
  });
});

describe("the saved preference", () => {
  it("reads Restricted as saved, and reads anything else, or no storage, as the default", () => {
    expect(readSavedLevel(memoryStorage({ [RESTRICTED_PREFERENCE_KEY]: "restricted" }))).toBe("restricted");
    expect(readSavedLevel(memoryStorage({ [RESTRICTED_PREFERENCE_KEY]: "full" }))).toBe("auto");
    expect(readSavedLevel(memoryStorage({ [RESTRICTED_PREFERENCE_KEY]: "auto" }))).toBe("auto");
    expect(readSavedLevel(memoryStorage())).toBe("auto");
    expect(readSavedLevel(null)).toBe("auto");
  });

  it("reads the default when storage throws", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: vi.fn(),
      removeItem: vi.fn(),
    };
    expect(readSavedLevel(broken)).toBe("auto");
  });

  it("saves Restricted and only Restricted", () => {
    const storage = memoryStorage();
    saveLevel("restricted", storage);
    expect(storage.setItem).toHaveBeenCalledWith(RESTRICTED_PREFERENCE_KEY, "restricted");
    expect(storage.data.get(RESTRICTED_PREFERENCE_KEY)).toBe("restricted");
  });

  it.each(["auto", "extended", "full"] as const)("removes the saved value when the level is %s, so it is never saved", (level) => {
    const storage = memoryStorage({ [RESTRICTED_PREFERENCE_KEY]: "restricted" });
    saveLevel(level, storage);
    expect(storage.setItem).not.toHaveBeenCalled();
    expect(storage.data.has(RESTRICTED_PREFERENCE_KEY)).toBe(false);
  });

  it("does not throw when storage refuses the write", () => {
    const refused = {
      getItem: vi.fn(),
      setItem: () => {
        throw new Error("quota");
      },
      removeItem: vi.fn(),
    };
    expect(() => saveLevel("restricted", refused)).not.toThrow();
  });
});
