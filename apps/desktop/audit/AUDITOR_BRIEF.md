# Auditor brief (shared by all screen auditors)

You compare the live MCAO desktop UI with the wireframe for ONE scope, then classify every difference.
Read-only. Edit nothing except your own findings file.

## Sources of truth (use these, not code reading, to find differences)

- Wireframe: `/Users/mckerracher.joshua/Code/agent-orch/docs/operator-experience/wireframes.html`.
  Open it with `file://…#<screen-id>`. The dark theme is `document.documentElement.dataset.theme="dark"`.

- Captures already taken (dark theme) are in `/Users/mckerracher.joshua/Code/agent-orch-worktrees/wireframe-audit/apps/desktop/audit/out/`:
  - `wf/<screen>.<bp>.{png,aria.yml,layout.json}`: the wireframe at bp ∈ desktop 1440×900, tablet 960×900, mobile 760×800.
  - `app/<screen>.<state>.<bp>.{png,aria.yml,layout.json}`: the app. The state is one of populated (fixture data), empty, loading, or offline (no daemon). App-only routes and interaction states are also there (`ls` the folder).
  - `layout.json` lists the visible landmarks, regions, headings and meters with bounding boxes (y includes scroll) and the doc width and height.

- To measure more, drive the live app yourself. Run `npx vite --host 127.0.0.1 --port <unique port>` from
  `/Users/mckerracher.joshua/Code/agent-orch-worktrees/wireframe-audit/apps/desktop`, open `http://127.0.0.1:<port>/?fixture=1#/<view>`, and use
  Playwright (`node` script with `import { chromium } from "@playwright/test"`, `channel: "chrome"`) for computed styles and
  bounding boxes. Before reading, set `localStorage["agent-orch.theme"]="dark"`. Stop your server when you finish.

## Classification sources

- Specs: `/Users/mckerracher.joshua/Code/agent-orch/docs/operator-experience/{operational-design.md,feedback-incorporation.md,plan.md}`,
  `/Users/mckerracher.joshua/Code/agent-orch-run/docs/ui/wireframe.md`, and `/Users/mckerracher.joshua/Code/agent-orch-run/docs/technical-implementation-plan.md`.

- Git history: `/Users/mckerracher.joshua/Code/agent-orch-run` at HEAD 9312e34d5 has the full history (579 commits). `motif-mcao/main` is a squash.
  Use `git log -S`, `git log --grep`, and `git log -- <path>` there. Code comments citing decisions (e.g. "D-5", "M7-18") count as evidence only
  when they trace to a spec or commit.

## Scope rules

- Compare structure, hierarchy, content (labels, sections, order, what is shown or hidden by default) and spacing/placement.
- Ignore color and typography UNLESS the wireframe or spec states them explicitly. Example: the dark sidebar surface IS specified
  (operational-design.md ~849).

- The wireframe shows sample data (10 workstreams, $26.00…). Different VALUES are not defects. A different STRUCTURE for showing them is a defect.
- List app elements absent from the wireframe as their own findings, classified like any other.
- Honest-state labels such as "PLACEHOLDER DATA (Dx)", the fixture banner and offline notices: classify them against the specs. Do not assume.

## Classification (exactly one)

- `defect`: the app departs from the wireframe and no spec or commit justifies it.
- `intentional`: a spec section or commit deliberately chose the deviation. Cite file:line or the commit SHA and subject.
- `wireframe-gap`: the wireframe does not define this (an app-only element or state, or an unspecified breakpoint). Say what the app does and whether it looks broken.

## Already-fixed branches (still report the defect; mark `covered_by`)

- `fix/DPES-1105_desktop-sidebar-dark-theme` (2092efbf8): dark translucent sidebar, ocean through the sidebar, brand blue, Workstreams heading box.
- `fix/DPES-1105_desktop-system-usage-rail` (77f08045a): removes the bottom Device resources strip. "System Usage" sits in the rail below Agent slots with bar meters.
- `fix/DPES-1105_desktop-workspace-activity-popover` (in progress): the rail becomes a "Workspace activity [n] ⌄" button with a popover of
  collapsible sections (Execution queue, Attention, Agent slots, System Usage), and the rail column is removed.

## Output

Write JSON to `/Users/mckerracher.joshua/Code/agent-orch-worktrees/wireframe-audit/apps/desktop/audit/findings/<scope>.json`:

```json
{ "scope": "...", "coverage": [{"capture": "app/x.y.z", "reviewed": true, "note": ""}],
  "findings": [{
    "id": "<scope>-NN", "title": "...", "classification": "defect|intentional|wireframe-gap",
    "severity": "high|medium|low",            // defects only: high = wrong structure/missing primary element/broken at a bp; medium = wrong placement/hierarchy/default state; low = spacing/label wording
    "routes": ["#/workspace"], "states": ["populated"], "breakpoints": ["desktop"],
    "wireframe_ref": "wireframes.html line N / selector / screen id",
    "evidence": "concrete: capture file names plus measured numbers (boxes, computed styles, ARIA lines)",
    "justification": "for intentional/gap: spec file:line or commit sha + subject",
    "suspected_root_cause": "component/file + mechanism (a hypothesis is OK; mark it as one)",
    "check": "a Playwright assertion in TS that FAILS on current main and PASSES when fixed. Use the harness API from audit/harness.ts (openApp(page, hash, scenario, bp)). Assert on roles/names/boxes, not on CSS class names, where possible.",
    "covered_by": null
  }]}

```

Then reply with only: the findings file path, the counts per classification, and one line per high-severity defect.
Be exhaustive within your scope. Every capture in your scope must appear in `coverage`.
