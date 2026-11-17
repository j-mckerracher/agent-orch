// The proposed actions waiting for a decision (DPES-1105). This list only points at each card: the person decides
// on the card in the conversation (decisions-2 Q8), so there is no Approve or Reject here.
import { useId } from "react";
import type { ConversationTarget, DecisionQueue, ProposalItem } from "../../../sources/surfaces/types";
import type { Route } from "../../router";
import { useGate, useServices, type SurfaceState } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { Badge, Pill } from "../../theme";
import { actionPhrase, cardTitle, type WorkstreamNames } from "../workspace/actionCardModel";
import { targetKey } from "../workspace/targets";
import "./ProposedActions.css";

/** Every card is shown in the Workspace view, in the conversation of its scope. */
const WORKSPACE: Route = { name: "global", view: "workspace" };

/** The queue holds no workstream titles, so a card is named by its action alone and the line above it names the scope. */
const NO_NAMES: WorkstreamNames = new Map();

const targetOf = (scope: ProposalItem["scope"]): ConversationTarget =>
  scope.kind === "global" ? { kind: "global" } : { kind: "workstream", workstreamId: scope.workstreamId };

const whereOf = (scope: ProposalItem["scope"]): string =>
  scope.kind === "global" ? "Global orchestrator" : `Workstream ${scope.workstreamId}`;

const countText = (count: number): string =>
  count === 0
    ? "No proposed actions are waiting."
    : `${count} proposed action${count === 1 ? " is" : "s are"} waiting for a decision.`;

/**
 * Opens the Workspace view on the conversation that holds the card. The route names no conversation, so the click
 * selects it first; a link that is never used changes nothing.
 */
function CardLink({ item }: { item: ProposalItem }) {
  const { drafts } = useServices();
  return (
    <span onClickCapture={() => drafts.setWorkspaceTarget(targetKey(targetOf(item.scope)))}>
      <NavLink to={WORKSPACE} className="pa-link">
        Open the card: {actionPhrase(item.proposal, NO_NAMES)}
      </NavLink>
    </span>
  );
}

export function ProposedActions({ decisions }: { decisions: SurfaceState<DecisionQueue> }) {
  const headingId = useId();
  const { surfaces } = useServices();
  const gate = useGate();
  const live = surfaces.decisions.provenance.kind === "daemon";
  // A daemon without the capability has no proposals to list, so the section is left out, as the cards are.
  if (live && !gate.connection.capabilities.includes("orchestrator_actions")) return null;
  const fixture = decisions.status === "ok" && decisions.provenance.kind === "fixture";
  const items = decisions.status === "ok" ? decisions.data.proposals : [];
  return (
    <section aria-labelledby={headingId} className="rd-card" data-testid="proposed-actions">
      <header className="rd-card-head">
        <h2 id={headingId}>Proposed actions</h2>
        {fixture && <Badge tone="placeholder">Fixture data</Badge>}
      </header>
      {/* A polite status, so a count that changes while the person is here is read once and never interrupts. */}
      <p className="rd-note" role="status" data-testid="proposed-actions-status">
        {decisions.status === "loading"
          ? "Loading proposed actions…"
          : decisions.status === "ok"
            ? countText(items.length)
            : `Proposed actions are not available: ${decisions.reason}`}
      </p>
      {items.length > 0 && (
        <ul className="rd-rows" aria-label="Proposed actions waiting for a decision">
          {items.map((item) => (
            <li key={item.proposal.id} className="pa-row">
              <span className="rd-row-title">
                {cardTitle(item.proposal.kind)}
                <span>
                  <Pill tone="info">Pending approval</Pill>
                  {fixture && <Badge tone="placeholder">Fixture data</Badge>}
                </span>
              </span>
              <span className="rd-row-sub">{whereOf(item.scope)}</span>
              <CardLink item={item} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
