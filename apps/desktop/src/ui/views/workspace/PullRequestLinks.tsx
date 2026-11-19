import type { PullRequestLink } from "../../../sources/surfaces/types";
import { CopyButton } from "../../theme";

export const PULL_REQUESTS_LABEL = "Pull requests";

/**
 * The recorded pull requests of a turn's tasks, under its reply (DPES-1108). The daemon builds each URL from its own
 * records, so the user never depends on the model copying one. The app does not follow links (see `Markdown`), so a
 * URL is plain text with a copy button.
 */
export function PullRequestLinks({ links }: { links: readonly PullRequestLink[] | undefined }) {
  if (!links || links.length === 0) return null;
  return (
    <ul className="ws-pr-links" aria-label={PULL_REQUESTS_LABEL}>
      {links.map((link) => (
        <li key={link.url} className="ws-pr-link">
          <span className="ws-pr-link-text">
            Pull request {link.number}: <code>{link.url}</code>
          </span>
          <CopyButton text={link.url} label={`Copy the link to pull request ${link.number}`} />
        </li>
      ))}
    </ul>
  );
}
