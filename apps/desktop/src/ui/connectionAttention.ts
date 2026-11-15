/**
 * Whether the connection strip may fold its explanatory notes (C1, DPES-1105).
 *
 * The strip is the honest-state surface, so it stays open whenever the connection needs attention. The rule
 * reads the connection model and the journal identity, never message text:
 * - no bridge (explicit fixture demo mode): no daemon is contacted and nothing live can be lost, so the strip
 *   may fold. The FIXTURE DEMO MODE banner sits outside the folding region and is never hidden;
 * - with a bridge, the strip stays open unless the connection is live and its history is complete.
 *
 * There is no notice severity model: the strip's notices are rendered from these same fields.
 */
import type { ConnectionSnapshot } from "../daemon/connection";
import type { JournalIdentity } from "../sources/types";
import type { SourceState } from "./services";

/** Why the strip must stay open. `null` from {@link connectionAttention} means it may fold. */
export type AttentionReason = "not-connected" | "daemon-error" | "history-incomplete" | "journal-reset";

export interface ConnectionAttentionInput {
  bridgeBound: boolean;
  connection: ConnectionSnapshot;
  journal: SourceState<JournalIdentity>;
}

export function connectionAttention({ bridgeBound, connection, journal }: ConnectionAttentionInput): AttentionReason | null {
  if (!bridgeBound) return null;
  // Offline, connecting, reconnecting, disconnected, unavailable, auth-failed and incompatible all land here.
  if (connection.status !== "connected") return "not-connected";
  if (connection.error !== null) return "daemon-error";
  if (connection.history.incomplete) return "history-incomplete";
  // A reset journal is also flagged by `history.incomplete`; checking it here keeps the rule explicit.
  if (journal.state === "ready" && journal.data.cursorRelationship === "reset") return "journal-reset";
  return null;
}
