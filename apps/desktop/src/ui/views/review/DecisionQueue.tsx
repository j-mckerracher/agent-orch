import { useRef, useState, type KeyboardEvent } from "react";
import type { Approval } from "../../../daemon/types";
import type { DecisionItem } from "../../../sources/surfaces/types";
import type { ReviewState } from "../../../review/useReview";
import { Badge, Button, EmptyState, Pill, type Tone } from "../../theme";
import {
  KIND_LABEL,
  STATE_LABEL,
  TABS,
  approvalKind,
  filterByTab,
  formatResponseTime,
  metaKey,
  type TabId,
} from "./queue";

const STATE_TONE: Record<Approval["state"]["state"], Tone> = {
  requested: "info",
  approved: "ok",
  rejected: "bad",
  superseded: "warn",
};

export interface QueueMeta {
  byId: ReadonlyMap<string, DecisionItem>;
  /** Fixture metadata is illustrative and always carries the PLACEHOLDER badge. */
  fixture: boolean;
}

function MetaLine({ approval, meta }: { approval: Approval; meta: QueueMeta }) {
  const item = meta.byId.get(metaKey(approval.id));
  const priority = item && item.priority !== "unknown" ? item.priority : null;
  const response = item?.latestResponseAt ?? null;
  if (!priority && !response) {
    return <span className="rd-row-meta">Priority and latest response time: not available</span>;
  }
  return (
    <span className="rd-row-meta">
      Priority: {priority ?? "not available"} · Latest response: {response ? formatResponseTime(response) : "not available"}{" "}
      {meta.fixture && <Badge tone="placeholder">PLACEHOLDER</Badge>}
    </span>
  );
}

export function DecisionQueue({ state, meta }: { state: ReviewState; meta: QueueMeta }) {
  const [tab, setTab] = useState<TabId>("actionable");
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const current = TABS.find((t) => t.id === tab) ?? TABS[0];
  const shown = filterByTab(tab, state.items);

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const index = TABS.findIndex((t) => t.id === tab);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % TABS.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = TABS.length - 1;
    else return;
    event.preventDefault();
    setTab(TABS[next].id);
    tabRefs.current[TABS[next].id]?.focus();
  }

  return (
    <section aria-labelledby="rd-queue-title" className="rd-card rd-queue-card">
      <header className="rd-card-head">
        <h2 id="rd-queue-title">Decision queue</h2>
        <Button variant="quiet" onClick={() => void state.refresh()} disabled={state.loading}>
          Refresh
        </Button>
      </header>

      <div role="tablist" aria-label="Decision filters" className="rd-tabs" onKeyDown={onKeyDown}>
        {TABS.map((t) => {
          const count = t.unsupported ? null : filterByTab(t.id, state.items).length;
          return (
            <button
              key={t.id}
              ref={(node) => {
                tabRefs.current[t.id] = node;
              }}
              type="button"
              role="tab"
              id={`rd-tab-${t.id}`}
              aria-selected={t.id === tab}
              aria-controls="rd-tabpanel"
              tabIndex={t.id === tab ? 0 : -1}
              className="rd-tab"
              onClick={() => setTab(t.id)}
            >
              {t.label}
              {count !== null && state.listState === "ready" ? ` (${count})` : ""}
            </button>
          );
        })}
      </div>

      <div role="tabpanel" id="rd-tabpanel" aria-labelledby={`rd-tab-${tab}`} className="rd-tabpanel">
        <p className="rd-note">{current.description}</p>
        {state.approval && !current.unsupported && !shown.some((a) => a.id === state.approval?.id) && (
          <p className="rd-note" role="note">
            The candidate shown on the right (approval {state.approval.id}) is not in this tab.
          </p>
        )}
        {current.unsupported ? (
          <EmptyState title="No separate verification decisions" description={current.unsupported} />
        ) : shown.length === 0 ? (
          state.listState === "ready" && <EmptyState title="Nothing in this tab" description={current.description} />
        ) : (
          <ul className="rd-rows" aria-label={`${current.label} decisions`}>
            {shown.map((approval) => {
              const selected = state.approval?.id === approval.id;
              return (
                <li key={approval.id}>
                  <button
                    type="button"
                    className="rd-row"
                    aria-current={selected ? "true" : undefined}
                    onClick={() => state.select(approval.id)}
                  >
                    <span className="rd-row-title">
                      Task {approval.action.task_id}
                      <Pill tone={STATE_TONE[approval.state.state]}>{STATE_LABEL[approval.state.state]}</Pill>
                    </span>
                    <span className="rd-row-sub">
                      {KIND_LABEL[approvalKind(approval)] ?? approvalKind(approval)} · approval {approval.id} · candidate{" "}
                      <code>{approval.action.candidate_commit.slice(0, 10)}</code>
                    </span>
                    <MetaLine approval={approval} meta={meta} />
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      {state.hidden.length > 0 && <NotReviewed approvals={state.hidden} />}
    </section>
  );
}

const NOT_REVIEWED_LIMIT = 10;

/** Pending approvals of other kinds are listed, not just counted, so none is silently dropped from the queue. */
function NotReviewed({ approvals }: { approvals: readonly Approval[] }) {
  const shown = approvals.slice(0, NOT_REVIEWED_LIMIT);
  return (
    <section aria-labelledby="rd-unreviewed-title" className="rd-unreviewed">
      <h3 id="rd-unreviewed-title">Not reviewed here ({approvals.length})</h3>
      <p className="rd-note">
        {approvals.length} non-delivery decision(s) are not reviewed here. This view cannot approve or reject them.
      </p>
      <ul className="rd-rows" aria-label="Decisions not reviewed here">
        {shown.map((approval) => (
          <li key={approval.id} className="rd-unreviewed-row">
            <span className="rd-row-title">
              Task {approval.action.task_id}
              <Pill tone="neutral">Not reviewed here</Pill>
            </span>
            <span className="rd-row-sub">
              {KIND_LABEL[approvalKind(approval)] ?? approvalKind(approval)} · approval {approval.id}
            </span>
          </li>
        ))}
      </ul>
      {approvals.length > shown.length && (
        <p className="rd-note">And {approvals.length - shown.length} more not listed.</p>
      )}
    </section>
  );
}
