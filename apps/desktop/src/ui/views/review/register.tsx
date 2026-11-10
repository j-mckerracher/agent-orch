// Registration for the integrator: spread `reviewGlobalViews` into `<App globalViews={...} />`.
// The view is the `#/review` global route; the shell's `reviewCount` slot already has a default.
import type { GlobalViewRenderer } from "../../../App";
import type { GlobalView } from "../../router";
import { useServices } from "../../services";
import { ReviewDecisionsView } from "./ReviewDecisionsView";

/** Same rule as the S6 route: fixtures only with `?fixture=1` and no daemon bridge. */
export function ReviewGlobalView() {
  const { bridgeBound } = useServices();
  const fixtureMode = !bridgeBound && new URLSearchParams(window.location.search).get("fixture") === "1";
  return <ReviewDecisionsView fixtureMode={fixtureMode} />;
}

export const reviewGlobalViews: Partial<Record<GlobalView, GlobalViewRenderer>> = {
  review: () => <ReviewGlobalView />,
};
