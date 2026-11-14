import type { ReactNode } from "react";
import { AmbientBackground, AmbientBackgroundControl } from "./AmbientBackground";

/**
 * Wiring points for the integrator (this item does not edit `App.tsx` or `shell/slots.ts`):
 *
 * - Render `layer` once anywhere inside the app. It portals to `<body>` and renders nothing
 *   while the animation is off, so it is safe to mount unconditionally.
 * - Render `control` where the user can reach it. `withAmbientBackground(themeToggle)` is
 *   the shortest wiring: pass it as the shell's `themeToggle` slot to place the switch and
 *   the layer together under the existing theme toggle.
 */
export const ambientBackgroundRegistration = {
  layer: <AmbientBackground />,
  control: <AmbientBackgroundControl />,
} as const;

export function withAmbientBackground(themeToggle: ReactNode): ReactNode {
  return (
    <>
      {themeToggle}
      {ambientBackgroundRegistration.control}
      {ambientBackgroundRegistration.layer}
    </>
  );
}
