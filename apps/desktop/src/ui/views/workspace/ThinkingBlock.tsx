/** A turn's agent thinking, in full: open while it streams, then collapsed unless the person toggled it. */
import { useState } from "react";
import { Timestamp } from "../../chat/Timestamp";
import { Badge, CopyButton, Disclosure, Markdown } from "../../theme";

/** `at` is when the turn was recorded; a session's thinking carries no time of its own, so it is left out there. */
export function ThinkingBlock({ text, streaming, at }: { text: string; streaming: boolean; at?: string }) {
  // null until the person toggles; from then on their choice wins over the streaming default.
  const [chosen, setChosen] = useState<boolean | null>(null);
  const open = chosen ?? streaming;
  return (
    <Disclosure
      className="ws-thinking"
      summary={
        <>
          Thinking
          {streaming && (
            <>
              {" "}
              <Badge tone="info">Streaming</Badge>
            </>
          )}
          {at !== undefined && (
            <>
              {" "}
              <Timestamp at={at} />
            </>
          )}
        </>
      }
      defaultOpen={open}
      // A programmatic open/close fires toggle too; only a change the page did not ask for is the person's.
      onToggle={(event) => event.currentTarget.open !== open && setChosen(event.currentTarget.open)}
    >
      <Markdown text={text} />
      <CopyButton text={text} label="Copy thinking" />
    </Disclosure>
  );
}
