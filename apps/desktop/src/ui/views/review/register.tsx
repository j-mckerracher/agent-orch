// Registration for the integrator: spread `reviewGlobalViews` into `<App globalViews={...} />`.
// The view is the `#/review` global route; the shell's `reviewCount` slot already has a default.
import type { GlobalViewRenderer } from "../../../App";
import type { GlobalView } from "../../router";
import { useServices } from "../../services";
import { ReviewDecisionsView } from "./ReviewDecisionsView";

/** Same rule as the S6 route: fixtures only with `?fixture=1` and no daemon bridge. */
export function ReviewGlobalView() {
  const { bridgeBound } = useServices();
  const params = new URLSearchParams(window.location.search);
  const fixtureMode = !bridgeBound && params.get("fixture") === "1";
  return (
    <ReviewDecisionsView
      fixtureMode={fixtureMode}
      evidenceScenario={params.get("evidence") ?? undefined}
      changesScenario={params.get("changes") ?? undefined}
    />
  );
}

export const reviewGlobalViews: Partial<Record<GlobalView, GlobalViewRenderer>> = {
  review: () => <ReviewGlobalView />,
};
