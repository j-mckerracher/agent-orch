import { decisionCountText, pendingDecisionCount } from "../format";
import { heldSets } from "../../sources/surfaces/holds";
import { useGate, useServices, useStore } from "../services";
import { Pill } from "../theme";
import { NavLink } from "./NavLink";
import { TopSearch } from "./Search";
import { UsagePill } from "./UsagePill";
import type { ShellSlots } from "./slots";
import "./topbar.css";

/**
 * The one "N need you" count, a link to Review. It is the pending approvals plus pending proposed actions, the same
 * lists as the Review nav badge, so the two cannot drift. A list nobody has read is "—", never 0: loading while a
 * daemon answer can still arrive, otherwise not available. "N+" when the daemon held more proposed actions than it sent.
 */
export function NeedYouCount() {
  const { data } = useServices();
  const { approvals, proposals, holds } = useStore(data);
  const gate = useGate();
  const link = { name: "global", view: "review" } as const;
  if (approvals.data && proposals.data) {
    const count = pendingDecisionCount(approvals.data, proposals.data, heldSets(holds.data ?? []));
    return (
      <p className="top-need" data-testid="top-need-you">
        <NavLink to={link}>
          <Pill tone={count > 0 ? "warn" : "neutral"} hideMark>
            {decisionCountText(count, proposals.more)} need you
          </Pill>
        </NavLink>
      </p>
    );
  }
  const failed = approvals.data ? proposals.status === "error" : approvals.status === "error";
  const loading = !failed && (gate.connected || gate.settling);
  return (
    <p className="top-need" data-testid="top-need-you">
      <NavLink to={link}>
        <Pill tone="neutral" hideMark>
          <span aria-hidden="true">— need you</span>
          <span className="top-sr">need you: {loading ? "loading" : "not available"}</span>
        </Pill>
      </NavLink>
    </p>
  );
}

/** The three top-bar slots; pass as `shellSlots` to `<App>` (see REGISTRATION.md). */
export const topBarSlots: Pick<ShellSlots, "topUsage" | "topSearch" | "topNeedYou"> = {
  topUsage: <UsagePill />,
  topSearch: <TopSearch />,
  topNeedYou: <NeedYouCount />,
};

export { TopSearch, UsagePill };
