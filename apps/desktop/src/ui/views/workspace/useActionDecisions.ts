/**
 * The approve and reject state of the conversations' cards (DPES-1105). Only what the daemon confirmed is kept as
 * a card's state: nothing shows as done until `actions.resolve` answers. Every outcome reloads the transcript and
 * the pending proposals the nav count reads. State is kept per conversation, so switching target does not lose a
 * decision that is still in flight.
 */
import { useRef, useState } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { ActionProposal, ConversationSource, Decision } from "../../../sources/surfaces/types";
import { useServices } from "../../services";
import { reloadWorkstreams } from "../workstreams/model";
import { outcomeAnnouncement, requestErrorText, type WorkstreamNames } from "./actionCardModel";

export interface ActionDecisions {
  /** Proposal ids with a decision in flight. Each card waits on its own decision only. */
  inFlight: ReadonlySet<string>;
  /** The daemon's answer to each decision, by proposal id. */
  confirmed: ReadonlyMap<string, ActionProposal>;
  errors: ReadonlyMap<string, DaemonError>;
  /** Fixture receipts, by proposal id. */
  notices: ReadonlyMap<string, string>;
  decide(proposal: ActionProposal, decision: Decision, parent: ActionProposal | null, names: WorkstreamNames): Promise<void>;
}

interface Decided {
  inFlight: ReadonlySet<string>;
  confirmed: ReadonlyMap<string, ActionProposal>;
  errors: ReadonlyMap<string, DaemonError>;
  notices: ReadonlyMap<string, string>;
}

const NOTHING: Decided = { inFlight: new Set(), confirmed: new Map(), errors: new Map(), notices: new Map() };

function withEntry<V>(map: ReadonlyMap<string, V>, id: string, value: V | null): ReadonlyMap<string, V> {
  const next = new Map(map);
  if (value === null) next.delete(id);
  else next.set(id, value);
  return next;
}

function withFlight(set: ReadonlySet<string>, id: string, on: boolean): ReadonlySet<string> {
  const next = new Set(set);
  if (on) next.add(id);
  else next.delete(id);
  return next;
}

export function useActionDecisions(options: {
  /** The target's option key. Each conversation keeps its own decisions. */
  scope: string;
  resolve: ConversationSource["resolve"];
  /** Writes to the conversation's polite region. */
  announce(text: string): void;
  /** Re-reads the transcript, so the cards show what the daemon now reports. */
  reload(): void;
}): ActionDecisions {
  const { scope, resolve, announce, reload } = options;
  const { data } = useServices();
  const [byScope, setByScope] = useState<ReadonlyMap<string, Decided>>(() => new Map());
  // Guards a second click that lands before the state update does. A decision is never sent twice.
  const sending = useRef(new Set<string>());
  const current = byScope.get(scope) ?? NOTHING;

  const update = (target: string, change: (value: Decided) => Decided) =>
    setByScope((all) => new Map(all).set(target, change(all.get(target) ?? NOTHING)));

  const decide = async (
    proposal: ActionProposal,
    decision: Decision,
    parent: ActionProposal | null,
    names: WorkstreamNames,
  ): Promise<void> => {
    if (sending.current.has(proposal.id)) return;
    sending.current.add(proposal.id);
    update(scope, (value) => ({
      ...value,
      inFlight: withFlight(value.inFlight, proposal.id, true),
      errors: withEntry(value.errors, proposal.id, null),
    }));
    try {
      const receipt = await resolve(proposal.id, decision, proposal.digest);
      if (receipt.proposal !== null) {
        const confirmed = receipt.proposal;
        update(scope, (value) => ({ ...value, confirmed: withEntry(value.confirmed, proposal.id, confirmed) }));
        if (confirmed.state === "executed") reloadWorkstreams();
        announce(outcomeAnnouncement(confirmed, parent, names));
      } else if (receipt.notice !== null) {
        const notice = receipt.notice;
        update(scope, (value) => ({ ...value, notices: withEntry(value.notices, proposal.id, notice) }));
        announce(notice);
      }
    } catch (thrown) {
      const error = asDaemonError(thrown);
      update(scope, (value) => ({ ...value, errors: withEntry(value.errors, proposal.id, error) }));
      announce(requestErrorText(error, parent));
    } finally {
      sending.current.delete(proposal.id);
      update(scope, (value) => ({ ...value, inFlight: withFlight(value.inFlight, proposal.id, false) }));
      reload();
      // Whatever the outcome, the pending list the nav count reads may have changed (also after an error that left it unknown).
      void data.refreshProposals();
    }
  };

  return {
    inFlight: current.inFlight,
    confirmed: current.confirmed,
    errors: current.errors,
    notices: current.notices,
    decide,
  };
}
