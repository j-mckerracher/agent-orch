/**
 * Reusable UI primitives. Styles live in primitives.css and read only tokens.css variables.
 *
 * Accessibility contract: every interactive primitive is a native element (`button`,
 * `details/summary`), so keyboard operation and focus order come from the platform, and the
 * shared `:focus-visible` rule draws the ring. State is never color-only: Pill carries a
 * glyph and text, Meter carries a numeric label.
 */
import { useEffect, useRef, useState, type ButtonHTMLAttributes, type CSSProperties, type HTMLAttributes, type ReactNode } from "react";

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
  /** Hide the redundant non-color glyph only when the text already states the state. */
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
  /**
   * The action is running: a spinner replaces the label (the width stays), and clicks are ignored. The button is not
   * `disabled`, so a person on the keyboard keeps focus on it; disable the other controls instead.
   */
  pending?: boolean;
}

export function Button({ variant = "secondary", type = "button", className, pending, onClick, ...rest }: ButtonProps) {
  return (
    <button
      type={type}
      className={cx("ui-btn", `ui-btn-${variant}`, className)}
      aria-busy={pending || undefined}
      aria-disabled={pending || undefined}
      onClick={pending ? undefined : onClick}
      {...rest}
    />
  );
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
  /** Controls at the end of the header, e.g. a CopyButton. */
  actions?: ReactNode;
}

export function Bubble({ from, author, time, actions, className, children, ...rest }: BubbleProps) {
  return (
    <article className={cx("ui-bubble", `ui-bubble-${from}`, className)} {...rest}>
      <header className="ui-bubble-meta">
        <b>{author}</b>
        {time !== undefined && time !== null && <span>{time}</span>}
        {actions && <span className="ui-bubble-actions">{actions}</span>}
      </header>
      <div className="ui-bubble-body">{children}</div>
    </article>
  );
}

/* CopyButton ---------------------------------------------------------------------------- */

export interface CopyButtonProps {
  /** The exact text put on the clipboard. */
  text: string;
  /** Accessible name; it stays the same while the result is shown. */
  label?: string;
  className?: string;
}

type CopyState = "idle" | "copied" | "failed";

const COPY_RESULT: Record<CopyState, string> = { idle: "", copied: "Copied", failed: "Copy failed" };
const COPY_RESET_MS = 2000;

function CopyIcon({ state }: { state: CopyState }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      aria-hidden="true"
      focusable="false"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {state === "copied" && <path d="M3 8.5 6.5 12 13 4.5" />}
      {state === "failed" && <path d="m4 4 8 8m0-8-8 8" />}
      {state === "idle" && (
        <>
          <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
          <path d="M10.5 3.5v-.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h.5" />
        </>
      )}
    </svg>
  );
}

/**
 * One click copies `text`. The icon turns into a check (copied) or a cross (failed) for two seconds, and a polite
 * live region says the same in words, so the result is never shape- or color-only.
 */
export function CopyButton({ text, label = "Copy message", className }: CopyButtonProps) {
  const [state, setState] = useState<CopyState>("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
    };
  }, []);
  const settle = (next: CopyState) => {
    if (!mounted.current) return;
    setState(next);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setState("idle"), COPY_RESET_MS);
  };
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      settle("copied");
    } catch {
      settle("failed");
    }
  };
  return (
    <span className={cx("ui-copy", className)} data-state={state}>
      <button
        type="button"
        className="ui-copy-button"
        aria-label={label}
        title={state === "idle" ? label : COPY_RESULT[state]}
        onClick={() => void copy()}
      >
        <CopyIcon state={state} />
      </button>
      {state !== "idle" && <span className="ui-copy-result">{COPY_RESULT[state]}</span>}
      <span className="ui-visually-hidden" aria-live="polite" aria-atomic="true">
        {COPY_RESULT[state]}
      </span>
    </span>
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
