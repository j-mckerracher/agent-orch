/** The scrolling message thread shared by the workspace and the task window: a labelled log that follows its newest line until the person scrolls up. */
import type { ReactNode } from "react";
import { useFollowScroll } from "../activity/useFollowScroll";
import "../activity/activity.css";
import "./chat.css";

export function ChatLog({ label, scrollKey, children }: { label: string; scrollKey: string | number; children: ReactNode }) {
  const { ref, onScroll, behind, jump } = useFollowScroll(scrollKey);
  return (
    <div ref={ref} onScroll={onScroll} className="ws-log" role="log" aria-label={label} tabIndex={0}>
      {children}
      {behind && (
        <button type="button" className="act-jump" onClick={jump}>
          Jump to latest
        </button>
      )}
    </div>
  );
}
