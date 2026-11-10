// Decision behaviour and audit sections. Approve, reject, request changes and delivery live
// in the accepted review flow above; everything here is an action the daemon cannot perform
// today, shown disabled with the reason visible as text (not only in a tooltip).
import type { TaskRecord } from "../../../daemon/types";
import { Badge, Button, Disclosure } from "../../theme";

export const UNSUPPORTED_ACTIONS: ReadonlyArray<{ id: string; label: string; reason: string }> = [
  { id: "acknowledge", label: "Acknowledge", reason: "The daemon has no acknowledge decision; use Accept or Reject." },
  { id: "postpone", label: "Postpone", reason: "The daemon cannot postpone a decision; it stays pending until resolved." },
  { id: "dismiss", label: "Dismiss", reason: "The daemon cannot dismiss a decision; use Reject to refuse it." },
  { id: "escalate", label: "Escalate", reason: "Escalation is not defined for any source yet." },
  { id: "extra-checks", label: "Request additional checks", reason: "The daemon has no method to request more checks." },
  {
    id: "pr-authorization",
    label: "Review PR creation authorization",
    reason: "Pull request automation is deferred; approving never creates a pull request.",
  },
];

export const AUDIT_ACTIONS: ReadonlyArray<{ id: string; label: string; reason: string }> = [
  { id: "inspect-integration", label: "Inspect integration history", reason: "The daemon exposes no integration history." },
  { id: "plan-recovery", label: "Plan recovery", reason: "Recovery planning is not supported by the daemon." },
  { id: "export-audit", label: "Export audit records", reason: "The daemon has no audit export method." },
];

function DisabledAction({ id, label, reason }: { id: string; label: string; reason: string }) {
  const reasonId = `rd-reason-${id}`;
  return (
    <li className="rd-disabled-action">
      <Button disabled aria-describedby={reasonId}>
        {label}
      </Button>
      <span id={reasonId} className="rd-reason">
        {reason}
      </span>
    </li>
  );
}

export function DecisionBehavior({ task, fixture }: { task: TaskRecord | null; fixture: boolean }) {
  return (
    <div className="rd-behavior">
      <Disclosure summary="Decision behavior" className="rd-disclosure">
        <p className="rd-note">
          Accept, Reject and Request changes above are real daemon decisions. These actions are not available:
        </p>
        <ul className="rd-action-list" aria-label="Actions not available">
          {UNSUPPORTED_ACTIONS.map((action) => (
            <DisabledAction key={action.id} {...action} />
          ))}
        </ul>
      </Disclosure>
      <Disclosure summary="Integration, recovery and audit history" className="rd-disclosure">
        <h4 className="rd-subtitle">
          Task audit transitions {fixture && <Badge tone="placeholder">PLACEHOLDER DATA</Badge>}
        </h4>
        {task === null ? (
          <p className="rd-note">The task record is not available, so no transitions are listed.</p>
        ) : !task.events || task.events.length === 0 ? (
          <p className="rd-note">The daemon has recorded no transitions for this task.</p>
        ) : (
          <ol aria-label="Task audit transitions" className="rd-events">
            {task.events.map((event, index) => (
              <li key={`${index}-${event}`}>{event}</li>
            ))}
          </ol>
        )}
        <ul className="rd-action-list" aria-label="Audit actions not available">
          {AUDIT_ACTIONS.map((action) => (
            <DisabledAction key={action.id} {...action} />
          ))}
        </ul>
      </Disclosure>
    </div>
  );
}
