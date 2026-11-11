# Third-party notices: ambient background animation

The optional background animation (`apps/desktop/src/ui/background`) bundles code from two
MIT-licensed projects. The licence texts are in this directory's `vendor/` folder, are
repeated in the renderer's file header, and are shown in the app under
"Animation licences" next to the Background switch.

| Component | Source | Licence file |
| --- | --- | --- |
| Agent-Native ocean renderer (`vendor/ocean-renderer.js`) | Builder.io, Inc., `BuilderIO/agent-native`, commit `41fae733f80038c9258d797279ef87620c6f12a7` (published asset `renderer-BGT6fWD2.js`, inspected 2026-11-08) | `vendor/LICENSE.agent-native` (Copyright (c) 2026 Builder.io, Inc.) |
| vgpu WebGPU helper, embedded in the renderer bundle | Vercel, Inc., `vercel-labs/vgpu` | `vendor/LICENSE.vgpu` (Copyright (c) 2025 Vercel, Inc.) |

## Local modifications

- `vendor/ocean-renderer.js` is copied from `docs/operator-experience/animation/ocean-renderer.js`
  with one change: `var AgentNativeOcean = …` became `export const AgentNativeOcean = …` so it
  loads as an ES module chunk under the Content Security Policy (`script-src 'self'`). The
  upstream pre-processing (network preloading replaced by a local loader) is noted in its header.
- `vendor/ocean-renderer.d.ts` is original typing for the bundle's `createRenderer` API.
- `engine.ts` is adapted from `docs/operator-experience/animation/particle-wave.js`, including the
  Canvas 2D approximation, which is not the upstream FFT renderer.

The renderer is decorative, makes no network requests and reads no application data.
