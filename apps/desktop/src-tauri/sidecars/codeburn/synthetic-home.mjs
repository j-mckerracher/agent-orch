// Synthetic Claude Code and Codex session files for today (UTC), for checking the pinned reader.
// No real user data is read or written: the files are invented, and the home folder is a temp dir.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export function todayUtc(now = new Date()) {
  return now.toISOString().slice(0, 10);
}

/**
 * Writes one Claude Code session (two calls, one of them an unpriced model) and one Codex session
 * (one priced call) into `home`. Totals are asserted by scripts/smoke/usage-reader.smoke.mjs.
 */
export function writeSyntheticHome(home, now = new Date()) {
  const today = todayUtc(now);
  const startOfDay = Date.parse(`${today}T00:00:00.000Z`);
  // The reader drops entries stamped after the current time, so the stamp is a second before now,
  // but never before the start of the UTC day.
  const noon = new Date(Math.max(now.getTime() - 1000, startOfDay + 1000)).toISOString();

  const claudeDir = join(home, ".claude", "projects", "-synthetic-demo");
  mkdirSync(claudeDir, { recursive: true });
  const claudeLines = [
    {
      type: "assistant",
      timestamp: noon,
      sessionId: "sess-claude-1",
      cwd: "/synthetic/demo",
      requestId: "req_1",
      uuid: "u1",
      message: {
        id: "msg_1",
        model: "claude-sonnet-4-5-20250929",
        role: "assistant",
        content: [{ type: "text", text: "synthetic" }],
        usage: { input_tokens: 1200, output_tokens: 300, cache_read_input_tokens: 5000, cache_creation_input_tokens: 800 },
      },
    },
    {
      type: "assistant",
      timestamp: noon,
      sessionId: "sess-claude-2",
      cwd: "/synthetic/demo",
      requestId: "req_2",
      uuid: "u2",
      message: {
        id: "msg_2",
        model: "synthetic-unpriced-model-9",
        role: "assistant",
        content: [{ type: "text", text: "synthetic" }],
        usage: { input_tokens: 700, output_tokens: 70, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
      },
    },
  ];
  writeFileSync(join(claudeDir, "sess-claude-1.jsonl"), claudeLines.map((line) => JSON.stringify(line)).join("\n") + "\n");

  const [year, month, day] = today.split("-");
  const codexDir = join(home, ".codex", "sessions", year, month, day);
  mkdirSync(codexDir, { recursive: true });
  const codexLines = [
    { timestamp: noon, type: "session_meta", payload: { id: "synth-codex-1", cwd: "/synthetic/demo" } },
    { timestamp: noon, type: "turn_context", payload: { model: "gpt-5-codex" } },
    {
      timestamp: noon,
      type: "event_msg",
      payload: {
        type: "token_count",
        info: {
          total_token_usage: { input_tokens: 1000, cached_input_tokens: 400, output_tokens: 50, reasoning_output_tokens: 20, total_tokens: 1050 },
          last_token_usage: { input_tokens: 1000, cached_input_tokens: 400, output_tokens: 50, reasoning_output_tokens: 20, total_tokens: 1050 },
        },
      },
    },
  ];
  writeFileSync(join(codexDir, "rollout-synthetic.jsonl"), codexLines.map((line) => JSON.stringify(line)).join("\n") + "\n");
}
