import { useEffect, useId, useRef } from "react";
import type { DaemonError } from "../../../daemon/errors";
import type { ActionProposal, Decision } from "../../../sources/surfaces/types";
import type { Route } from "../../router";
import { NavLink } from "../../shell/NavLink";
import { Badge, Button, Disclosure, Pill } from "../../theme";
import { Notice } from "../../widgets";
import {
  actionPhrase,
  approvedByText,
  cardTitle,
  doneOnItsOwnText,
  fieldsOf,
  openTaskId,
  openWorkstreamId,
  overrideLines,
  requestErrorText,
  viewOf,
  type WorkstreamNames,
} from "./actionCardModel";
import "./ActionCard.css";

export interface ActionCardProps {
  proposal: ActionProposal;
  parent: ActionProposal | null;
  names: WorkstreamNames;
  /** The workstream this conversation belongs to; null for Global. */
  scope: string | null;
  /** Where a task's page is, or null while the app has not listed that task. A project id is needed, and the card holds none. */
  taskRoute: (taskId: string) => Route | null;
  live: boolean;
  canDecide: boolean;
  /** False while the daemon session is down, so the card asks the user to reconnect. */
  connected: boolean;
  /** False when this session is not the approver (DPES-1105). */
  approver: boolean;
  deciding: boolean;
  /** A fixture receipt. Nothing was sent. */
  notice: string | null;
  error: DaemonError | null;
  onDecide: (decision: Decision) => void;
}

/** How many facts the compact card shows; the Details section lists every field. */
const FACT_COUNT = 2;

/** One proposed action under the reply that proposed it (DPES-1105). Its state is only what the daemon reported. */
export function ActionCard({
  proposal,
  parent,
  names,
  scope,
  taskRoute,
  live,
  canDecide,
  connected,
  approver,
  deciding,
  notice,
  error,
  onDecide,
}: ActionCardProps) {
  const headingId = useId();
  const pillId = useId();
  const messageId = useId();
  const articleRef = useRef<HTMLElement>(null);
  const wasDeciding = useRef(false);
  const view = viewOf(proposal, parent, { canDecide, connected, approver, deciding, names });
  const phrase = actionPhrase(proposal, names);
  const link = view.state === "executed" ? openWorkstreamId(proposal, parent, scope) : null;
  const taskId = view.state === "executed" ? openTaskId(proposal) : null;
  const taskLink = taskId !== null ? taskRoute(taskId) : null;
  const approvedBy = approvedByText(proposal);
  const onItsOwn = doneOnItsOwnText(proposal);
  const fields = fieldsOf(proposal, names, parent);
  // "Nothing runs until you approve." is the card's resting message; every other message says why something is off or what happened.
  const restingMessage = view.state === "pending" && view.message.tone === "info";
  const messageNotice = (
    <Notice id={messageId} tone={view.message.tone}>
      {view.message.text}
    </Notice>
  );

  // The buttons leave the page once the state settles, so keep focus on the card instead of letting it fall to the body.
  useEffect(() => {
    if (deciding) {
      wasDeciding.current = true;
      return;
    }
    if (!wasDeciding.current) return;
    wasDeciding.current = false;
    const active = document.activeElement;
    if (active === null || active === document.body || articleRef.current?.contains(active)) articleRef.current?.focus();
  }, [deciding]);

  return (
    // The card sits inside the transcript's role="log", which announces additions. Outcomes are announced once,
    // through the conversation's polite region, so the card opts out and stays visible.
    <article
      ref={articleRef}
      tabIndex={-1}
      className="ws-action-card"
      aria-live="off"
      aria-labelledby={headingId}
      aria-describedby={pillId}
    >
      <header className="ws-action-head">
        <h3 id={headingId}>{cardTitle(proposal.kind, proposal.params)}</h3>
        <Pill id={pillId} tone={view.pill.tone}>
          {view.pill.text}
        </Pill>
        {!live && <Badge tone="placeholder">Fixture data</Badge>}
      </header>
      <dl className="ws-action-fields">
        {fields.slice(0, FACT_COUNT).map((item) => (
          <div key={item.label} className="ws-action-field">
            <dt>{item.label}</dt>
            <dd>{typeof item.value === "string" ? item.value : `${item.value[0]}${item.value.length > 1 ? " …" : ""}`}</dd>
          </div>
        ))}
      </dl>
      {!restingMessage && messageNotice}
      {notice !== null && <Notice tone="info">{notice}</Notice>}
      {error !== null && <Notice tone="bad">{requestErrorText(error, parent)}</Notice>}
      {view.state === "pending" && (
        <div className="ws-action-buttons">
          <Button
            variant={view.approve.variant}
            aria-label={`Approve: ${phrase}`}
            aria-describedby={view.approve.enabled ? undefined : messageId}
            disabled={!view.approve.enabled}
            onClick={() => onDecide("approve")}
          >
            Approve
          </Button>
          <Button
            variant="secondary"
            aria-label={`Reject: ${phrase}`}
            aria-describedby={view.reject.enabled ? undefined : messageId}
            disabled={!view.reject.enabled}
            onClick={() => onDecide("reject")}
          >
            Reject
          </Button>
        </div>
      )}
      {(taskLink !== null || link !== null) && (
        <div className="ws-action-buttons">
          {taskLink !== null && (
            <NavLink to={taskLink} className="ws-chip-link">
              Open task
            </NavLink>
          )}
          {link !== null && (
            <NavLink to={{ name: "global", view: "workstreams", workstreamId: link }} className="ws-chip-link">
              Open workstream
            </NavLink>
          )}
        </div>
      )}
      <Disclosure summary="Details" className="ws-action-details">
        <dl className="ws-action-fields">
          {fields.map((item) => (
            <div key={item.label} className="ws-action-field">
              <dt>{item.label}</dt>
              <dd>
                {typeof item.value === "string" ? (
                  item.value
                ) : (
                  <ul>
                    {item.value.map((line, index) => (
                      <li key={index}>{line}</li>
                    ))}
                  </ul>
                )}
              </dd>
            </div>
          ))}
        </dl>
        {restingMessage && messageNotice}
        {approvedBy !== null && <p className="ws-action-approver">{approvedBy}</p>}
        {onItsOwn !== null && <p className="ws-action-approver">{onItsOwn}</p>}
        {overrideLines(proposal).map((line) => (
          <p key={line} className="ws-action-approver">
            {line}
          </p>
        ))}
      </Disclosure>
    </article>
  );
}
