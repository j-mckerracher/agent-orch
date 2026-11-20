import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Button } from "../../theme";
import { Dialog } from "../../Dialog";
import { usePopover } from "../../usePopover";
import type { TargetOption } from "./targets";
import "./ConversationActions.css";

export interface ConversationActionsProps {
  option: TargetOption;
  /** Live daemon. Fixture mode simulates the actions and labels them as placeholder data. */
  live: boolean;
  connected: boolean;
  /** The daemon negotiated `conversation_history`; an older daemon does not. */
  supported: boolean;
  /** A reply to this conversation is running; clearing is refused until it ends. */
  running: boolean;
  /** Another action on this conversation is in flight. */
  busy: boolean;
  onClear: () => void;
  /** Present for project coordinators only. */
  onRemove?: () => void;
  /** A workstream conversation has no history actions: its menu holds only `children`. */
  history?: boolean;
  /** Scope, monitoring and coordinator controls, shown above the actions in the menu. */
  children?: ReactNode;
}

/**
 * Why a history action cannot run now, or null when it can. The menu shows the reason next to
 * the action, so a disabled control always says what would enable it.
 */
export function historyActionReason(
  state: Pick<ConversationActionsProps, "live" | "connected" | "supported" | "running" | "busy">,
): string | null {
  if (!state.live) return null;
  if (!state.connected) return "Offline: reconnect to the daemon to change history.";
  if (!state.supported) return "This daemon is older and cannot change history. Update the daemon to use this action.";
  if (state.running) return "A reply is still running. Wait for it to finish or cancel it first.";
  if (state.busy) return "Another action on this conversation is still running.";
  return null;
}

/**
 * The conversation header's "⋯" menu (DPES-1105): the conversation's scope, monitoring and coordinator controls
 * (`children`), "Clear history…" for every Global or project conversation, and "Remove coordinator…" for a project
 * coordinator. Each action opens a confirmation dialog that names the conversation. Escape closes the menu and
 * returns focus to its button; arrow keys move between the actions.
 */
export function ConversationActions({
  option,
  live,
  connected,
  supported,
  running,
  busy,
  onClear,
  onRemove,
  history = true,
  children,
}: ConversationActionsProps) {
  const ids = useId();
  const menuId = `${ids}-menu`;
  const reasonId = `${ids}-reason`;
  const panelRef = useRef<HTMLDivElement>(null);
  const popover = usePopover(() =>
    panelRef.current?.querySelector<HTMLElement>('[role="menuitem"], button, input, a[href]')?.focus(),
  );
  const [dialog, setDialog] = useState<"clear" | "remove" | null>(null);
  const reason = historyActionReason({ live, connected, supported, running, busy });

  const choose = (action: "clear" | "remove") => {
    if (reason !== null) return;
    // Focus the trigger before the dialog opens, so the dialog returns focus to it on close.
    popover.setOpen(false);
    popover.buttonRef.current?.focus();
    setDialog(action);
  };

  const moveFocus = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
    if (items.length === 0) return;
    event.preventDefault();
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : event.key === "ArrowDown"
            ? (index + 1) % items.length
            : (index - 1 + items.length) % items.length;
    items[next].focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    popover.onKeyDown(event);
    moveFocus(event);
  };

  const fixtureNote = live ? null : (
    <p>
      <strong>PLACEHOLDER DATA.</strong> Nothing is deleted. The placeholder transcript is hidden for this session only.
    </p>
  );

  return (
    <>
      <div ref={popover.rootRef} className="ws-actions" onKeyDown={onKeyDown} onBlur={popover.onBlur}>
        <button
          ref={popover.buttonRef}
          type="button"
          className="ws-actions-trigger"
          aria-haspopup="menu"
          aria-expanded={popover.open}
          aria-controls={menuId}
          aria-label={`Conversation actions for ${option.label}`}
          onClick={() => popover.setOpen((value) => !value)}
        >
          <span aria-hidden="true">⋯</span>
        </button>
        <div ref={panelRef} id={menuId} className="ws-actions-menu ws-actions-panel" hidden={!popover.open}>
          {children}
          {history && (
            <div role="menu" aria-label={`Actions for ${option.label}`} className="ws-actions-list">
              <button
                type="button"
                role="menuitem"
                className="ws-actions-item"
                aria-disabled={reason !== null || undefined}
                aria-describedby={reason !== null ? reasonId : undefined}
                onClick={() => choose("clear")}
              >
                Clear history…
              </button>
              {option.target.kind === "project" && onRemove && (
                <button
                  type="button"
                  role="menuitem"
                  className="ws-actions-item"
                  aria-disabled={reason !== null || undefined}
                  aria-describedby={reason !== null ? reasonId : undefined}
                  onClick={() => choose("remove")}
                >
                  Remove coordinator…
                </button>
              )}
              {reason !== null && (
                <p id={reasonId} className="ws-actions-reason">
                  {reason}
                </p>
              )}
            </div>
          )}
        </div>
      </div>

      {dialog === "clear" && (
        <Dialog
          title={`Clear history for ${option.label}?`}
          onClose={() => setDialog(null)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setDialog(null)}>
                Cancel
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setDialog(null);
                  onClear();
                }}
              >
                Clear history permanently
              </Button>
            </>
          }
        >
          <p>
            This permanently deletes every message and reply in this conversation. It cannot be undone.
          </p>
          <p>Other conversations, the project and its tasks are not changed. Drafts you have not sent are kept.</p>
          {fixtureNote}
        </Dialog>
      )}

      {dialog === "remove" && onRemove && (
        <Dialog
          title={`Remove ${option.label}?`}
          onClose={() => setDialog(null)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setDialog(null)}>
                Cancel
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setDialog(null);
                  onRemove();
                }}
              >
                Remove coordinator permanently
              </Button>
            </>
          }
        >
          <p>
            This permanently deletes every message and reply in its conversation. It cannot be undone. The coordinator
            leaves the conversation targets.
          </p>
          <p>
            The project, its tasks, the coordinator&apos;s scope sentence and the project&apos;s preferences and learnings are
            kept. You can add the coordinator back later; it starts with an empty conversation.
          </p>
          {fixtureNote}
        </Dialog>
      )}
    </>
  );
}

export interface AddCoordinatorDialogProps {
  /** Projects without a coordinator: never added, or removed. */
  projects: readonly { id: string; label: string }[];
  /** The project being added, while its request is in flight. */
  busyId: string | null;
  onAdd: (projectId: string) => void;
  onClose: () => void;
}

/** Lists projects without a coordinator, with an "Add" action each (DPES-1105). An added coordinator starts with an empty conversation. */
export function AddCoordinatorDialog({ projects, busyId, onAdd, onClose }: AddCoordinatorDialogProps) {
  return (
    <Dialog
      title="Add a coordinator"
      onClose={onClose}
      footer={
        <Button variant="secondary" onClick={onClose}>
          Close
        </Button>
      }
    >
      {projects.length === 0 ? (
        <p>Every project already has an active coordinator.</p>
      ) : (
        <ul className="ws-add-list" aria-label="Projects without a coordinator">
          {projects.map((project) => (
            <li key={project.id}>
              <span>{project.label}</span>
              <Button
                variant="secondary"
                disabled={busyId !== null}
                aria-label={`Add ${project.label}`}
                onClick={() => onAdd(project.id)}
              >
                {busyId === project.id ? "Adding…" : "Add"}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <p>
        An added coordinator starts with an empty conversation. One you add back keeps its scope sentence and the
        project&apos;s preferences and learnings. The project and its tasks are not changed.
      </p>
    </Dialog>
  );
}
