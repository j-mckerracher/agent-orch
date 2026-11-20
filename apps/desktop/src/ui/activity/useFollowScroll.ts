/** Keeps a scrolling log at its newest line until the person scrolls up; `behind` then says newer lines wait below. */
import { useCallback, useEffect, useRef, useState } from "react";

/** Within this many pixels of the end still counts as at the end. */
const AT_END_PX = 24;

export function useFollowScroll(scrollKey: string | number) {
  const ref = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [behind, setBehind] = useState(false);

  const onScroll = useCallback(() => {
    const log = ref.current;
    if (!log) return;
    following.current = log.scrollHeight - log.scrollTop - log.clientHeight <= AT_END_PX;
    if (following.current) setBehind(false);
  }, []);

  // `scrollKey` changes whenever the log grows.
  useEffect(() => {
    const log = ref.current;
    if (!log) return;
    if (following.current) log.scrollTop = log.scrollHeight;
    else setBehind(true);
  }, [scrollKey]);

  const jump = useCallback(() => {
    const log = ref.current;
    if (log) log.scrollTop = log.scrollHeight;
    following.current = true;
    setBehind(false);
  }, []);

  // A log in a hidden tab panel has no height and loses its scroll position; showing it again lands on the newest line.
  useEffect(() => {
    const log = ref.current;
    if (!log || typeof ResizeObserver === "undefined") return;
    let height = log.clientHeight;
    const observer = new ResizeObserver(() => {
      const shown = height === 0 && log.clientHeight > 0;
      height = log.clientHeight;
      if (shown) jump();
      else if (following.current) log.scrollTop = log.scrollHeight;
    });
    observer.observe(log);
    return () => observer.disconnect();
  }, [jump]);

  return { ref, onScroll, behind, jump };
}
