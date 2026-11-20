import { AmbientBackground, AmbientBackgroundControl } from "./AmbientBackground";

/**
 * Wiring points for the integrator:
 *
 * - Render `layer` once anywhere inside the app. It portals to `<body>` and renders nothing
 *   while the animation is off, so it is safe to mount unconditionally (`main.tsx` does).
 * - `control` is the on/off switch; Settings > Appearance renders it (`AppearanceSection`).
 */
export const ambientBackgroundRegistration = {
  layer: <AmbientBackground />,
  control: <AmbientBackgroundControl />,
} as const;
