import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

export interface WsTab {
  id: string;
  label: string;
  content: ReactNode;
}

/**
 * Tablist for one workstream's detail (OX-W3). Roving tab stop with arrow, Home and End keys,
 * as in the decision filters. Inactive panels stay mounted and hidden, so their text is kept.
 */
export function WsTabs({ label, tabs, actions }: { label: string; tabs: readonly WsTab[]; actions?: ReactNode }) {
  const baseId = useId();
  const [active, setActive] = useState<string | undefined>(tabs[0]?.id);
  const refs = useRef(new Map<string, HTMLButtonElement>());
  const current = tabs.some((tab) => tab.id === active) ? active : tabs[0]?.id;

  const select = (id: string) => {
    setActive(id);
    refs.current.get(id)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = tabs.findIndex((tab) => tab.id === current);
    let next: number;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else return;
    event.preventDefault();
    const target = tabs[next];
    if (target) select(target.id);
  };

  return (
    <div className="ws-tabs">
      <div className="ws-tablist-row">
        <div role="tablist" aria-label={label} className="ws-tablist" onKeyDown={onKeyDown}>
          {tabs.map((tab) => (
            <button
              key={tab.id}
              ref={(node) => {
                if (node) refs.current.set(tab.id, node);
                else refs.current.delete(tab.id);
              }}
              type="button"
              role="tab"
              id={`${baseId}-tab-${tab.id}`}
              aria-selected={tab.id === current}
              aria-controls={`${baseId}-panel-${tab.id}`}
              tabIndex={tab.id === current ? 0 : -1}
              className="ws-tab"
              onClick={() => setActive(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
        {actions}
      </div>
      {tabs.map((tab) => (
        <div
          key={tab.id}
          role="tabpanel"
          id={`${baseId}-panel-${tab.id}`}
          aria-labelledby={`${baseId}-tab-${tab.id}`}
          hidden={tab.id !== current}
          className="ws-tabpanel"
        >
          {tab.content}
        </div>
      ))}
    </div>
  );
}
