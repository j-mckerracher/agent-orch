import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { mountAmbientBackground, type AmbientOptions } from "./engine";
import { ambientSettings, useAmbientSettings, type AmbientSettings } from "./settings";
import agentNativeLicense from "./vendor/LICENSE.agent-native?raw";
import vgpuLicense from "./vendor/LICENSE.vgpu?raw";
import "./background.css";

export interface AmbientBackgroundProps extends AmbientOptions {
  settings?: AmbientSettings;
}

/**
 * The fixed, decorative layer under the whole interface. Renders nothing (and loads no
 * renderer code) until the user opts in. It is portalled to `<body>` so it can sit below
 * every in-flow element regardless of where the integrator mounts it. `data-ambient-background`
 * on `<html>` lets `background.css` make the shell's page background see-through only
 * while the layer is on.
 */
export function AmbientBackground({ settings, ...options }: AmbientBackgroundProps) {
  const { enabled } = useAmbientSettings(settings ?? ambientSettings());
  const hostRef = useRef<HTMLDivElement | null>(null);
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    const host = hostRef.current;
    if (!enabled || !host) return;
    const root = document.documentElement;
    root.setAttribute("data-ambient-background", "on");
    const controller = mountAmbientBackground(host, optionsRef.current);
    return () => {
      controller.destroy();
      root.removeAttribute("data-ambient-background");
    };
  }, [enabled]);

  if (!enabled) return null;
  return createPortal(
    <div ref={hostRef} className="ambient-background" aria-hidden="true" data-testid="ambient-background" />,
    document.body,
  );
}

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() => matchMediaOrNull()?.matches ?? false);
  useEffect(() => {
    const query = matchMediaOrNull();
    if (!query) return;
    const onChange = () => setReduced(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);
  return reduced;
}

function matchMediaOrNull(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia("(prefers-reduced-motion: reduce)") : null;
}

/** Opt-in switch plus the attribution that MIT licensing of the bundled renderer requires. */
export function AmbientBackgroundControl({ settings, className }: { settings?: AmbientSettings; className?: string }) {
  const { enabled, persisted, setEnabled } = useAmbientSettings(settings ?? ambientSettings());
  const reduced = useReducedMotion();
  return (
    <div className={["ambient-control", className].filter(Boolean).join(" ")} data-testid="ambient-control">
      <button
        type="button"
        className="ui-theme-toggle"
        aria-pressed={enabled}
        aria-describedby="ambient-note"
        onClick={() => setEnabled(!enabled)}
      >
        <span aria-hidden="true">≈</span>
        <span>Background</span>
        <span className="ui-theme-state">
          {enabled ? "On" : "Off"}
          {persisted ? "" : " (this session)"}
        </span>
      </button>
      <p id="ambient-note" className="ambient-note">
        Decorative only; it does not show activity.
        {reduced ? " Reduced motion is on, so the pattern stays still." : ""}
      </p>
      <details className="ambient-licences">
        <summary>Animation licences</summary>
        <p className="ambient-note">
          The wave uses the MIT-licensed ocean renderer from Agent-Native (Builder.io) and the vgpu library (Vercel).
        </p>
        <pre tabIndex={0} aria-label="Agent-Native licence">{agentNativeLicense}</pre>
        <pre tabIndex={0} aria-label="vgpu licence">{vgpuLicense}</pre>
      </details>
    </div>
  );
}
