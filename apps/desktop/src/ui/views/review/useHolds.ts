/** Held decisions for the Review view: the holds in force, and the two user-only changes (DPES-1105 Phase 4d). */
import { useCallback, useMemo, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { FIXTURE_HOLD_NOTICE, heldSets, type HeldSets } from "../../../sources/surfaces/holds";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { HoldItem, HoldReceipt, HoldSubjectKind } from "../../../sources/surfaces/types";
import { type Action, type Outcome, useAction } from "../../action";
import { useGate, useServices, useSurface } from "../../services";
import { useRetained } from "../workspace/useRetained";
import { formatHoldDay, holdErrorText } from "./holdDates";

/** A hold ends on its date with no push from the daemon, so the list is read again on this interval. */
export const HOLDS_POLL_MS = 60_000;

/** The `busy` value while a hold or a release runs: each control compares with its own, for its spinner. */
export const holdBusyKey = (kind: HoldSubjectKind, subjectId: string) => `hold:${kind}:${subjectId}`;
export const releaseBusyKey = (holdId: string) => `release:${holdId}`;

const NO_HOLDS: HoldItem[] = [];

export interface HoldsModel {
  /** False for a live daemon without the `holds` capability: no Held tab, no controls, no error. */
  supported: boolean;
  /** True while the first read has not answered. */
  loading: boolean;
  /** Why the list could not be read; null while it can. */
  unavailable: string | null;
  items: HoldItem[];
  held: HeldSets;
  fixture: boolean;
  /** The visible reason the hold and release controls are off; null when they work. */
  blocked: string | null;
  /** What is being changed now (see holdBusyKey and releaseBusyKey); null when idle. */
  busy: string | null;
  /** The last hold or release: pending, then what happened. Render it with ActionStatus beside the controls. */
  action: Action;
  /** Holds every pending approval of a task until `until` (`YYYY-MM-DD`). Resolves true when the daemon confirmed it. */
  hold: (kind: HoldSubjectKind, subjectId: string, until: string, label: string) => Promise<boolean>;
  /** Ends a hold early. Resolves true when the daemon confirmed it. */
  release: (hold: HoldItem, label: string) => Promise<boolean>;
}

export function useHolds(): HoldsModel {
  const { data, surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.holds;
  const live = source.provenance.kind === "daemon";
  // Read from the negotiated list, not `gate.can`, so the tab does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("holds");
  const [nonce, setNonce] = useState(0);
  const [key, setKey] = useState<string | null>(null);
  const action = useAction();
  // Busy lasts as long as the action is pending, which includes its minimum visible time, so a fast answer still shows a spinner.
  const busy = action.pending ? key : null;
  const [local, setLocal] = useState<{ base: unknown; items: HoldItem[] } | null>(null);

  const loaded = useSurface(
    () =>
      supported
        ? source.list()
        : Promise.resolve(notOk<HoldItem[]>(source.provenance, "unsupported", "This daemon has no holds.", observation(Date.now()))),
    `holds#${nonce}`,
    source.provenance,
    supported ? { pollMs: HOLDS_POLL_MS, onFocus: true } : undefined,
  );
  const result = useRetained(loaded, "holds");
  const base = supported && result.status === "ok" ? result.data : NO_HOLDS;
  // A confirmed change shows at once; the next read replaces it, because that read is a new result object.
  const items = local !== null && local.base === result ? local.items : base;
  const held = useMemo(() => heldSets(items), [items]);

  const blocked = !live
    ? null
    : !gate.connected
      ? "Reconnect to the daemon to hold or release a decision."
      : !gate.connection.approver
        ? APPROVER_REQUIRED_NOTICE
        : null;

  const change = useCallback(
    async (
      id: string,
      operation: "hold" | "release",
      send: () => Promise<HoldReceipt>,
      apply: (receipt: HoldReceipt) => HoldItem[],
      done: string,
    ): Promise<boolean> => {
      if (busy !== null) return false;
      let confirmed = false;
      await action.run(operation === "hold" ? "Holding…" : "Releasing…", async (): Promise<Outcome> => {
        setKey(id);
        try {
          const receipt = await send();
          setLocal({ base: result, items: apply(receipt) });
          setNonce((value) => value + 1);
          confirmed = true;
          return receipt.simulated ? { kind: "fixture", text: `${done} ${FIXTURE_HOLD_NOTICE}` } : { kind: "saved", text: done };
        } catch (error) {
          throw new Error(holdErrorText(asDaemonError(error), operation));
        } finally {
          // The nav count leaves out held decisions: read the holds now rather than at the next 30 second tick.
          void data.refreshHolds();
        }
      });
      return confirmed;
    },
    [action.run, busy, data, result],
  );

  const hold = useCallback(
    (kind: HoldSubjectKind, subjectId: string, until: string, label: string) =>
      change(
        holdBusyKey(kind, subjectId),
        "hold",
        () => source.set(kind, subjectId, until),
        (receipt) => [...items.filter((item) => !(item.subjectKind === kind && item.subjectId === subjectId)), receipt.hold],
        `Held: ${label} until ${formatHoldDay(until, true)}.`,
      ),
    [change, items, source],
  );

  const release = useCallback(
    (target: HoldItem, label: string) =>
      change(
        releaseBusyKey(target.id),
        "release",
        () => source.release(target.id),
        () => items.filter((item) => item.id !== target.id),
        `Released: ${label}. It is back in the Actionable tab.`,
      ),
    [change, items, source],
  );

  return {
    supported,
    loading: supported && result.status === "loading",
    unavailable: supported && (result.status === "unavailable" || result.status === "unknown") ? result.reason : null,
    items,
    held,
    fixture: !live,
    blocked,
    busy,
    action,
    hold,
    release,
  };
}
