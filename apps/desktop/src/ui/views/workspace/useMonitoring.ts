/** Monitoring state for the Workspace view: mode, paused scopes and unseen counts, read every 5 seconds (DPES-1105 Phase 3). */
import { useCallback, useState } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { LiveConversationTarget, MonitoringMode, MonitoringState } from "../../../sources/surfaces/types";
import { useGate, useServices, useSurface } from "../../services";
import { FIXTURE_MONITORING_TEXT } from "./monitoringModel";
import { useRetained } from "./useRetained";

/** The daemon has no push stream for monitoring, so the view reads again on this interval. */
export const MONITORING_POLL_MS = 5_000;

export interface MonitoringModel {
  /** False for a live daemon without the `monitoring` capability: no chip, no badge, no error. */
  supported: boolean;
  /** Null until the first read answers, and after a read that failed. */
  state: MonitoringState | null;
  /** The mode being sent, while a change is in flight. */
  pending: MonitoringMode | null;
  error: DaemonError | null;
  /** Set after a fixture change: it saved nothing. */
  note: string | null;
  /** Resolves true when the change went through. */
  setMode: (mode: MonitoringMode) => Promise<boolean>;
  ack: (target: LiveConversationTarget) => Promise<void>;
}

export function useMonitoring(): MonitoringModel {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.monitoring;
  const live = source.provenance.kind === "daemon";
  const supported = !live || gate.can("monitoring");
  const [nonce, setNonce] = useState(0);
  const [pending, setPending] = useState<MonitoringMode | null>(null);
  const [error, setError] = useState<DaemonError | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const loaded = useSurface(
    () =>
      supported
        ? source.get()
        : Promise.resolve(notOk<MonitoringState>(source.provenance, "unsupported", "This daemon has no monitoring.", observation(Date.now()))),
    `monitoring#${nonce}`,
    source.provenance,
    supported ? { pollMs: MONITORING_POLL_MS } : undefined,
  );
  const result = useRetained(loaded, "monitoring");
  const state = supported && result.status === "ok" ? result.data : null;

  const setMode = useCallback(
    async (mode: MonitoringMode) => {
      if (pending !== null) return false;
      setPending(mode);
      setError(null);
      setNote(null);
      try {
        const receipt = await source.set(mode);
        if (receipt.simulated) setNote(FIXTURE_MONITORING_TEXT);
        setNonce((value) => value + 1);
        return true;
      } catch (thrown) {
        setError(asDaemonError(thrown));
        return false;
      } finally {
        setPending(null);
      }
    },
    [source, pending],
  );

  const ack = useCallback(
    async (target: LiveConversationTarget) => {
      await source.ack(target);
      setNonce((value) => value + 1);
    },
    [source],
  );

  return { supported, state, pending, error, note, setMode, ack };
}
