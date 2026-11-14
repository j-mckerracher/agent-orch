import { useRef } from "react";
import type { SurfaceState } from "../../services";

/** Keeps showing the last settled result of `scope` while a refresh of the same scope loads. */
export function useRetained<T>(state: SurfaceState<T>, scope: string): SurfaceState<T> {
  const last = useRef<{ scope: string; state: SurfaceState<T> } | null>(null);
  if (state.status !== "loading") last.current = { scope, state };
  return state.status === "loading" && last.current?.scope === scope ? last.current.state : state;
}
