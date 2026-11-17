import { describe, expect, it } from "vitest";
import type { ConnectionSnapshot, ConnectionStatus } from "../daemon/connection";
import { DaemonError } from "../daemon/errors";
import type { JournalIdentity } from "../sources/types";
import { connectionAttention } from "./connectionAttention";

const healthy: ConnectionSnapshot = {
  status: "connected",
  error: null,
  capabilities: [],
  lastSequence: 4,
  lastEventAt: 1,
  lastConnectedAt: 1,
  approver: true,
  history: { incomplete: false, reason: null },
  resumedFrom: 3,
  generation: 1,
  failedAttempts: 0,
  journalEpoch: "epoch-1",
};

const ready = (cursorRelationship: JournalIdentity["cursorRelationship"]) => ({
  state: "ready" as const,
  data: { storeIdentity: "epoch-1", cursorRelationship },
});

const withStatus = (status: ConnectionStatus): ConnectionSnapshot => ({ ...healthy, status });

describe("connectionAttention", () => {
  it("lets a live, complete connection fold", () => {
    expect(connectionAttention({ bridgeBound: true, connection: healthy, journal: ready("same") })).toBeNull();
    expect(connectionAttention({ bridgeBound: true, connection: healthy, journal: { state: "loading" } })).toBeNull();
  });

  it("lets fixture demo mode fold: no daemon is contacted and nothing live can be lost", () => {
    expect(connectionAttention({ bridgeBound: false, connection: withStatus("idle"), journal: { state: "loading" } })).toBeNull();
  });

  it.each<ConnectionStatus>(["idle", "connecting", "reconnecting", "disconnected", "unavailable", "auth-failed", "incompatible"])(
    "keeps the strip open while the status is %s",
    (status) => {
      expect(connectionAttention({ bridgeBound: true, connection: withStatus(status), journal: ready("same") })).toBe(
        "not-connected",
      );
    },
  );

  it("keeps the strip open for a daemon error even when the status says connected", () => {
    const connection = { ...healthy, error: new DaemonError("disconnected", "down") };
    expect(connectionAttention({ bridgeBound: true, connection, journal: ready("same") })).toBe("daemon-error");
  });

  it("keeps the strip open while history is incomplete", () => {
    const connection = { ...healthy, history: { incomplete: true, reason: "slow-consumer" as const } };
    expect(connectionAttention({ bridgeBound: true, connection, journal: ready("same") })).toBe("history-incomplete");
  });

  it("keeps the strip open after a journal reset", () => {
    expect(connectionAttention({ bridgeBound: true, connection: healthy, journal: ready("reset") })).toBe("journal-reset");
  });

  it("reports the status before the history or journal when several apply", () => {
    const connection = { ...withStatus("reconnecting"), history: { incomplete: true, reason: "replay-gap" as const } };
    expect(connectionAttention({ bridgeBound: true, connection, journal: ready("reset") })).toBe("not-connected");
  });
});
