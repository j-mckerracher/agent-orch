/** Adapter inventory derivations for the Agents & Sessions view (M7-14). */
import type { AgentCapabilityEntry } from "../../../daemon/types";
import type { AgentCapability } from "../../../sources/types";

export type Flag = "yes" | "no" | "unknown";

export interface InventoryRow {
  id: string;
  name: string;
  /** Verbatim availability sentence for the assignment column. */
  assignment: string;
  installed: Flag;
  launchable: Flag;
  resume: Flag;
  interactive: Flag;
  /** `null` when the agent is not detected and reports nothing. */
  capabilities: Array<{ label: string; flag: Flag }> | null;
  /** Installed and launchable (live) or reported installed (preview). */
  assignable: boolean;
  note: string | null;
}

type Reported = NonNullable<AgentCapabilityEntry["capabilities"]>;

const FLAG_LABELS: ReadonlyArray<[keyof Reported, string]> = [
  ["streaming", "Streaming"],
  ["interactive_input", "Interactive input"],
  ["structured_output", "Structured output"],
  ["cancellation", "Cancellation"],
  ["session_resumption", "Session resumption"],
  ["approvals", "Approvals"],
  ["model_selection", "Model selection"],
];

const flag = (value: boolean): Flag => (value ? "yes" : "no");

export function assignmentText(installed: boolean, launchable: boolean): string {
  if (installed && launchable) return "Installed and launchable";
  if (installed) return "Installed, not launchable";
  if (launchable) return "Launchable, not detected";
  return "Not detected";
}

/** Exact `agents.capabilities` flags; nothing is inferred or defaulted. */
export function inventoryFromDaemon(entries: readonly AgentCapabilityEntry[]): InventoryRow[] {
  return entries.map((entry) => {
    const reported = entry.capabilities;
    return {
      id: entry.agent,
      name: entry.agent,
      assignment: assignmentText(entry.installed, entry.launchable),
      installed: flag(entry.installed),
      launchable: flag(entry.launchable),
      resume: reported ? flag(reported.session_resumption) : "unknown",
      interactive: reported ? flag(reported.interactive_input) : "unknown",
      capabilities: reported ? FLAG_LABELS.map(([key, label]) => ({ label, flag: flag(reported[key]) })) : null,
      assignable: entry.installed && entry.launchable,
      note: reported ? null : "Capabilities are not reported because the agent was not detected.",
    };
  });
}

const AVAILABILITY_TEXT: Record<AgentCapability["availability"], string> = {
  installed: "Installed",
  available: "Available, not detected",
  unsupported: "Unsupported",
  unknown: "Unknown",
};

/** D1 preview entries carry free-text features, so resume and input stay `unknown` unless named. */
export function inventoryFromPreview(agents: readonly AgentCapability[]): InventoryRow[] {
  const named = (agent: AgentCapability, needle: string): Flag =>
    agent.features.some((feature) => feature.toLowerCase().includes(needle)) ? "yes" : "unknown";
  return agents.map((agent) => ({
    id: agent.id,
    name: agent.name,
    assignment: AVAILABILITY_TEXT[agent.availability],
    installed: agent.availability === "installed" ? "yes" : "unknown",
    launchable: "unknown",
    resume: named(agent, "resum"),
    interactive: named(agent, "interactive"),
    capabilities: agent.features.map((label) => ({ label, flag: "yes" as const })),
    assignable: agent.availability === "installed",
    note: agent.note ?? null,
  }));
}
