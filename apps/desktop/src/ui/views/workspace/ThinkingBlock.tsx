/** A turn's agent thinking, in full: open while it streams, then collapsed unless the person toggled it. */
import { useState } from "react";
import { Badge, CopyButton, Disclosure, Markdown } from "../../theme";

export function ThinkingBlock({ text, streaming }: { text: string; streaming: boolean }) {
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
