# Wireframe conformance audit — inventory

Sources: `docs/operator-experience/wireframes.html` (original, main checkout) and the live
desktop frontend served by Vite from `motif-mcao/main` (be553261e). The running Tauri app's
`apps/desktop` tree is byte-identical to that commit.

## Routes (from `apps/desktop/src/ui/router.ts`)

| # | Route | Hash | Wireframe screen |
|---|-------|------|------------------|
| R1 | global/workspace | `#/workspace` (default) | `#command-center` (Orchestrator Workspace) |
| R2 | global/workstreams | `#/workstreams` | `#workstreams` |
| R3 | global/agents | `#/agents` | `#agents-sessions` |
| R4 | global/review | `#/review` (`?task=<id>` opens one candidate, `?project=<id>` filters) | `#review-decisions` (section D) |
| R5 | projects | `#/projects` | none (app-only) |
| R6 | project | `#/project/:id` | none |
| R7 | tasks | `#/project/:id/tasks/:filter` (all, queued, running, blocked, review, terminal, state:*) | none |
| R8 | task | `#/project/:id/task/:taskId` | none |
| R9 | fixture-task | `#/project/:id/preview/:id` | none |
| R10 | coordinator | `#/project/:id/coordinator` | partial: workspace "Talking to: `<coordinator>`" variants |
| R11 | project review (old form) | `#/project/:id/review/:taskId?` redirects to R4 | `#review-decisions` detail |

## Modes (from `src/native/boot.ts`)

- **fixture**: `?fixture=1`, sample data. Populated state.
- **no-bridge**: browser without fixture. Daemon unavailable → error/offline state.
- **native**: Tauri webview + live daemon. WKWebView has no CDP, so Playwright cannot drive it.

## States per route

| State | How produced | Applies to |
|-------|--------------|------------|
| populated | fixture mode | R1–R4, R9, R11 |
| empty | fixture mode + Playwright route-intercept of the bundled fixture JSON modules returning empty collections | R1–R4 |
| loading | route-intercept that never resolves fixture modules | R1–R4 |
| error / offline | no-bridge mode | R1–R8, R10 |
| validation | composer: empty draft send disabled; target search no-match (`target-empty` in the wireframe) | R1 |
| conversation target variants | target picker: Global / coordinator | R1 (wireframe has 3 targets) |
| inspector open/closed | Workspace activity disclosure | R1 |
| roles | **N/A**: no role/permission model in router, daemon types, or wireframe | — |

## Breakpoints (from the wireframe's own media queries: ≤760, 761–960, ≥961 / 1150 / 1200)

- **1440×900** (desktop, above every max-width rule)
- **960×900** (tablet rule `max-width:960px`; also the existing geometry spec width)
- **760×800** (mobile rule `max-width:760px`; equals Tauri `minWidth`)

Theme: the wireframe is authored dark (`data-theme="dark"`). Audit in dark; light is a spot check only.

## Not verifiable with Playwright

- Native Tauri window chrome and the live daemon data path (WKWebView). Covered only by
  structural parity: the same bundle in no-bridge/fixture mode.
- Project-scoped routes R6–R8, R10 with real data: fixture mode has no projects, so only their
  offline/error state is reachable.
