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

  return { ref, onScroll, behind, jump };
}
