/** One tool call of an agent: a keyboard-expandable row with its state and the daemon's redacted result preview. */
import { memo, useId, useState } from "react";
import type { ToolItem } from "../../sources/surfaces/types";

const STATE_TEXT = { running: "Running", ok: "Finished", failed: "Failed", stopped: "Stopped" } as const;
const EMPTY_RESULT = {
  running: "The call has not finished.",
  stopped: "The call was stopped before it finished.",
  ok: "No result preview was recorded.",
  failed: "No result preview was recorded.",
} as const;

/** `stopped` is a call that never finished in a run that is no longer running: cancel, timeout, failure or a restart. */
export const ToolRow = memo(function ToolRow({ tool, stopped }: { tool: ToolItem; stopped: boolean }) {
  const [open, setOpen] = useState(false);
  const resultId = useId();
  const state = tool.state === "running" && stopped ? "stopped" : tool.state;
  return (
    <div className="act-tool" data-state={state}>
      <button type="button" className="act-tool-head" aria-expanded={open} aria-controls={resultId} onClick={() => setOpen(!open)}>
        <span className="act-mark" aria-hidden="true" />
        <span className="ui-visually-hidden">{STATE_TEXT[state]}: </span>
        <span className="act-tool-name">{tool.name}</span>
        {tool.summary !== "" && <span className="act-tool-summary">{tool.summary}</span>}
      </button>
      <pre id={resultId} className="act-tool-result" hidden={!open}>
        {tool.result === "" ? EMPTY_RESULT[state] : tool.result}
      </pre>
    </div>
  );
});
