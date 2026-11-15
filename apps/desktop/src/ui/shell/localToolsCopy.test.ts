import { describe, expect, it } from "vitest";
import type { LocalToolsProvider, LocalToolsReader, LocalToolsSnapshot } from "../../sources/surfaces/types";
import {
  ESTIMATE_LABEL,
  OPT_IN_LABEL,
  ageLabel,
  chipText,
  localTotal,
  providerCopy,
  readerLine,
  readerNote,
  readerSourceLine,
  staleLine,
  totalLine,
} from "./localToolsCopy";

const WINDOW = { startMs: Date.parse("2026-11-14T00:00:00Z"), endMs: Date.parse("2026-11-15T00:00:00Z"), timezone: "UTC" };

function reader(state: LocalToolsReader["state"], extra: Partial<LocalToolsReader> = {}): LocalToolsReader {
  return { state, version: "0.9.28", pinnedVersion: "0.9.28", nodeVersion: "22.16.0", exitStatus: null, source: "bundled", ...extra };
}

function provider(id: LocalToolsProvider["id"], label: string, extra: Partial<LocalToolsProvider> = {}): LocalToolsProvider {
  return {
    id,
    label,
    state: "ok",
    sessions: 1,
    calls: 2,
    tokens: { input: 10, output: 5, cacheRead: null, cacheWrite: null },
    estimatedUsd: 0.01,
    unpricedModels: 0,
    unpricedTokens: 0,
    costComplete: true,
    ...extra,
  };
}

const NOT_RUN = (id: LocalToolsProvider["id"], label: string) =>
  provider(id, label, { state: "not_run", sessions: null, calls: null, tokens: { input: null, output: null, cacheRead: null, cacheWrite: null }, estimatedUsd: null, costComplete: false });

const PROVIDERS = [provider("claude-code", "Claude Code"), provider("codex", "Codex"), provider("github-copilot", "GitHub Copilot")];

function snapshot(extra: Partial<LocalToolsSnapshot> = {}): LocalToolsSnapshot {
  return {
    enabled: true,
    window: WINDOW,
    figures: WINDOW,
    reader: reader("ok"),
    refreshing: false,
    observedAtMs: Date.parse("2026-11-14T10:00:00Z"),
    stale: false,
    providers: PROVIDERS,
    ...extra,
  };
}

const UNAVAILABLE = { figures: null, observedAtMs: null, providers: PROVIDERS.map((p) => NOT_RUN(p.id, p.label)) };

