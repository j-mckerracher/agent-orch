/** The composer shared by the workspace and the task window: pinned input, round send button, Enter to send. */
import { useId, type KeyboardEvent, type ReactNode } from "react";
import { Button } from "../theme";
import "./chat.css";

export interface ChatComposerProps {
  /** Accessible name of the form and the text box. */
  label: string;
  sendLabel: string;
  value: string;
  onChange: (value: string) => void;
  /** Runs from the send button and from Enter, and only while `canSend`. */
  onSend: () => void;
  canSend: boolean;
  busy?: boolean;
  placeholder?: string;
  /** Turns the text box off too. The workspace leaves it on, so a draft can be typed while sending is blocked. */
  disabled?: boolean;
  /** Why sending is off. Shown under the box and read as its description. */
  blocked: string | null;
  /** Controls left of the send button. */
  controls?: ReactNode;
  /** Notes between the control row and the blocked reason. */
  notes?: ReactNode;
}

export function ChatComposer({
  label,
  sendLabel,
  value,
  onChange,
  onSend,
  canSend,
  busy = false,
  placeholder,
  disabled,
  blocked,
  controls,
  notes,
}: ChatComposerProps) {
  const blockedId = useId();
  const describedBy = blocked ? blockedId : undefined;
  // Enter sends. Shift+Enter keeps the default newline; Ctrl/Cmd+Enter also sends. Enter that confirms an IME
  // composition is left to the IME. A send that is not allowed does nothing, and the newline is still
  // suppressed, so Enter never adds a line the user did not ask for.
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing || event.keyCode === 229) return;
    if (event.shiftKey && !event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    if (canSend) onSend();
  };
  return (
    <form
      className="ws-composer"
      aria-label={label}
      onSubmit={(event) => {
        event.preventDefault();
        if (canSend) onSend();
      }}
    >
      <textarea
        aria-label={label}
        rows={1}
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        aria-describedby={describedBy}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <div className="ws-composer-row">
        {controls}
        <Button type="submit" variant="primary" className="ws-send" disabled={!canSend} aria-label={sendLabel} aria-describedby={describedBy}>
          {busy ? "…" : "↑"}
        </Button>
      </div>
      {notes}
      {blocked && (
        <p id={blockedId} className="ws-small ws-blocked">
          Sending is disabled. {blocked}
        </p>
      )}
    </form>
  );
}
