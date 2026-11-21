import { describe, expect, it, vi } from "vitest";
import type { ModeGroup, ModeOption } from "../../../sources/surfaces/types";
import { effectiveMode, modeOffer, readSavedMode, saveMode, selectedOptions } from "./modeChoice";

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => void data.set(key, value)),
    removeItem: vi.fn((key: string) => void data.delete(key)),
  };
}

function option(id: string, overrides: Partial<ModeOption> = {}): ModeOption {
  return {
    id,
    label: id,
    description: "",
    available: true,
    unavailableReason: null,
    needsConfirmation: false,
    isDefault: false,
    allowsActions: true,
    ...overrides,
  };
}

const GROUPS: ModeGroup[] = [
  {
    id: "permission",
    label: "Permission",
    options: [
      option("auto", { label: "Auto", isDefault: true }),
      option("plan", { label: "Plan" }),
      option("full", { label: "Full", needsConfirmation: true }),
      option("retired", { label: "Retired", available: false, unavailableReason: "Not offered." }),
    ],
  },
  {
    id: "work",
    label: "Work",
    options: [option("standard", { label: "Standard", isDefault: true }), option("goal", { label: "Goal", available: false })],
  },
];

describe("saving a chat mode per agent", () => {
  it("saves under the agent's own key", () => {
    const storage = memoryStorage();
    saveMode("codex", { permission: "plan" }, GROUPS, storage);
    expect(storage.data.get("agent-orch.chat.mode.codex")).toBe(JSON.stringify({ permission: "plan" }));
  });

  it("never saves an option that needs confirmation, so a reload starts from the default", () => {
    const storage = memoryStorage();
    saveMode("codex", { permission: "full", work: "standard" }, GROUPS, storage);
    expect(JSON.parse(storage.data.get("agent-orch.chat.mode.codex") ?? "{}")).toEqual({ work: "standard" });
  });

  it("ignores an id the catalog no longer offers, on save and on read", () => {
    const storage = memoryStorage({ "agent-orch.chat.mode.codex": JSON.stringify({ permission: "gone", work: "standard" }) });
    expect(readSavedMode("codex", GROUPS, storage)).toEqual({ work: "standard" });
    saveMode("codex", { permission: "retired" }, GROUPS, storage);
    expect(storage.data.get("agent-orch.chat.mode.codex")).toBe("{}");
  });

  it("reads nothing from unreadable or malformed storage", () => {
    expect(readSavedMode("codex", GROUPS, memoryStorage({ "agent-orch.chat.mode.codex": "not json" }))).toEqual({});
    expect(readSavedMode("codex", GROUPS, null)).toEqual({});
  });
});

describe("the mode each group runs with", () => {
  it("takes the default for a group left out, and the chosen option when it is still offered", () => {
    expect(effectiveMode(GROUPS, {})).toEqual({ permission: "auto", work: "standard" });
    expect(effectiveMode(GROUPS, { permission: "plan" })).toEqual({ permission: "plan", work: "standard" });
  });

  it("falls back to the default when the chosen option is no longer available", () => {
    expect(effectiveMode(GROUPS, { permission: "retired", work: "goal" })).toEqual({ permission: "auto", work: "standard" });
  });

  it("lists the selected options in group order, for the trigger's labels", () => {
    expect(selectedOptions(GROUPS, { permission: "plan", work: "standard" }).map((item) => item.label)).toEqual(["Plan", "Standard"]);
  });
});

describe("what the mode control offers", () => {
  it("offers nothing to a routed message, and says to choose an agent", () => {
    expect(modeOffer(undefined, true, undefined)).toEqual({
      groups: [],
      disabled: { label: "Default mode", reason: "Choose an agent to pick its mode." },
    });
  });

  it("says a daemon without chat modes needs an update", () => {
    expect(modeOffer("codex", false, { status: "ok", value: GROUPS, observedAt: "x", ageMs: 0 })).toEqual({
      groups: [],
      disabled: { label: "Needs an updated daemon", reason: "This daemon predates chat modes. Update the daemon to choose a mode." },
    });
  });

  it("offers the groups the daemon reports", () => {
    expect(modeOffer("codex", true, { status: "ok", value: GROUPS, observedAt: "x", ageMs: 0 })).toEqual({ groups: GROUPS, disabled: null });
  });

  it("shows an unconfirmed read as not confirmed, and an unsupported one as the default", () => {
    expect(modeOffer("codex", true, { status: "unknown", reason: "retry shortly" }).disabled).toEqual({
      label: "Modes not confirmed",
      reason: "retry shortly",
    });
    expect(modeOffer("codex", true, { status: "unsupported", reason: "no modes" }).disabled).toEqual({
      label: "Default mode",
      reason: "no modes",
    });
  });
});