describe("reader copy", () => {
  it.each([
    ["not_installed", { version: null, nodeVersion: "22.16.0" }, "codeburn 0.9.28 was not found on PATH or at AGENT_ORCH_CODEBURN_BIN (development builds only). Set AGENT_ORCH_CODEBURN_BIN or add codeburn to PATH for this development build."],
    ["node_missing", { version: null, nodeVersion: null }, "Node.js was not found, and codeburn needs Node.js 22.15 or newer. Install Node.js 22.15 or newer, then refresh."],
    ["node_too_old", { version: null, nodeVersion: "20.11.1" }, "Node.js 20.11.1 is too old for codeburn, which needs 22.15 or newer. Install Node.js 22.15 or newer, then refresh."],
    ["integrity_mismatch", { version: "0.9.28" }, "The installed codeburn 0.9.28 files do not match the build MCAO was tested with. Reinstall the application (release) or use the pinned codeburn files (development)."],
    ["timeout", {}, "codeburn did not finish reading session files within 90 seconds. Refresh later. If it repeats, the session folders may be very large."],
    ["failed", { exitStatus: 2 }, "codeburn exited with status 2. Run codeburn doctor in a terminal to see what it found, then refresh."],
    ["bad_output", {}, "codeburn returned output MCAO could not read for version 0.9.28. Reinstall the pinned version, or report the problem."],
    ["output_too_large", {}, "codeburn returned more output than MCAO accepts. Reinstall the pinned version, or report the problem."],
    ["wrong_day", {}, "codeburn reported a different day from this window. Refresh; if it repeats, check the machine timezone."],
  ] as const)("%s reads its required copy", (state, extra, expected) => {
    expect(readerLine(snapshot({ reader: reader(state, extra) }), true)).toBe(expected);
  });

  it("gives the exact unsupported-version string with its development fix", () => {
    const line = readerLine(snapshot({ reader: reader("unsupported_version", { version: "0.9.10" }) }), true);
    expect(line).toBe(
      "unsupported codeburn 0.9.10; MCAO supports 0.9.28. Development builds: point AGENT_ORCH_CODEBURN_BIN at codeburn 0.9.28 or remove the other install from PATH.",
    );
  });

  it("says a release build without the reader is not bundled, with no fix", () => {
    expect(readerNote(reader("not_bundled"))).toEqual({ text: "not available: the usage reader is not bundled in this build." });
    expect(readerLine(snapshot({ reader: reader("not_bundled") }), true)).toBe("not available: the usage reader is not bundled in this build.");
  });

  it("never mentions npm in any reader copy, in any state", () => {
    const states = ["not_started", "not_bundled", "not_installed", "node_missing", "node_too_old", "unsupported_version", "integrity_mismatch", "timeout", "failed", "bad_output", "output_too_large", "wrong_day"] as const;
    for (const state of states) {
      const line = readerLine(snapshot({ reader: reader(state, { exitStatus: 2 }) }), true) ?? "";
      expect(line, state).not.toMatch(/npm/i);
    }
  });

  it("names where a development build's reader came from, and says nothing for a bundled one", () => {
    expect(readerSourceLine(reader("ok", { source: "path" }))).toBe("Using the codeburn found on PATH (development builds only).");
    expect(readerSourceLine(reader("ok", { source: "override" }))).toBe("Using the codeburn at AGENT_ORCH_CODEBURN_BIN (development builds only).");
    expect(readerSourceLine(reader("ok", { source: "bundled" }))).toBeNull();
    expect(readerSourceLine(reader("not_bundled", { source: null }))).toBeNull();
  });

  it("shows the checking line while the first read has not finished", () => {
    expect(readerNote(reader("not_started"))).toEqual({ text: "Checking local tools now." });
    expect(readerLine(snapshot({ reader: reader("not_started") }), true)).toBe("Checking local tools now.");
  });

  it("has no note for off or ok", () => {
    expect(readerNote(reader("off"))).toBeNull();
    expect(readerNote(reader("ok"))).toBeNull();
  });

  it("treats a turned-on reader still reporting off as checking", () => {
    expect(readerLine(snapshot({ reader: reader("off") }), true)).toBe("Checking local tools now.");
    expect(readerLine(snapshot({ reader: reader("off") }), false)).toBeNull();
  });

  it("never puts raw reader output or stderr in the copy", () => {
    const copy = readerLine(snapshot({ reader: reader("failed", { exitStatus: 3 }) }), true) ?? "";
    expect(copy).not.toMatch(/stderr|stdout|Error:|\n/);
  });
});

describe("stale copy", () => {
  it("states the age in minutes, and says less than a minute for a fresh read", () => {
    expect(ageLabel(23 * 60_000)).toBe("23 min");
    expect(ageLabel(20_000)).toBe("less than 1 min");
    expect(ageLabel(-5_000)).toBe("less than 1 min");
    expect(staleLine(21 * 60_000)).toBe("Showing figures from 21 min ago.");
  });
});

describe("estimates and totals", () => {
  it("sums only USD estimates, and marks the total partial when a provider is unpriced or unread", () => {
    const whole = localTotal(PROVIDERS);
    expect(whole?.usd).toBeCloseTo(0.03);
    expect(whole?.complete).toBe(true);
    const unpriced = localTotal([PROVIDERS[0]!, provider("codex", "Codex", { unpricedModels: 1, unpricedTokens: 4000, costComplete: false })]);
    expect(unpriced?.usd).toBeCloseTo(0.02);
    expect(unpriced?.complete).toBe(false);
    const unread = localTotal([PROVIDERS[0]!, NOT_RUN("codex", "Codex")]);
    expect(unread?.usd).toBeCloseTo(0.01);
    expect(unread?.complete).toBe(false);
  });

  it("is null, never zero, when no provider has a price", () => {
    expect(localTotal([provider("claude-code", "Claude Code", { estimatedUsd: null, costComplete: false })])).toBeNull();
    expect(totalLine([provider("claude-code", "Claude Code", { estimatedUsd: null, costComplete: false })])).toBe("No total: no call had a list price.");
  });

  it("names the total as a lower bound when partial", () => {
    expect(totalLine([PROVIDERS[0]!, NOT_RUN("codex", "Codex")])).toMatch(/^Total of priced calls: \$0\.01 estimated, lower bound\.$/);
  });
});

