import { formatAt } from "../views/workspace/time";

/** The time a chat turn was said, in the device's timezone. `at` is a daemon instant. */
export function Timestamp({ at }: { at: string }) {
  return (
    <time className="ws-time" dateTime={at}>
      {formatAt(at)}
    </time>
  );
}
