/**
 * Display labels for agent and provider wire ids (DPES-1105).
 *
 * Wire ids (`codex`, `claude-code`, `github-copilot`, `pi`) are the daemon's identifiers and are
 * never renamed: they stay in requests, stored data, fixtures and tests. Only what the user reads
 * goes through this map. An id with no entry is shown as-is, so an unknown agent is never given a
 * label the daemon did not name.
 */
const AGENT_DISPLAY_NAMES: ReadonlyMap<string, string> = new Map([
  ["codex", "Codex"],
  ["claude-code", "Claude Code"],
  ["github-copilot", "GitHub Copilot"],
  ["pi", "Pi"],
]);

/** The label a user reads for an agent or provider wire id. */
export function agentDisplayName(id: string): string {
  return AGENT_DISPLAY_NAMES.get(id) ?? id;
}
