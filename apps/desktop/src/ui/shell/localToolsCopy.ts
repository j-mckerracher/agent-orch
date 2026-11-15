/**
 * Copy and display rules for the other-AI-tools part of the usage pill. The pill composes every sentence
 * about these figures here, from the codes the daemon sends, so the wording is tested in one place.
 * Nothing here is a reported figure: every amount is a list-price estimate from local session files.
 */
import type { LocalToolsProvider, LocalToolsReader, LocalToolsSnapshot, LocalToolsWindow } from "../../sources/surfaces/types";
import { money } from "./usageMoney";

export const OPT_IN_LABEL = "Include usage from other AI tools on this machine";
export const OPT_IN_STATEMENT =
  "Reads usage counters (tokens and call counts) from Claude Code, Codex and GitHub Copilot session files in your home folder, on this machine only. Prompts, project names and file paths are not kept. Nothing is sent anywhere. Figures are estimates, never added to the reported figure above.";
export const ESTIMATE_LABEL = "Estimated from local session files";
export const PRICE_BASIS = "List-price estimate, not a bill.";
export const CHIP_TITLE = `${ESTIMATE_LABEL}. ${PRICE_BASIS} Not added to the reported figure.`;
const CHECKING = "Checking local tools now.";

/** The time since a figure was read, in the pill's words: "23 min" or "less than 1 min". */
export function ageLabel(ageMs: number): string {
  const minutes = Math.round(Math.max(0, ageMs) / 60_000);
  return minutes < 1 ? "less than 1 min" : `${minutes} min`;
}

export function staleLine(ageMs: number): string {
  return `Showing figures from ${ageLabel(ageMs)} ago.`;
}

/** A window start in words, in the window's own timezone; falls back to ISO text for a zone the runtime does not know. */
export function windowLabel(window: LocalToolsWindow): string {
  try {
    const when = new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short", timeZone: window.timezone });
    return `since ${when.format(new Date(window.startMs))} (${window.timezone})`;
  } catch {
    return `since ${new Date(window.startMs).toISOString()} (${window.timezone})`;
  }
}

/** Ends a sentence with one period, whether or not the text already has one. */
function sentence(text: string): string {
  return /[.!?]$/.test(text) ? text : `${text}.`;
}

export interface ReaderNote {
  text: string;
  /** What to do next. Absent when there is nothing for the user to fix. */
  fix?: string;
}

/**
 * The reader's problem in plain words, from its state and the versions the daemon reports. Raw reader
 * output and stderr never reach the copy. `off` has no note: the opt-in control already says so.
 */
export function readerNote(reader: LocalToolsReader): ReaderNote | null {
  const pinned = reader.pinnedVersion;
  const version = reader.version ?? "version unknown";
  switch (reader.state) {
    case "off":
    case "ok":
      return null;
    case "not_started":
      return { text: CHECKING };
    case "not_bundled":
      return { text: "not available: the usage reader is not bundled in this build." };
    case "not_installed":
      return {
        text: `codeburn ${pinned} was not found on PATH or at AGENT_ORCH_CODEBURN_BIN (development builds only).`,
        fix: "Set AGENT_ORCH_CODEBURN_BIN or add codeburn to PATH for this development build.",
      };
    case "node_missing":
      return {
        text: "Node.js was not found, and codeburn needs Node.js 22.15 or newer.",
        fix: "Install Node.js 22.15 or newer, then refresh.",
      };
    case "node_too_old":
      return {
        text: `Node.js ${reader.nodeVersion ?? "version unknown"} is too old for codeburn, which needs 22.15 or newer.`,
        fix: "Install Node.js 22.15 or newer, then refresh.",
      };
    case "unsupported_version":
      return {
        text: `unsupported codeburn ${version}; MCAO supports ${pinned}`,
        fix: `Development builds: point AGENT_ORCH_CODEBURN_BIN at codeburn ${pinned} or remove the other install from PATH.`,
      };
    case "integrity_mismatch":
      return {
        text: `The installed codeburn ${version} files do not match the build MCAO was tested with.`,
        fix: "Reinstall the application (release) or use the pinned codeburn files (development).",
      };
    case "timeout":
      return {
        text: "codeburn did not finish reading session files within 90 seconds.",
        fix: "Refresh later. If it repeats, the session folders may be very large.",
      };
    case "failed":
      return {
        text: `codeburn exited with status ${reader.exitStatus ?? "unknown"}.`,
        fix: "Run codeburn doctor in a terminal to see what it found, then refresh.",
      };
    case "bad_output":
      return {
        text: `codeburn returned output MCAO could not read for version ${pinned}.`,
        fix: "Reinstall the pinned version, or report the problem.",
      };
    case "output_too_large":
      return {
        text: "codeburn returned more output than MCAO accepts.",
        fix: "Reinstall the pinned version, or report the problem.",
      };
    case "wrong_day":
      return {
        text: "codeburn reported a different day from this window.",
        fix: "Refresh; if it repeats, check the machine timezone.",
      };
  }
}

