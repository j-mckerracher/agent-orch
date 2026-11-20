/** The scrolling message thread shared by the workspace and the task window: a labelled log that follows its newest message. */
import { useEffect, useRef, type ReactNode } from "react";
import "./chat.css";

export function ChatLog({ label, scrollKey, children }: { label: string; scrollKey: string | number; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  // `scrollKey` changes whenever the thread grows, so the log then jumps to its end.
  useEffect(() => {
    const log = ref.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [scrollKey]);
  return (
    <div ref={ref} className="ws-log" role="log" aria-label={label} tabIndex={0}>
      {children}
    </div>
  );
}
