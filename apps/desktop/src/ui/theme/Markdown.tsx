/**
 * Message text as Markdown (GitHub flavour). Styles live in primitives.css under `.ui-md`.
 *
 * Safety contract: raw HTML stays literal text, links are shown but never followed (a followed link would navigate
 * the app's own window away), and images are never fetched; their alt text stands in.
 */
import { memo, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

// remark-breaks keeps single line breaks, which chat writers expect to see.
const PLUGINS = [remarkGfm, remarkBreaks];

// A message sits under the view's own h2, so its headings start at h3.
function heading(level: 3 | 4 | 5 | 6) {
  const Tag = `h${level}` as const;
  return function MarkdownHeading({ children }: { children?: ReactNode }) {
    return <Tag>{children}</Tag>;
  };
}

const COMPONENTS: Components = {
  a: ({ href, children }) => (
    <span className="ui-md-link" title={href}>
      {children}
    </span>
  ),
  img: ({ alt, src }) => <span>{alt || (typeof src === "string" ? src : "")}</span>,
  table: ({ children }) => (
    <div className="ui-md-table">
      <table>{children}</table>
    </div>
  ),
  h1: heading(3),
  h2: heading(4),
  h3: heading(5),
  h4: heading(6),
  h5: heading(6),
  h6: heading(6),
};

export interface MarkdownProps {
  text: string;
}

/** Memoised so a streaming transcript does not re-parse every earlier message. */
export const Markdown = memo(function Markdown({ text }: MarkdownProps) {
  return (
    <div className="ui-md">
      <ReactMarkdown remarkPlugins={PLUGINS} components={COMPONENTS}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