/** The reader note as one line of popover text, or the checking line while a turned-on reader has not answered. */
export function readerLine(snapshot: LocalToolsSnapshot, enabled: boolean): string | null {
  const note = enabled && snapshot.reader.state === "off" ? { text: CHECKING } : readerNote(snapshot.reader);
  if (note === null) return null;
  return note.fix ? `${sentence(note.text)} ${note.fix}` : sentence(note.text);
}

/** Where a development build's reader came from. Bundled readers say nothing: that is the release path. */
export function readerSourceLine(reader: LocalToolsReader): string | null {
  if (reader.source === "path") return "Using the codeburn found on PATH (development builds only).";
  if (reader.source === "override") return "Using the codeburn at AGENT_ORCH_CODEBURN_BIN (development builds only).";
  return null;
}

const countText = (count: number | null, noun: string) => (count === null ? `${noun}s not reported` : `${count} ${count === 1 ? noun : `${noun}s`}`);
const tokenText = (count: number | null) => (count === null ? "not reported" : `${count}`);

export interface ProviderCopy {
  heading: string;
  lines: string[];
}

/**
 * One provider's figures. A provider with no calls has no money figure, and neither does a provider that
 * has not been read: each says so in words. An unpriced model is named, never shown as zero.
 */
export function providerCopy(provider: LocalToolsProvider, figures: LocalToolsWindow | null): ProviderCopy {
  if (provider.state === "no_calls") {
    return { heading: provider.label, lines: ["No calls recorded in this window."] };
  }
  if (provider.state === "not_run" || figures === null) {
    return { heading: provider.label, lines: ["No figures for this window yet."] };
  }
  const lines: string[] = [];
  let heading: string;
  if (provider.estimatedUsd !== null) {
    heading = `${provider.label}: ${money(provider.estimatedUsd, "USD")} estimated${provider.costComplete ? "" : ", lower bound"}`;
  } else {
    heading = `${provider.label}: cost not estimated`;
    lines.push("No call had a list price.");
  }
  lines.push(`${ESTIMATE_LABEL}, ${windowLabel(figures)}. ${PRICE_BASIS}`);
  lines.push(
    `${countText(provider.sessions, "session")}, ${countText(provider.calls, "call")}. Tokens: input ${tokenText(provider.tokens.input)}, output ${tokenText(provider.tokens.output)}, cache read ${tokenText(provider.tokens.cacheRead)}, cache write ${tokenText(provider.tokens.cacheWrite)}.`,
  );
  if (provider.unpricedModels > 0) {
    const models = provider.unpricedModels === 1 ? "model" : "models";
    lines.push(
      `${provider.unpricedModels} ${models} with tokens but no price: ${provider.unpricedTokens} tokens not priced. Total is a lower bound.`,
    );
  }
  return { heading, lines };
}

export interface LocalTotal {
  usd: number;
  /** False when a provider is unread, unpriced, or only partly priced: the total is then a lower bound. */
  complete: boolean;
}

/**
 * Sum of the USD estimates, or null when no provider has a price. Every figure here is USD, so summing is
 * safe. A provider with no calls adds nothing and does not make the total partial.
 */
export function localTotal(providers: readonly LocalToolsProvider[]): LocalTotal | null {
  const priced = providers.filter((provider) => provider.state === "ok" && provider.estimatedUsd !== null);
  if (priced.length === 0) return null;
  const usd = priced.reduce((sum, provider) => sum + provider.estimatedUsd!, 0);
  const complete = providers.every(
    (provider) => provider.state === "no_calls" || (provider.state === "ok" && provider.estimatedUsd !== null && provider.costComplete),
  );
  return { usd, complete };
}

export function totalLine(providers: readonly LocalToolsProvider[]): string | null {
  const total = localTotal(providers);
  if (total !== null) return `Total of priced calls: ${money(total.usd, "USD")} estimated${total.complete ? "" : ", lower bound"}.`;
  return providers.some((provider) => provider.state === "ok") ? "No total: no call had a list price." : null;
}

/**
 * The pill's secondary chip, or null when nothing should show. It appears only while the opt-in is on
 * (`enabled`, which includes a change still being saved). It is never merged into the reported headline.
 */
export function chipText(snapshot: LocalToolsSnapshot | null, enabled: boolean): string | null {
  if (snapshot === null || !enabled) return null;
  const { reader, figures, providers } = snapshot;
  if (figures === null) {
    return reader.state === "not_started" || reader.state === "off" ? "Other tools: checking" : "Other tools: not available";
  }
  const stale = snapshot.stale || reader.state !== "ok" ? " (stale)" : "";
  if (!providers.some((provider) => provider.state === "ok")) return `Other tools: no calls recorded${stale}`;
  const total = localTotal(providers);
  if (total === null) return `Other tools: not priced${stale}`;
  return `Other tools ≈ ${money(total.usd, "USD")} est.${total.complete ? "" : " (partial)"}${stale}`;
}
