import { Dialog } from "../../Dialog";
import { Button } from "../../theme";
import { FULL_AGENT_TEXT, FULL_REVIEW_TEXT, FULL_SAVE_TEXT, FULL_SUMMARY, agentTexts } from "./permissionLevel";

/** The confirmation before Full applies. The task level and the chat mode both use it, so they say the same thing. */
export function FullConfirmDialog({
  title,
  agent,
  onCancel,
  onConfirm,
}: {
  title: string;
  /** The agent the message names, or undefined when the daemon routes it. */
  agent: string | undefined;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" onClick={onConfirm}>
            Use Full
          </Button>
        </>
      }
    >
      <p>{FULL_SUMMARY}</p>
      {agentTexts(FULL_AGENT_TEXT, agent).map(({ agent: id, text }) => (
        <p key={id}>{text}</p>
      ))}
      <p>{FULL_REVIEW_TEXT}</p>
      <p>{FULL_SAVE_TEXT}</p>
    </Dialog>
  );
}
