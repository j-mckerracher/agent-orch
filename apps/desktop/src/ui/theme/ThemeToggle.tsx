import { useTheme } from "./theme";

/** Light/dark switch. `aria-pressed` is true while dark is showing; the choice persists. */
export function ThemeToggle({ className }: { className?: string }) {
  const { resolved, persisted, toggle } = useTheme();
  const dark = resolved === "dark";
  return (
    <button
      type="button"
      className={["ui-theme-toggle", className].filter(Boolean).join(" ")}
      aria-pressed={dark}
      onClick={toggle}
    >
      <span aria-hidden="true">{dark ? "☾" : "☀"}</span>
      <span>Dark mode</span>
      <span className="ui-theme-state">
        {dark ? "On" : "Off"}
        {persisted ? "" : " (this session)"}
      </span>
    </button>
  );
}
