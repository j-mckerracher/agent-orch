// View-level filter tab bar for the Review & Decisions queue. It sits above the two-column
// layout and selects the tab the queue list shows (OX-R2).
import { useRef, type KeyboardEvent } from "react";
import type { ReviewState } from "../../../review/useReview";
import { TABS, filterByTab, type TabId } from "./queue";

export function DecisionTabs({ state, tab, onTab }: { state: ReviewState; tab: TabId; onTab: (tab: TabId) => void }) {
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const index = TABS.findIndex((t) => t.id === tab);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % TABS.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = TABS.length - 1;
    else return;
    event.preventDefault();
    onTab(TABS[next].id);
    tabRefs.current[TABS[next].id]?.focus();
  }

  return (
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
            onClick={() => onTab(t.id)}
          >
            {t.label}
            {count !== null && state.listState === "ready" ? ` (${count})` : ""}
          </button>
        );
      })}
    </div>
  );
}
