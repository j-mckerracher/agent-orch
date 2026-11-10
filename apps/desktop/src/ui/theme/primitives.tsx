/**
 * Reusable UI primitives. Styles live in primitives.css and read only tokens.css variables.
 *
 * Accessibility contract: every interactive primitive is a native element (`button`,
 * `details/summary`), so keyboard operation and focus order come from the platform, and the
 * shared `:focus-visible` rule draws the ring. State is never colour-only: Pill carries a
 * glyph and text, Meter carries a numeric label.
 */
import type { ButtonHTMLAttributes, CSSProperties, HTMLAttributes, ReactNode } from "react";

function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

/* Disclosure ---------------------------------------------------------------------------- */

export interface DisclosureProps extends Omit<HTMLAttributes<HTMLDetailsElement>, "title"> {
  summary: ReactNode;
  /** Short trailing count or hint shown on the summary row. */
  count?: ReactNode;
  defaultOpen?: boolean;
}

/** Native `<details>` disclosure: Enter/Space toggles, no script state to lose. */
export function Disclosure({ summary, count, defaultOpen, className, children, ...rest }: DisclosureProps) {
  return (
    <details className={cx("ui-disclosure", className)} open={defaultOpen} {...rest}>
      <summary>
        <span className="ui-disclosure-title">{summary}</span>
        {count !== undefined && count !== null && <span className="ui-disclosure-count">{count}</span>}
      </summary>
      <div className="ui-disclosure-body">{children}</div>
    </details>
  );
}

/* Pill ---------------------------------------------------------------------------------- */

export type Tone = "neutral" | "info" | "ok" | "warn" | "bad";

const TONE_MARK: Record<Tone, string> = { neutral: "–", info: "i", ok: "✓", warn: "!", bad: "✕" };

export interface PillProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  /** Hide the redundant non-colour glyph only when the text already states the state. */
  hideMark?: boolean;
}

export function Pill({ tone = "neutral", hideMark, className, children, ...rest }: PillProps) {
  return (
    <span className={cx("ui-pill", `ui-tone-${tone}`, className)} {...rest}>
      {!hideMark && (
        <span className="ui-pill-mark" aria-hidden="true">
          {TONE_MARK[tone]}
        </span>
      )}
      {children}
    </span>
  );
}

/* Badge --------------------------------------------------------------------------------- */

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  /** `placeholder` marks fixture or illustrative content (dashed outline). */
  tone?: Tone | "placeholder";
}

export function Badge({ tone = "neutral", className, children, ...rest }: BadgeProps) {
  return (
    <span className={cx("ui-badge", `ui-tone-${tone}`, className)} {...rest}>
      {children}
    </span>
  );
}

/* Button -------------------------------------------------------------------------------- */

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "danger" | "quiet";
}

export function Button({ variant = "secondary", type = "button", className, ...rest }: ButtonProps) {
  return <button type={type} className={cx("ui-btn", `ui-btn-${variant}`, className)} {...rest} />;
}

/* Panel --------------------------------------------------------------------------------- */

export interface PanelProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  title?: ReactNode;
  headingLevel?: 2 | 3 | 4;
  /** Content aligned to the right of the title (counts, actions). */
  actions?: ReactNode;
}

export function Panel({ title, headingLevel = 3, actions, className, children, ...rest }: PanelProps) {
  const Heading = `h${headingLevel}` as const;
  return (
    <section className={cx("ui-panel", className)} {...rest}>
      {(title || actions) && (
        <header className="ui-panel-header">
          {title && <Heading className="ui-panel-title">{title}</Heading>}
          {actions && <div className="ui-panel-actions">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}

/* Bubble -------------------------------------------------------------------------------- */

export interface BubbleProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  from: "coordinator" | "human";
  /** Visible speaker label, e.g. "Global orchestrator". */
  author: ReactNode;
  time?: ReactNode;
}

export function Bubble({ from, author, time, className, children, ...rest }: BubbleProps) {
  return (
    <article className={cx("ui-bubble", `ui-bubble-${from}`, className)} {...rest}>
      <header className="ui-bubble-meta">
        <b>{author}</b>
        {time !== undefined && time !== null && <span>{time}</span>}
      </header>
      <div className="ui-bubble-body">{children}</div>
    </article>
  );
}

/* EmptyState ---------------------------------------------------------------------------- */

export interface EmptyStateProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  title: ReactNode;
  description?: ReactNode;
  /** Optional next action, typically a Button. */
  action?: ReactNode;
}

export function EmptyState({ title, description, action, className, ...rest }: EmptyStateProps) {
  return (
    <div className={cx("ui-empty", className)} {...rest}>
      <p className="ui-empty-title">{title}</p>
      {description && <p className="ui-empty-description">{description}</p>}
      {action && <div className="ui-empty-action">{action}</div>}
    </div>
  );
}

/* Meter --------------------------------------------------------------------------------- */

export interface MeterProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  label: string;
  /** `null` or `undefined` means the value is unavailable; it is never drawn as zero. */
  value: number | null | undefined;
  min?: number;
  max?: number;
  /** Visible value text, e.g. "42%". Defaults to `${value}`. Also used as `aria-valuetext`. */
  valueText?: string;
}

export function Meter({ label, value, min = 0, max = 100, valueText, className, ...rest }: MeterProps) {
  if (value === null || value === undefined || !Number.isFinite(value) || max <= min) {
    return (
      <div className={cx("ui-meter", "ui-meter-unavailable", className)} {...rest}>
        <span className="ui-meter-label">{label}</span>
        <span className="ui-meter-track" aria-hidden="true" />
        <span className="ui-meter-value">unavailable</span>
      </div>
    );
  }
  const clamped = Math.min(max, Math.max(min, value));
  const level = ((clamped - min) / (max - min)) * 100;
  const text = valueText ?? `${clamped}`;
  return (
    <div
      className={cx("ui-meter", className)}
      role="meter"
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={clamped}
      aria-valuetext={text}
      {...rest}
    >
      <span className="ui-meter-label" aria-hidden="true">
        {label}
      </span>
      <span className="ui-meter-track" aria-hidden="true" style={{ "--level": `${level}%` } as CSSProperties} />
      <span className="ui-meter-value" aria-hidden="true">
        {text}
      </span>
    </div>
  );
}
