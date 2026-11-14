// Review state that must outlive one mounted screen. approvals.list returns pending
// approvals only, so a decision the user already made (and the delivery receipt that
// followed it) exists nowhere but here. A global view that remounts when the user
// navigates away and back shares one session, keyed by the daemon connection, so an
// approved-but-undelivered candidate stays reachable.
import type { Approval, DeliveryResult } from "../daemon/types";
import type { DaemonErrorKind } from "../daemon/errors";

export type MutationView<T> =
  | { phase: "pending" }
  | { phase: "succeeded"; receipt: T }
  | { phase: "failed"; kind: DaemonErrorKind; message: string; indeterminate: boolean; epoch: number };

export interface Mutations {
  decision?: MutationView<Approval>;
  delivery?: MutationView<DeliveryResult>;
}

export interface SessionState {
  /** Approvals this client resolved; the daemon no longer lists them. */
  resolved: Readonly<Record<string, Approval>>;
  mutations: Readonly<Record<string, Mutations>>;
}

export interface ReviewSession {
  state(): SessionState;
  update(fn: (state: SessionState) => SessionState): void;
  subscribe(listener: () => void): () => void;
  /** Idempotency keys by request fingerprint: a retry after a remount reuses the original key. */
  readonly keys: Map<string, string>;
  /** Drops failed mutations, whose refresh-epoch bookkeeping belongs to the screen that made them. */
  settle(): void;
}

export function createReviewSession(): ReviewSession {
  let state: SessionState = { resolved: {}, mutations: {} };
  const listeners = new Set<() => void>();
  const session: ReviewSession = {
    state: () => state,
    update(fn) {
      state = fn(state);
      for (const listener of [...listeners]) listener();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    keys: new Map(),
    settle() {
      const kept: Record<string, Mutations> = {};
      for (const [id, m] of Object.entries(state.mutations)) {
        const next: Mutations = {};
        if (m.decision && m.decision.phase !== "failed") next.decision = m.decision;
        if (m.delivery && m.delivery.phase !== "failed") next.delivery = m.delivery;
        if (next.decision || next.delivery) kept[id] = next;
      }
      state = { ...state, mutations: kept };
    },
  };
  return session;
}

/** One session per owner object (the daemon connection, or an injected test client). */
const sessions = new WeakMap<object, ReviewSession>();

export function sessionFor(owner: object): ReviewSession {
  let session = sessions.get(owner);
  if (!session) {
    session = createReviewSession();
    sessions.set(owner, session);
  }
  session.settle();
  return session;
}