describe("provider copy", () => {
  it("labels a priced provider as an estimate with its window and price basis", () => {
    const copy = providerCopy(PROVIDERS[0]!, WINDOW);
    expect(copy.heading).toBe("Claude Code: $0.01 estimated");
    expect(copy.lines.join(" ")).toContain(ESTIMATE_LABEL);
    expect(copy.lines.join(" ")).toContain("since Nov 14, 2026, 12:00 AM (UTC)");
    expect(copy.lines.join(" ")).toContain("List-price estimate, not a bill.");
    expect(copy.lines.join(" ")).toContain("Tokens: input 10, output 5, cache read not reported, cache write not reported.");
  });

  it("names an unpriced model in words and never shows it as zero", () => {
    const copy = providerCopy(provider("github-copilot", "GitHub Copilot", { unpricedModels: 1, unpricedTokens: 4000, costComplete: false }), WINDOW);
    expect(copy.heading).toBe("GitHub Copilot: $0.01 estimated, lower bound");
    expect(copy.lines).toContain("1 model with tokens but no price: 4000 tokens not priced. Total is a lower bound.");
  });

  it("says a provider with calls but no price has no cost estimate", () => {
    const copy = providerCopy(provider("codex", "Codex", { estimatedUsd: null, costComplete: false }), WINDOW);
    expect(copy.heading).toBe("Codex: cost not estimated");
    expect(copy.lines).toContain("No call had a list price.");
  });

  it("shows no money for a provider with no calls", () => {
    const copy = providerCopy(provider("codex", "Codex", { state: "no_calls", estimatedUsd: null }), WINDOW);
    expect(copy).toEqual({ heading: "Codex", lines: ["No calls recorded in this window."] });
  });

  it("shows no money for a provider that has not been read", () => {
    expect(providerCopy(NOT_RUN("codex", "Codex"), WINDOW).lines).toEqual(["No figures for this window yet."]);
    expect(providerCopy(PROVIDERS[0]!, null).lines).toEqual(["No figures for this window yet."]);
  });
});

describe("chip rules", () => {
  it("shows nothing while opted out, whatever the snapshot holds", () => {
    expect(chipText(snapshot({ enabled: false }), false)).toBeNull();
    expect(chipText(snapshot(), false)).toBeNull();
    expect(chipText(null, true)).toBeNull();
  });

  it("shows the estimate as a separate chip, labelled est. and partial when not complete", () => {
    expect(chipText(snapshot(), true)).toBe("Other tools ≈ $0.03 est.");
    expect(chipText(snapshot({ providers: [PROVIDERS[0]!, provider("codex", "Codex", { costComplete: false, unpricedModels: 1, unpricedTokens: 9 })] }), true)).toBe(
      "Other tools ≈ $0.02 est. (partial)",
    );
  });

  it("says checking while the first read is pending, and not available for an unavailable reader", () => {
    expect(chipText(snapshot({ reader: reader("not_started"), ...UNAVAILABLE }), true)).toBe("Other tools: checking");
    expect(chipText(snapshot({ reader: reader("off"), ...UNAVAILABLE }), true)).toBe("Other tools: checking");
    expect(chipText(snapshot({ reader: reader("unsupported_version", { version: "0.9.10" }), ...UNAVAILABLE }), true)).toBe("Other tools: not available");
  });

  it("marks last good figures as stale when the reader has since failed", () => {
    const failed = snapshot({ reader: reader("timeout"), stale: false, observedAtMs: Date.parse("2026-11-14T09:00:00Z") });
    expect(chipText(failed, true)).toBe("Other tools ≈ $0.03 est. (stale)");
  });

  it("never shows a dollar amount for unavailable data", () => {
    for (const state of ["not_started", "not_installed", "node_missing", "timeout", "failed", "wrong_day"] as const) {
      expect(chipText(snapshot({ reader: reader(state), ...UNAVAILABLE }), true)).not.toMatch(/\$/);
    }
    expect(chipText(snapshot({ figures: null, observedAtMs: null, providers: PROVIDERS.map((p) => NOT_RUN(p.id, p.label)) }), true)).not.toMatch(/\$/);
  });

  it("says no calls, not zero dollars, when every provider has no calls", () => {
    const none = PROVIDERS.map((p) => provider(p.id, p.label, { state: "no_calls", estimatedUsd: null, sessions: 0, calls: 0 }));
    expect(chipText(snapshot({ providers: none }), true)).toBe("Other tools: no calls recorded");
  });

  it("says not priced when calls have no price, rather than zero", () => {
    const unpriced = PROVIDERS.map((p) => provider(p.id, p.label, { estimatedUsd: null, costComplete: false, unpricedModels: 1, unpricedTokens: 10 }));
    expect(chipText(snapshot({ providers: unpriced }), true)).toBe("Other tools: not priced");
  });

  it("is never the reported headline and carries no reported wording", () => {
    expect(chipText(snapshot(), true)).not.toMatch(/reported/i);
  });
});

describe("opt-in wording", () => {
  it("uses the exact control label", () => {
    expect(OPT_IN_LABEL).toBe("Include usage from other AI tools on this machine");
  });
});
