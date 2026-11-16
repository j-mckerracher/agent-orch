import { describe, expect, it, vi } from "vitest";
import { CHOICE_PREFERENCE_KEY, clearSavedChoice, readSavedChoice, saveChoice } from "./modelChoice";

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => void data.set(key, value)),
    removeItem: vi.fn((key: string) => void data.delete(key)),
  };
}

describe("reading the saved choice", () => {
  it("returns nothing when no choice is saved", () => {
    expect(readSavedChoice(memoryStorage())).toBeNull();
  });

  it("returns the agent alone, or with its model and effort", () => {
    expect(readSavedChoice(memoryStorage({ [CHOICE_PREFERENCE_KEY]: JSON.stringify({ agent: "codex" }) }))).toEqual({
      agent: "codex",
    });
    expect(
      readSavedChoice(
        memoryStorage({ [CHOICE_PREFERENCE_KEY]: JSON.stringify({ agent: "claude-code", model: "opus", effort: "high" }) }),
      ),
    ).toEqual({ agent: "claude-code", model: "opus", effort: "high" });
  });

  it.each([
    ["not JSON", "{"],
    ["not an object", JSON.stringify("codex")],
    ["missing its agent", JSON.stringify({ model: "opus" })],
    ["an empty agent", JSON.stringify({ agent: "" })],
    ["a model that is not text", JSON.stringify({ agent: "codex", model: 5 })],
    ["an effort that is null", JSON.stringify({ agent: "codex", effort: null })],
  ])("counts a saved value that is %s as no choice, so no part is dropped and sent as another choice", (_label, raw) => {
    expect(readSavedChoice(memoryStorage({ [CHOICE_PREFERENCE_KEY]: raw }))).toBeNull();
  });

  it("counts unreadable storage as no choice", () => {
    const storage = {
      getItem: vi.fn(() => {
        throw new Error("storage blocked");
      }),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    };
    expect(readSavedChoice(storage)).toBeNull();
  });

  it("reads the app's browser storage when no storage is given", () => {
    saveChoice({ agent: "github-copilot" });
    expect(readSavedChoice()).toEqual({ agent: "github-copilot" });
    window.localStorage.removeItem(CHOICE_PREFERENCE_KEY);
  });
});

describe("clearing the choice", () => {
  it("removes the saved choice, so the picker starts at the daemon default", () => {
    const storage = memoryStorage({ [CHOICE_PREFERENCE_KEY]: JSON.stringify({ agent: "codex" }) });
    clearSavedChoice(storage);
    expect(storage.removeItem).toHaveBeenCalledWith(CHOICE_PREFERENCE_KEY);
    expect(readSavedChoice(storage)).toBeNull();
  });

  it("does not throw when storage is blocked", () => {
    const storage = {
      getItem: vi.fn(() => null),
      setItem: vi.fn(),
      removeItem: vi.fn(() => {
        throw new Error("storage blocked");
      }),
    };
    expect(() => clearSavedChoice(storage)).not.toThrow();
  });
});

describe("saving the choice", () => {
  it("writes the choice under its own key, and reads back the same choice", () => {
    const storage = memoryStorage();
    saveChoice({ agent: "claude-code", model: "opus" }, storage);
    expect(storage.data.get(CHOICE_PREFERENCE_KEY)).toBe(JSON.stringify({ agent: "claude-code", model: "opus" }));
    expect(readSavedChoice(storage)).toEqual({ agent: "claude-code", model: "opus" });
  });

  it("does not throw when the write fails: the choice then lasts only for this session", () => {
    const storage = {
      getItem: vi.fn(() => null),
      setItem: vi.fn(() => {
        throw new Error("quota exceeded");
      }),
      removeItem: vi.fn(),
    };
    expect(() => saveChoice({ agent: "codex" }, storage)).not.toThrow();
  });
});
