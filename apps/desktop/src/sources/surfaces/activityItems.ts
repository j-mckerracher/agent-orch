/** Builds the ordered thinking and tool items of one agent run from the daemon's activity rows, for chat turns and task sessions alike. */
import type { LiveActivityWire } from "../../daemon/types";
import type { ActivityItem, LiveActivity, ToolItem } from "./types";

/** Where a started tool sits, so its finish row updates the same item even in a later run. */
export interface ToolSlot {
  items: ActivityItem[];
  index: number;
}

function parse(body: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(body);
    return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

const text = (value: unknown): string | undefined => (typeof value === "string" ? value : undefined);

/**
 * Adds one row to `items`: thinking joins an adjacent thinking item, a tool start appends a running tool, a tool finish
 * updates its start wherever that is. Items are replaced, never changed, so an unchanged item keeps its identity.
 * Returns false for a row it does not know or cannot read, which changes nothing.
 */
export function addActivity(items: ActivityItem[], tools: Map<string, ToolSlot>, sequence: number, kind: string, body: string): boolean {
  if (kind === "thinking" || kind === "thinking_chunk") {
    const last = items[items.length - 1];
    if (last?.kind === "thinking") items[items.length - 1] = { ...last, text: last.text + body };
    else items.push({ kind: "thinking", key: `t${sequence}`, text: body });
    return true;
  }
  if (kind !== "tool_started" && kind !== "tool_finished") return false;
  const row = parse(body);
  if (!row) return false;
  if (kind === "tool_started") {
    if (row.truncated === true) {
      items.push({ kind: "cap", key: `c${sequence}` });
      return true;
    }
    const id = text(row.id);
    if (id === undefined) return false;
    items.push({ kind: "tool", key: `u${sequence}`, id, name: text(row.name) ?? "tool", summary: text(row.summary) ?? "", state: "running", result: "" });
    tools.set(id, { items, index: items.length - 1 });
    return true;
  }
  const id = text(row.id);
  const slot = id === undefined ? undefined : tools.get(id);
  if (!slot || id === undefined) return false;
  slot.items[slot.index] = { ...(slot.items[slot.index] as ToolItem), state: row.ok === false ? "failed" : "ok", result: text(row.summary) ?? "" };
  tools.delete(id);
  return true;
}

export function mapLive(wire: LiveActivityWire, receivedAt: number): LiveActivity {
  return { phase: wire.phase, tool: wire.tool, phaseMs: wire.phase_ms, idleMs: wire.idle_ms, receivedAt };
}
