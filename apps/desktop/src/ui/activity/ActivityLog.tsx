/** An agent's thinking and tool calls in the order they happened. Shared by the orchestrator chat and the task conversation. */
import { memo } from "react";
import type { ActivityItem } from "../../sources/surfaces/types";
import { ThinkingBlock } from "../views/workspace/ThinkingBlock";
import { ToolRow } from "./ToolRow";
import "./activity.css";

/** The newest tool rows drawn; a run with more says how many earlier ones are left out. */
export const MAX_TOOL_ROWS = 200;

/**
 * `streaming`: the newest item can still grow. `active`: the turn or session that made these items is still running, so
 * a call with no finish yet is in progress; otherwise it never finished and shows as stopped. They differ in a chat run
 * that a reply followed: it stopped streaming, but its tool call can still finish while the turn runs. `at` is when the
 * chat turn was recorded and is shown on the first thinking block; session and task activity carry no time, so it is left out.
 */
export const ActivityLog = memo(function ActivityLog({
  items,
  streaming,
  active,
  at,
}: {
  items: readonly ActivityItem[];
  streaming: boolean;
  active: boolean;
  at?: string;
}) {
  const toolCount = items.reduce((count, item) => count + (item.kind === "tool" ? 1 : 0), 0);
  let skip = Math.max(0, toolCount - MAX_TOOL_ROWS);
  const hidden = skip;
  const firstThinking = items.findIndex((item) => item.kind === "thinking");
  return (
    <div className="act-log">
      {hidden > 0 && <p className="ws-small">{hidden} earlier tool {hidden === 1 ? "call" : "calls"}</p>}
      {items.map((item, index) => {
        switch (item.kind) {
          case "thinking":
            // Only the newest item can still be streaming; thinking before a tool call is done.
            return <ThinkingBlock key={item.key} text={item.text} streaming={streaming && index === items.length - 1} at={index === firstThinking ? at : undefined} />;
          case "tool":
            if (skip > 0) {
              skip--;
              return null;
            }
            return <ToolRow key={item.key} tool={item} stopped={!active} />;
          case "cap":
            return (
              <p key={item.key} className="ws-small">
                The daemon stopped recording tool calls here. Later calls are not shown.
              </p>
            );
        }
      })}
    </div>
  );
});
