// Pure queue logic for the Review & Decisions view: tabs, filtering, and the join of
// daemon approvals with the D16 metadata (priority, latest response time).
import type { Approval } from "../../../daemon/types";
import type { DecisionItem } from "../../../sources/surfaces/types";

export type TabId = "actionable" | "approvals" | "verification" | "candidates" | "delivery" | "resolved";

export interface TabDef {
  id: TabId;
  label: string;
  /** Shown above the list so the user knows exactly what the tab selects. */
  description: string;
  /** When set, the daemon has no data for this tab and it explains why instead of listing items. */
  unsupported?: string;
}

export const TABS: readonly TabDef[] = [
  { id: "actionable", label: "Actionable", description: "Approvals waiting for your decision." },
  { id: "approvals", label: "Approvals", description: "Every approval the daemon lists, plus those you resolved in this session." },
  {
    id: "verification",
    label: "Verification",
    description: "Verification decisions.",
    unsupported:
      "The daemon has no separate verification decision. Each candidate's automated-check outcome is shown in its six separate facts.",
  },
  { id: "candidates", label: "Candidates", description: "Approvals that merge or open a candidate (local review, pull request)." },
  { id: "delivery", label: "Delivery", description: "Approved this session: delivery is a separate request and has not been made by approving." },
  { id: "resolved", label: "Resolved", description: "Approved, rejected or superseded in this session. The daemon lists pending approvals only." },
];

export function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

export function matchesTab(tab: TabId, approval: Approval): boolean {
  const state = approval.state.state;
  switch (tab) {
    case "actionable":
      return state === "requested";
    case "approvals":
      return true;
    case "verification":
      return false;
    case "candidates":
      return approvalKind(approval) !== "ReportPublish";
    case "delivery":
      return state === "approved";
    case "resolved":
      return state !== "requested";
  }
}

export function filterByTab(tab: TabId, items: readonly Approval[]): Approval[] {
  return items.filter((item) => matchesTab(tab, item));
}

export const STATE_LABEL: Record<Approval["state"]["state"], string> = {
  requested: "Awaiting decision",
  approved: "Approved",
  rejected: "Rejected",
  superseded: "Superseded",
};

export const KIND_LABEL: Record<string, string> = {
  LocalReviewMerge: "Local review merge",
  PullRequestCreate: "Pull request creation",
  ReportPublish: "Report publication",
  policy_change: "Policy change",
};

/** Fixture ids differ between the two fixture sets (`fixture:x` vs `fixture-x`); the join ignores the prefix. */
export function metaKey(id: string): string {
  return id.replace(/^fixture[:-]/, "");
}

export function indexMeta(items: readonly DecisionItem[]): Map<string, DecisionItem> {
  return new Map(items.map((item) => [metaKey(item.id), item]));
}

export function formatResponseTime(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return `${String(at.getUTCHours()).padStart(2, "0")}:${String(at.getUTCMinutes()).padStart(2, "0")} UTC`;
}
