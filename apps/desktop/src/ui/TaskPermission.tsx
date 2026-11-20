/** A task's permission level badge and its "worker lacked tools" note (DPES-1126). Each renders nothing without data. */
import type { ToolsNoticeWire } from "../daemon/types";
import type { PermissionLevel } from "../sources/surfaces/types";
import { Pill } from "./theme";
import { permissionLabel } from "./views/workspace/permissionLevel";

/** The level a task ran with. Nothing for a preview task, which has no level. */
export function TaskPermissionBadge({ level }: { level: PermissionLevel | null }) {
  if (level === null) return null;
  return <Pill title="Permission level">{permissionLabel(level)}</Pill>;
}

/** Sits beside the outcome, so a Succeeded task that lacked tools does not look clean. */
export function ToolsNotice({ notice }: { notice: ToolsNoticeWire | null | undefined }) {
  if (!notice) return null;
  return (
    <Pill tone="warn" role="status">
      Worker lacked tools: {notice.tools.join(", ")} ({notice.reason})
    </Pill>
  );
}
