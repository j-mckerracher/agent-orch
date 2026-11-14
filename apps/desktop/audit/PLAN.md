# Wireframe conformance: fix plan (DPES-1105)

Path: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/wireframe-audit/apps/desktop/audit/PLAN.md`

Baseline: `motif-mcao/main` be553261e. Audit branch `audit/wireframe-conformance`.
Wireframe: `/Users/mckerracher.joshua/Code/agent-orch/docs/operator-experience/wireframes.html` (dark theme).
Spec citations: `wireframe.md` = `/Users/mckerracher.joshua/Code/agent-orch-run/docs/ui/wireframe.md`
(decision record D-1..D-11 at line 235; OX mapping at lines 325-424); `operational-design.md` = `/Users/mckerracher.joshua/Code/agent-orch/docs/operator-experience/operational-design.md`.

## Summary

| Class | Count | Notes |
| --- | --- | --- |
| defect (fix needed) | 58 | 57 have a check; `workstreams-04` is unverifiable and has none |
| blocked-on-data | 2 | `review-and-app-only-06` (DPES-1108), `agents-06` (DPES-1107); checks exist, marked `test.fixme` |
| intentional | 20 | 16 original plus 4 reclassified from defect |
| wireframe-gap | 23 | 21 original gaps, `review-and-app-only-04` (not in wireframe), and `workstreams-03` (DPES-1105) |
| total findings | 103 | matches the 103 entries in the four findings files |

Root causes: 23 (ranked in section 3). Checks: 59 tests in `audit/checks.spec.ts`: 57 defect checks and 2 blocked-on-data checks (`test.fixme`).
Open defect checks FAIL on main. Checks for fixes merged to main (#16, #17) FAIL on be553261e and PASS on main (section 5a).
Each branch turns a subset green (section 5).

## 1. Reclassification table

Old class to new class. Coordinator-directed items are confirmed unless noted.

| Finding | Old → new | Spec citation and reason |
| --- | --- | --- |
| workspace-09 | defect → intentional | D-9 (`wireframe.md` line 249): background animation is post-release and opt-in. OX-A10 deferred (line 349). |
| workspace-10 | defect → intentional | OX-B4 lists per-workstream targets as missing (line 360). D-4 (line 244): real workstream and objective entities are post-release. No objective data source. |
| workspace-11 | defect (narrowed) | "Notes, no reply" is intentional: D-6 (line 246) and OX-B2 (line 357). The disabled "Request review" stays a defect: OX-B2a (line 358) says review chips navigate to OX-S2, which exists. "Escalate to Global" stays disabled with a reason (fixture rule 3, line 474). |
| workspace-17 | wireframe-gap → defect | `operational-design.md` line 244: "exactly four persistent primary destinations". The wireframe has no Projects nav item (verified). |
| workspace-22 | wireframe-gap → defect | Coordinator confirmed. `operational-design.md` line 216: unavailable is never shown as current. D-11 (line 251): unknown is never zero. The offline conversation stays on "Loading conversations" with no error and no composer. |
| workstreams-20 | wireframe-gap → defect | Loading state says "workstreams: not available". Loading and unavailable are different states (line 216). |
| agents-16 | wireframe-gap → defect | Empty list reads "task list is not available" and counts stay "(…)". An empty collection is available data. The wireframe defines no empty state, so this rests on the honest-state rule (line 216) and D-11. |
| review-and-app-only-13 | wireframe-gap → defect | Coordinator confirmed. "Decisions: loading" while pending decisions are unavailable (D-11, line 251). |
| review-and-app-only-16 | wireframe-gap → defect | Same as workspace-17: a fifth primary destination (`operational-design.md` line 244). |
| review-and-app-only-17 | wireframe-gap → defect | Coordinator confirmed. Offline project page shows "Pending review (0)" (D-11 line 251; OX-A9 line 348: unavailable is never 0). |
| review-and-app-only-19 | wireframe-gap → defect | Coordinator confirmed. Offline tasks page shows 0 / 0 on every filter (same rule). |
| review-and-app-only-04 | defect → wireframe-gap (dropped) | "Assign independent review" does not appear anywhere in `wireframes.html` review section (verified by text search). No OX row defines it. There is no wireframe basis, so no check. |
| agents-10 | defect → intentional | Scheduling and routing is a "Later-scope preview" (OX-G9, line 399). The deferred definition (line 332) names that wireframe label, so the collapsed default is a deferral. |
| agents-15 | defect → intentional | Usage provenance is a "Later-scope preview" (OX-G10, line 400). D12 has no live source (line 437). |
| agents-06 | defect → blocked-on-data | Heartbeat is missing per OX-G5 (line 395). The base-revision field has no OX row (see section 2). |
| review-and-app-only-06 | defect → blocked-on-data | The "groups" count needs a grouping key. D16 (line 441) lists only priority, response time, owner and silences. |
| workspace-03 | defect (narrowed) | The placeholder copy change is OX-A7-compliant ("searches loaded records only" note, line 346). Only the 277px width stays a defect (spec silent, so wireframe is authority, D-2). |
| workstreams-11 | defect (narrowed) | Archive and save-view are missing per OX-W2 (line 377, D9, M7-11), so they are intentional. "N initiatives" wording and derived sub-lines stay defects (OX-W2 "N initiatives list"). |
| agents-07 | defect (narrowed) | "0 disconnected sessions" shows unknown as zero (OX-G2, line 392). The disconnected list itself is blocked on D3 (section 2). |
| agents-13 | defect (narrowed) | Attempt number is existing data (OX-G4/G5). Provider and model per row is blocked on a model or version field (OX-G3, line 393). |
| workspace-06 | defect (scope note) | The check asserts the three groups `operational-design.md` section 21 names (queue, attention, agent slots). Running is a sub-group that the spec does not address, so the check excludes it. |
| workstreams-03 | defect → wireframe-gap (DPES-1105) | The wireframe itself does not keep rows 2-3 inside its 260 px tree box at 760 px. At 760x800, `section.worker-tree` is y 249-509 with `overflow:auto` (`wireframes.html` line 15). Workstream details 2 (`2 / 10 Platform API`, y 540-581) and 3 (`3 / 10 Desktop UI`, y 584-625) start below its bottom edge and are reachable only by scrolling. The check asserted containment at rest, which the wireframe does not give. Reclassified in `findings/workstreams.json`; check removed. |
| workspace-07 | defect (check was wrong; app is correct on main) | The autofix ("Open both disclosures before measuring the queue row") looked for a `Running` disclosure and matched `/^Running/` against row summaries that begin with a state glyph. On main, Running is a plain group (`ActivityPanel.tsx` `TaskGroup`, `role=group`), not a disclosure, since #17. Measured queue rows: main 39.5 px; be553261e 102.5 px. The wireframe queue cards are 46 px (`details.queue-card`, `wireframes.html` line 226). Check repaired to measure every rendered row against 48 px. Passes on main. |

Covered-by corrections (verified against branch heads):

- `workspace-05` covered by popover branch: true (passes).
- `workspace-06` covered by popover branch: true for the three spec groups. The Running sub-group stays open (spec-silent).
- `workspace-08` covered by usage-rail branch: true (passes). Covered by popover branch: FALSE (still full-width, measured 1440px). Covered by combined branch: true.
- `agents-18` covered by usage-rail branch: true. Popover branch: FALSE (detail bottom 1304px, strip top 843px). Combined branch: true.
- `workspace-07` covered_by was null. Popover branch: fails. Combined branch: passes. Main (#17): passes after the DPES-1105 check repair (39.5 px; be553261e 102.5 px).
- `workspace-01` and `workstreams-10` covered by sidebar branch: true (both pass).
- `covered_by` is missing (undefined) in `agents-16`, `agents-17`, `agents-19`. Normalized to null.

## 2. Blocked-on-data (scope questions for the user)

| Finding | Missing daemon field or method | Spec status |
| --- | --- | --- |
| review-and-app-only-06 | Grouping key on approvals (for "N groups") | Not in D16 contract (line 441). Raw-request and decision counts are derivable. |
| agents-06 | Workspace base revision on a task or session | Not in the OX mapping. Needs a field audit. |
| agents-06 | Heartbeat on a session | Missing per OX-G5 (line 395). Not in the fix. |
| agents-07 | Disconnected session list and count | D3 has no daemon method (line 197, line 451). The "0" chip fix needs no data, only the honest "not available" state. |
| agents-13 | Provider, model and version per session | OX-G3 lists it as missing (line 393). D14 is agents-only. |

Tracked daemon follow-ups (DPES-1105 decision, section 7):

- `agents-06` (workspace base revision): [DPES-1107](https://mccdh.atlassian.net/browse/DPES-1107)
- `review-and-app-only-06` (grouping key for the "N groups" receipt): [DPES-1108](https://mccdh.atlassian.net/browse/DPES-1108)

Both checks are `test.fixme(true, "Blocked on DPES-110x: …")`. Their assertions are unchanged, so each goes green only when the daemon data lands. The audit reports them as known-blocked (skipped), not as open defects. Until DPES-1108 lands, PR #22's honest partial state is correct: "Groups: not available · N raw requests · N independent decisions".

Not blocked (fixable without new data): review-and-app-only-01 (six facts can render "unavailable", OX-R6 line 411). Create workstream and the plan controls can render disabled with a reason (OX-W1, OX-W9).

## 3. Root causes, ranked

Severity is the highest defect severity in the group, then user impact. "Status" is covered (by a branch), partial, or new. The gates line lists the CI gates the fix must pass (section 3a).

1. RC-01 Honest states: unavailable is never loading, zero, or empty copy. High. 7 findings: workspace-22 (high), workstreams-05, workstreams-20, agents-16, review-and-app-only-13, -17, -19. Status: new.
2. RC-02 Workspace activity is a docked panel, not a top-right popover. High. 3 findings: workspace-05 (high), workspace-06, workspace-07. Status: covered by popover branch for 05 and 06; 07 covered by combined branch, and on main (#17) after the DPES-1105 check repair.
3. RC-03 Sidebar workstream tree overflows and blocks its own rows. High. 1 finding: workstreams-01 (high), checked at rest. workstreams-03 is a wireframe-gap (section 1). Status: new.
4. RC-04 Breakpoint parity with wireframe media queries (960 and 760). High. 3 findings: agents-01 (high), workstreams-02 (high), workstreams-15. Status: new.
5. RC-05 Six-fact completion record missing on project review and misplaced on global review. High. 2 findings: review-and-app-only-01 (high), -02. Status: new.
6. RC-07 Device resources band spans the window and covers content. Medium. 2 findings: workspace-08, agents-18. Status: covered by usage-rail branch (both pass); combined branch covers both; popover branch does not.
7. RC-06 Sidebar dark theme and Workstreams heading box. Medium. 2 findings: workspace-01, workstreams-10. Status: covered by sidebar branch (both pass).
8. RC-12 Agents control placement and action set. Medium. 4 findings: agents-04, agents-05, agents-08, agents-12. Status: new.
9. RC-16 Global review queue layout (tab bar placement, queue width). Medium. 2 findings: review-and-app-only-07, -08. Status: new.
10. RC-18 Candidate panel identity and action placement. Medium. 2 findings: review-and-app-only-03, -05. Status: new.
11. RC-17 Decision queue content and row actions. Medium (one member blocked). 2 findings: review-and-app-only-06 (blocked), -09. Status: new.
12. RC-20 Primary navigation has a fifth destination (Projects). Medium. 2 findings: workspace-17, review-and-app-only-16. Status: new.
13. RC-14 Agents default state and desktop split. Medium. 3 findings: agents-02, agents-03, agents-11 (low). Status: new.
14. RC-13 Agents assigned-work list: counts, filters, row facts, Available / idle tab. Medium. 4 findings: agents-07, agents-09, agents-13, agents-14 (low). Status: new.
15. RC-15 Agents selected detail facts. Medium (blocked). 1 finding: agents-06. Status: blocked-on-data.
16. RC-09 Workstream detail header, plan revision and tabs. Medium. 3 findings: workstreams-07, -08, -16 (low). Status: new.
17. RC-10 Workstreams page header and plan controls. Medium. 3 findings: workstreams-06, -12 (low), -14 (low). Status: new.
18. RC-11 Workstream detail panels and task table. Medium. 2 findings: workstreams-09, -13 (low). Status: new.
19. RC-08 Workstreams grouped by project (D-4) and list card. Medium. 2 findings: workstreams-04 (unverifiable, no check), workstreams-11 (low). Status: new.
20. RC-21 Workspace conversation copy, chips and composer. Medium. 4 findings: workspace-11, -12 (low), -13 (low), -14 (low). Status: partial; workspace-14 passes incidentally on the popover branch. workspace-14 is not in the popover branch's covered-by claim.
21. RC-19 Usage pill window label. Medium. 1 finding: workspace-04. Status: new.
22. RC-22 Shell brand and top-bar search sizing. Low. 2 findings: workspace-02, workspace-03. Status: new.
23. RC-23 Sidebar tree row markup and names. Low. 3 findings: workstreams-17, -18, -19. Status: new.

Totals: 60 findings across the 23 groups (58 defects plus 2 blocked-on-data), with workstreams-04 counted as a member with no check. workstreams-03 is a wireframe-gap and is not a member.

### 3a. Per-root-cause detail

For each: evidence (measured), wireframe reference, suspected files, check names, CI gates.

Common gates for every desktop fix (`scripts/ci/gates.mjs` and `.github/workflows/ci.yml`):
`desktop-npm-ci`, `desktop-tsc` (`tsc -b`), `desktop-vitest` (`vitest run`, scope `src/**/*.test.{ts,tsx}`), `desktop-build`.
Test-count manifests: `scripts/ci/expected.mjs` line 76 (`VITEST = { files: 52, tests: 1253 }`) and `scripts/ci/desktop-test-names.json`. Any added or removed Vitest test must update both.
`desktop-e2e` (macOS only; `E2E_TESTS = 44` at `scripts/ci/expected.mjs` line 77; `e2e/geometry.spec.ts`). Required when a fix touches layout at any breakpoint (RC-02, RC-03, RC-04, RC-07, RC-14, RC-16).
Docs-only changes to `audit/*.md` run super-linter markdown rules (`.github/linters/.markdown-lint.yml`; MD013, MD041, MD060 are off).
Audit gate (this plan): the RC's check names flip to PASS on the fix branch and stay FAIL on main.

RC-01 honest states. Evidence: `app/workspace.offline.desktop.png` shows "Loading conversations…" with no header or composer. Offline `workstreams` shows "Loading workstreams…". Review offline shows "Decisions: loading" and "Pending review (0)" on the project page. Wireframe: C1 connection states (`wireframe.md` line 35; `operational-design.md` line 216). Suspected: `WorkspaceView.tsx` lines 42 and 59 (`live` from provenance; `listLoading` never resolves); `WorkstreamTree.tsx` and `WorkstreamsView.tsx` (loading text); `AgentsView.tsx` lines 66-68 and `SessionList.tsx` label(); `ReviewDecisionsView.tsx` heading; `ProjectDetail.tsx` pending count (`approvals.data ?? []`); `TaskList.tsx` filter counts (`tasks.data ?? []`). Checks: workspace-22, workstreams-05, workstreams-20, agents-16, review-and-app-only-13, -17, -19. Status: new. Gates: common desktop gates.

RC-02 workspace activity popover. Evidence: capture `app/workspace.populated.desktop.layout.json` shows the docked aside at 300x651 when collapsed. Wireframe: OX-B6 (`wireframe.md` line 362), `wireframes.html` `command-center` inspector. Suspected: `AppShell.tsx` lines 95-97 (aside as grid column), `ActivityPanel.tsx` line 383. Checks: workspace-05, workspace-06, workspace-07. Branch: popover `b95a4b74a` passes 05 and 06; combined `978e8a96b` passes 05, 06, 07. Status: covered by popover branch (05, 06); 07 covered by combined branch and passes on main (#17) after the DPES-1105 check repair. Gates: common desktop gates, plus `desktop-e2e` (layout).

RC-03 sidebar tree overflow. Evidence: at desktop and tablet, `treeitem` "Workstream 2 of 3" and "3 of 3" fail `elementFromPoint` hit tests (below the fold or covered). At mobile (760), tree row bottom exceeds nav bottom. Wireframe: sidebar tree `Workstream N of 10` (`wireframes.html` `workstreams`). Suspected: `shell.css` nav `max-height` with `overflow: visible`; `WorkstreamTree.tsx`. Checks: workstreams-01 (at rest, no scroll). workstreams-03 removed (wireframe-gap, section 1). Status: new. Gates: common desktop gates, plus `desktop-e2e`.

RC-04 breakpoint parity. Evidence: at tablet, agents detail sits beside the Assigned work card (agents-01); workstreams list and detail stack (workstreams-02). At mobile, workstream list is one column (workstreams-15). Wireframe: media queries at `max-width:960px` (4 rules), `max-width:760px` (10 rules), `max-width:1150px` (24 rules). Suspected: `agents.css` line 203 (`max-width:900px`, not 960); `workstreams.css` lines 386-392 (`.ws-layout` collapse, no two-column mobile grid). Checks: agents-01, workstreams-02, workstreams-15. Status: new. Gates: common desktop gates, plus `desktop-e2e`.

RC-05 six-fact record. Evidence: `/project/fixture-project/review/fixture-task-1` shows no "Six separate facts" heading (review-and-app-only-01). Global `#/review` places the facts above "Unified diff" (review-and-app-only-02). Wireframe: OX-R6 (`wireframe.md` line 411); S9 layout. Suspected: `src/review/ReviewScreen.tsx` line 115 (`{approval && embedded?.facts?.(r)}`); project route does not mount the facts slot. Checks: review-and-app-only-01, -02. Status: new. Gates: common desktop gates, plus `desktop-e2e`.

RC-06 sidebar dark theme and heading. Evidence: `--sidebar: #f2f7fc` light value (`tokens.css` lines 41, 119, 169); sidebar background averages above 128. Workstreams heading has no bordered ancestor. Wireframe: `wireframes.html` sidebar (dark sidebar, `operational-design.md` line 849). Checks: workspace-01, workstreams-10. Status: covered by sidebar branch `2092efbf8`, both pass. Gates: common desktop gates (its `sidebar.test.ts` and `contrast.test.ts` are already in the branch).

RC-07 device resources band. Evidence: `workspace-08` measures the "Device resources" region at 1440px width (full window) on main and popover. `agents-18`: selected detail bottom at 1304px, strip top at 843px. Wireframe: four resource lines at the bottom edge (`operational-design.md` section 18) versus OX-A9 and OX-B6h (System Usage in the activity panel). Suspected: `AppShell.tsx` lines 100-102 (resources slot as full-width row beneath content); `ResourceStrip.tsx`. Checks: workspace-08, agents-18. Status: covered by usage-rail `77f08045a` (both pass); combined `978e8a96b` (both pass); popover `b95a4b74a` does not cover (both fail). Gates: common desktop gates, plus `desktop-e2e`.

RC-08 workstreams grouping and list card. Evidence: list card reads "3 workstreams" (fixture entities `ws-auth`, `ws-docs`, ...), wireframe "10 initiatives" (OX-W2 line 377). D-4 (line 244) says one workstream per project. Suspected: `views/workstreams/model.ts` builds groups from fixture workstream entities. Checks: workstreams-11 (workstreams-04 has no check, see section 6). Status: new. Gates: common desktop gates.

RC-09 workstream detail header and tabs. Evidence: no "Plan revision" text, no Activity tab, no "Purpose and rationale" paragraph. Wireframe: OX-W3 (line 378), OX-W5 (line 380), `wireframes.html` `workstreams`. Suspected: `WorkstreamsView.tsx` detail header (workstream-level only, objective card absent); tabs replaced by a sentence near line 446. Checks: workstreams-07, workstreams-08, workstreams-16. Status: new. Gates: common desktop gates.

RC-10 workstreams header and plan controls. Evidence: "Create workstream" is a form under the heading (`WorkstreamsView.tsx` `<form className="ws-create">`); no Combine or Change execution policy buttons; eyebrow reads "ORCHESTRATION" inline, wireframe "Continuing initiatives / S4" above the title. Wireframe: OX-W1 (line 376), OX-W9 (line 384, disabled with reason). Checks: workstreams-06, workstreams-12, workstreams-14. Status: new. Gates: common desktop gates.

RC-11 workstream detail panels and task table. Evidence: no "Dependency and resource detail" panel; task table has separate "Objective" and "Project" columns (wireframe: "objective/project", OX-W6 line 381). Wireframe: OX-W7 (line 382). Suspected: panel not built in `WorkstreamsView.tsx`; table header. Checks: workstreams-09, workstreams-13. Status: new. Gates: common desktop gates.

RC-12 agents control placement and action set. Evidence: "Cancel attempt" is under Task controls with label "Simulate cancel" (`TaskControls.tsx` line 131). "Reassign via new attempt", "Filter raw trace", "Request diagnostic", "Branch from retained evidence" are absent. "Configure providers" sits inside the inventory (`Inventory.tsx` line 96), not the page header (`AgentsView.tsx` lines 24-29). Resume label reads "Resume session" with a reason (`SessionDetail.tsx` lines 105-108). Wireframe: OX-G1 (line 391), OX-G6 (line 396), OX-G7 (line 397), OX-G8 (line 398). Checks: agents-04, agents-05, agents-08, agents-12. Status: new. Gates: common desktop gates.

RC-13 agents assigned-work list. Evidence: no filter controls (`SessionList.tsx`). "Failed" and "Disconnected" tabs count tasks (`model.ts` `tabRows`). "0 disconnected sessions" shown as a number. "Available" tab lists adapters (`model.ts` line 15, `SessionList.tsx` Available()). Rows omit attempt number (`WorkRow` has no attempt field in view). Wireframe: OX-G2 (line 392), OX-G4 (line 394). Checks: agents-07, agents-09, agents-13, agents-14. Status: new. Gates: common desktop gates.

RC-14 agents default state and split. Evidence: inventory is open by default (`Inventory.tsx` line 21 `defaultOpen`); detail panel has no default selection (`AgentsView.tsx`, `selectedId` null). Detail column 516px, wireframe 545px; `agents.css` line 81 `minmax(0,1.25fr) minmax(0,1fr)` vs wireframe `minmax(0,1.1fr)`. Checks: agents-02, agents-03, agents-11. Status: new. Gates: common desktop gates, plus `desktop-e2e` (layout).

RC-15 agents selected detail facts. Evidence: the detail names "Amend TaskContract" only as a disabled control. The auditor's claim was the base revision. Checks: agents-06 (blocked). Status: blocked-on-data (section 2). Gates: common desktop gates when unblocked.

RC-16 global review queue layout. Evidence: filter tabs render inside the queue card (`DecisionQueue.tsx`, `queue.ts` line 17). Queue column 340px (`review-decisions.css` lines 55-57 `minmax(260px, 340px)`) vs wireframe about 560px. Wireframe: OX-R2 (line 407). Checks: review-and-app-only-07, -08. Status: new. Gates: common desktop gates, plus `desktop-e2e` (layout).

RC-17 decision queue content. Evidence: no grouping receipt (`ReviewDecisionsView.tsx` heading shows only the count). Queue rows have no per-item primary action (`DecisionQueue.tsx` row markup). Wireframe: OX-R3 (line 408); row labels "Compare / Inspect / Review exact action". Checks: review-and-app-only-06 (blocked), -09. Status: new (06 blocked). Gates: common desktop gates.

RC-18 candidate panel and action placement. Evidence: "Candidate identity" region lacks project and workstream identity (`src/review/ReviewScreen.tsx` identity block, lines 88-115). "Request additional checks" sits in a collapsed Decision behavior disclosure (`DecisionBehavior.tsx`). Wireframe: OX-R5 (line 410). Checks: review-and-app-only-03, -05. Status: new. Gates: common desktop gates.

RC-19 usage pill window label. Evidence: popover reads "Today's AI usage" with "$26.00 reported since Jan 1, 2026, 12:00 AM (America/Denver)". The heading is hard-coded (`UsagePill.tsx` line 206) and the window comes from the fixture `windowStart` (lines 75, 82-83). Wireframe: "since local midnight" (`operational-design.md` line 836). Checks: workspace-04. Status: new. Gates: common desktop gates. Fixture-only window; the daemon has no usage source (D12).

RC-20 primary navigation. Evidence: sidebar contains a "Projects" link on `#/workspace` and `#/projects` (probe). The wireframe has no Projects item. `operational-design.md` line 244 says exactly four destinations. `SidebarNav.tsx` route list. Checks: workspace-17, review-and-app-only-16. Status: new. Gates: common desktop gates. D-3 keeps the `#/project/...` routes, so only the nav entry changes.

RC-21 workspace conversation copy, chips and composer. Evidence: project target shows "Notes, no reply" (intentional) and disabled "Request review" (defect). Scope line reads "All projects" (`WorkspaceView.tsx` line 21). Placeholder reads "Message `<target>`" (`WorkspaceView.tsx` line 383). Target chip label clipped (`workspace.css` lines 207-212). Checks: workspace-11, -12, -13, -14. Status: partial (workspace-14 incidentally passes on popover branch). Gates: common desktop gates. Overlaps the popover branch (`ConversationTargetPicker.tsx`, `ModelPicker.tsx`, `workspace.css`).

RC-22 brand and top-bar search sizing. Evidence: brand wordmark 18px (`shell.css` lines 115-121) vs wireframe 30px. Search combobox 277px (`Search.tsx` line 262; `topbar.css` grid) vs wireframe 681px. Checks: workspace-02, workspace-03. Status: new. Gates: common desktop gates. Overlaps sidebar, usage-rail and popover branches (`shell.css`).

RC-23 sidebar tree row markup and names. Evidence: chevron span (`WorkstreamTree.tsx` explicit chevron, class `.ws-tree-chevron`). Region name "Workstreams" vs wireframe "Workstreams and individual workers" (`SidebarNav.tsx` / `WorkstreamTree.tsx` aria-label). No green bullet (`.ws-tree-task-name` lacks `::before`). Checks: workstreams-17, -18, -19. Status: new. Gates: common desktop gates. Note: the chevron and bullet checks use class names (see section 6).

## 4. File-overlap matrix and recommended order

Branches: `sidebar` = `fix/DPES-1105_desktop-sidebar-dark-theme`; `usagerail` = `fix/DPES-1105_desktop-system-usage-rail`; `popover` = `fix/DPES-1105_desktop-workspace-activity-popover`; `combined` = `fix/DPES-1105_desktop-workspace-activity-and-system-usage` (`978e8a96b`; includes the popover work and the System Usage move).

| Root cause | Files (suspected) | Branch overlap |
| --- | --- | --- |
| RC-01 honest states | `WorkspaceView.tsx`, `WorkstreamsView.tsx`, `WorkstreamTree.tsx`, `AgentsView.tsx`, `SessionList.tsx`, `ReviewDecisionsView.tsx`, `ProjectDetail.tsx`, `TaskList.tsx` | `WorkspaceView.tsx` also in RC-21 |
| RC-02 activity popover | `AppShell.tsx`, `ActivityPanel.tsx`, `shell.css`, `usePopover.ts` | popover and combined (covered) |
| RC-03 sidebar tree overflow | `shell.css`, `WorkstreamTree.tsx` | sidebar, usagerail and popover edit `shell.css` |
| RC-04 breakpoints | `agents.css`, `workstreams.css` | none |
| RC-05 six facts | `src/review/ReviewScreen.tsx`, `ReviewDecisionsView.tsx` | none |
| RC-06 sidebar theme | `tokens.css`, `shell.css`, `WorkstreamTree.tsx` | sidebar (covered) |
| RC-07 device band | `AppShell.tsx`, `ResourceStrip.tsx`, `shell.css`, `registry.tsx`, `slots.ts` | usagerail (covered); combined (covered); popover edits `AppShell.tsx` and `shell.css` but not the band |
| RC-08 grouping and list card | `views/workstreams/model.ts`, `WorkstreamsView.tsx` | `WorkstreamsView.tsx` shared with RC-09 to RC-11 |
| RC-09 detail header and tabs | `WorkstreamsView.tsx` | shared with RC-08, RC-10, RC-11 |
| RC-10 header and plan controls | `WorkstreamsView.tsx` | shared |
| RC-11 detail panels and table | `WorkstreamsView.tsx` | shared |
| RC-12 agents actions | `TaskControls.tsx`, `SessionDetail.tsx`, `Inventory.tsx`, `AgentsView.tsx` | `AgentsView.tsx` shared with RC-13, RC-14 |
| RC-13 agents list | `model.ts`, `SessionList.tsx`, `AgentsView.tsx` | shared |
| RC-14 agents default and split | `Inventory.tsx`, `AgentsView.tsx`, `agents.css` | shared |
| RC-15 agents detail (blocked) | `model.ts`, `SessionDetail.tsx` | none |
| RC-16 review queue layout | `DecisionQueue.tsx`, `queue.ts`, `review-decisions.css` | none |
| RC-17 decision queue content | `ReviewDecisionsView.tsx`, `DecisionQueue.tsx` | shared with RC-16 |
| RC-18 candidate panel | `ReviewScreen.tsx`, `DecisionBehavior.tsx` | shared with RC-05 (`ReviewScreen.tsx`) |
| RC-19 usage pill | `UsagePill.tsx`, `fixtures/ui/usage.json` | none |
| RC-20 primary nav | `SidebarNav.tsx` | none |
| RC-21 conversation copy | `WorkspaceView.tsx`, `ModelPicker.tsx`, `ConversationTargetPicker.tsx`, `workspace.css` | popover and combined edit `ConversationTargetPicker.tsx`, `ModelPicker.tsx`, `workspace.css` |
| RC-22 brand and search | `shell.css`, `Search.tsx`, `topbar.css` | sidebar, usagerail and popover edit `shell.css` |
| RC-23 tree row markup | `WorkstreamTree.tsx`, `workstreams.css` | none (sidebar does not edit `WorkstreamTree.tsx`) |

Recommended order (merge in this sequence so the shared-file conflicts are resolved once):

1. Merge `sidebar` (RC-06 covered). Then merge `usagerail` (RC-07 covered). Both edit `shell.css`.
2. Merge `combined` in place of `popover` (it contains popover plus the System Usage move; RC-02 and RC-07 covered). Do not merge `popover` and `combined` both.
3. RC-03, RC-22, RC-20 on top of the merged `shell.css` and `SidebarNav.tsx`.
4. RC-21 after the popover work (shares `ConversationTargetPicker.tsx`, `ModelPicker.tsx`, `workspace.css`). RC-01 after RC-21 (shares `WorkspaceView.tsx`).
5. The workstreams cluster (RC-08 to RC-11, RC-23) as one branch on `WorkstreamsView.tsx`.
6. The agents cluster (RC-12, RC-13, RC-14, RC-15) as one branch on `AgentsView.tsx`.
7. The review cluster (RC-05, RC-16, RC-17, RC-18) as one branch on `ReviewScreen.tsx` and `DecisionQueue.tsx`.
8. RC-04 (breakpoints) can run in parallel with any of the above since it edits `agents.css` and `workstreams.css` only, but it changes the same layout tests as RC-14 and RC-16, so rebase onto those.
9. RC-19 (usage pill) is independent.

## 5. Check results

Columns: `main` (`/Users/mckerracher.joshua/Code/agent-orch-worktrees/wireframe-audit`, all checks expected to FAIL), `sidebar` (`desktop-sidebar-dark-theme` `2092efbf8`), `usagerail` (`desktop-system-usage-rail` `77f08045a`), `popover` (`desktop-workspace-activity-popover` `b95a4b74a`). Combined branch (`desktop-activity-system-usage` `978e8a96b`) was run for RC-02 and RC-07 only: workspace-05, -06, -07, -08 and agents-18 all PASS.

Counts: main 0 PASS of 60. sidebar 2 PASS. usagerail 2 PASS. popover 3 PASS. combined (5 checks run) 5 PASS.

`workstreams-04` has no check and is not in the matrix (section 6).

The `workspace-08` check was revised twice during this run. The final version measures the device band width (full-width band fails) and was rerun on all four configurations. The earlier runs in `audit/out/results-*.json` used the earlier wording and are superseded for that one check.

Checks changed in DPES-1105 (workstreams-01, workstreams-03, workspace-07, workstreams-20, agents-18, agents-06, review-and-app-only-06, workspace-01) have superseded rows in this matrix. Section 5a has their current results. workstreams-03 has no check now.

Matrix (`PASS` = the check passes on that app, `FAIL` = it fails):

| Check | main | sidebar | usagerail | popover |
| --- | --- | --- | --- | --- |
| agents-01 | FAIL | FAIL | FAIL | FAIL |
| agents-02 | FAIL | FAIL | FAIL | FAIL |
| agents-03 | FAIL | FAIL | FAIL | FAIL |
| agents-04 | FAIL | FAIL | FAIL | FAIL |
| agents-05 | FAIL | FAIL | FAIL | FAIL |
| agents-06 | FAIL | FAIL | FAIL | FAIL |
| agents-07 | FAIL | FAIL | FAIL | FAIL |
| agents-08 | FAIL | FAIL | FAIL | FAIL |
| agents-09 | FAIL | FAIL | FAIL | FAIL |
| agents-11 | FAIL | FAIL | FAIL | FAIL |
| agents-12 | FAIL | FAIL | FAIL | FAIL |
| agents-13 | FAIL | FAIL | FAIL | FAIL |
| agents-14 | FAIL | FAIL | FAIL | FAIL |
| agents-16 | FAIL | FAIL | FAIL | FAIL |
| agents-18 | FAIL | FAIL | PASS | FAIL |
| review-07 | FAIL | FAIL | FAIL | FAIL |
| review-08 | FAIL | FAIL | FAIL | FAIL |
| review-13 | FAIL | FAIL | FAIL | FAIL |
| review-17 | FAIL | FAIL | FAIL | FAIL |
| review-19 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-01 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-02 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-03 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-05 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-06 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-09 | FAIL | FAIL | FAIL | FAIL |
| review-and-app-only-16 | FAIL | FAIL | FAIL | FAIL |
| workspace-01 | FAIL | PASS | FAIL | FAIL |
| workspace-02 | FAIL | FAIL | FAIL | FAIL |
| workspace-03 | FAIL | FAIL | FAIL | FAIL |
| workspace-04 | FAIL | FAIL | FAIL | FAIL |
| workspace-05 | FAIL | FAIL | FAIL | PASS |
| workspace-06 | FAIL | FAIL | FAIL | PASS |
| workspace-07 | FAIL | FAIL | FAIL | FAIL |
| workspace-08 | FAIL | FAIL | PASS | FAIL |
| workspace-11 | FAIL | FAIL | FAIL | FAIL |
| workspace-12 | FAIL | FAIL | FAIL | FAIL |
| workspace-13 | FAIL | FAIL | FAIL | FAIL |
| workspace-14 | FAIL | FAIL | FAIL | PASS |
| workspace-17 | FAIL | FAIL | FAIL | FAIL |
| workspace-22 | FAIL | FAIL | FAIL | FAIL |
| workstreams-01 | FAIL | FAIL | FAIL | FAIL |
| workstreams-02 | FAIL | FAIL | FAIL | FAIL |
| workstreams-05 | FAIL | FAIL | FAIL | FAIL |
| workstreams-06 | FAIL | FAIL | FAIL | FAIL |
| workstreams-07 | FAIL | FAIL | FAIL | FAIL |
| workstreams-08 | FAIL | FAIL | FAIL | FAIL |
| workstreams-09 | FAIL | FAIL | FAIL | FAIL |
| workstreams-10 | FAIL | PASS | FAIL | FAIL |
| workstreams-11 | FAIL | FAIL | FAIL | FAIL |
| workstreams-12 | FAIL | FAIL | FAIL | FAIL |
| workstreams-13 | FAIL | FAIL | FAIL | FAIL |
| workstreams-14 | FAIL | FAIL | FAIL | FAIL |
| workstreams-15 | FAIL | FAIL | FAIL | FAIL |
| workstreams-16 | FAIL | FAIL | FAIL | FAIL |
| workstreams-17 | FAIL | FAIL | FAIL | FAIL |
| workstreams-18 | FAIL | FAIL | FAIL | FAIL |
| workstreams-19 | FAIL | FAIL | FAIL | FAIL |
| workstreams-20 | FAIL | FAIL | FAIL | FAIL |
| combined (RC-02, RC-07 checks only) | run: workspace-05 PASS, workspace-06 PASS, workspace-07 PASS, workspace-08 PASS, agents-18 PASS | | | |

### 5a. Repaired checks (DPES-1105)

Current `checks.spec.ts` run against three builds: be553261e (pre-fix, detached worktree), main d9aaef92f, and PR #21 at 053ae7026 (head at run time). Positions are CSS px (y ranges).

| Check | be553261e (pre-fix) | main d9aaef92f | PR #21 053ae7026 | Evidence |
| --- | --- | --- | --- | --- |
| workstreams-01, desktop 1440x900 | FAIL | FAIL | PASS | main: rows 2 and 3 at y 809-837 and 837-883, tree box 554-742. Row 2 is covered by `button.ui-theme-toggle`, row 3 by `p.ambient-note`. #21: rows at 680-708 and 708-754, inside tree box 425-838. |
| workstreams-01, tablet 960x900 | FAIL | FAIL | PASS | main: rows at 909-937 and 937-983, off screen (viewport 900), tree box 584-744. #21: rows at 764-792 and 793-839, inside tree box 439-840 (row 3 ends 1 px inside the bottom edge). |
| workspace-07 | FAIL (102.5 px) | PASS (39.5 px) | not run | Tallest rendered queue row. Wireframe queue card: 46 px. |
| workstreams-20 | FAIL | FAIL | not run | Loading state: the top-bar `span.top-sr` text and the sidebar `span[aria-label]` both match. |
| agents-18 | FAIL | PASS | not run | be553261e: detail bottom 1304 px, strip top 843 px. Main passes without measuring: no "Device resources" region on `#/agents` (section 6). |
| workspace-01 | FAIL (mean 247) | PASS | not run | Sidebar background; light on be553261e. |
| workstreams-03 | removed | removed | removed | wireframe-gap (section 1). |
| agents-06 | skipped (`test.fixme`, DPES-1107) | skipped | not run | Blocked on data (section 2). |
| review-and-app-only-06 | skipped (`test.fixme`, DPES-1108) | skipped | not run | Blocked on data (section 2). |

Autofix review. Five autofix commits (`43b4520fd` to `05ab3933c`) followed PR #18 (head before the autofix: `c3b464ac3`). Verdicts:

- `workstreams-20`: the autofix changed `getByText` to `getByLabel`, which missed the top-bar text. The check now unions both locators. It fails on both builds.
- `workspace-07`: the check was wrong (section 1). Repaired. The app passes on main.
- `workstreams-01`: weakened. Scrolling each row into view before hit-testing made it pass on be553261e, so it detected nothing. Repaired to an at-rest check (table above). It fails on main.
- `workstreams-03`: weakened, and the premise is wrong. Removed; reclassified as wireframe-gap.
- `agents-18`: weakened. The reachability check passed on be553261e. The original geometric check is restored. It fails on be553261e and passes on main.
- `workspace-01`: kept. The `color(srgb ...)` normalization only changes results for that format, where unscaled channels in 0-1 would always pass. It fails on be553261e (mean 247).

## 6. What could not be verified

- Empty and loading scenarios for the review queue. The review queue has its own fixture daemon, so `openApp(..., "empty" | "loading")` does not reach it. `review-and-app-only-*` checks use populated and offline only.
- `workstreams-04` (grouping by project). Fixture-mode `#/projects` renders "Offline: no cached projects" with no rows, so there is no project list to compare against. Its check was removed; the finding stays a defect with no check.
- Native Tauri and live daemon. WKWebView has no CDP. Everything here runs in fixture or no-bridge mode in Chromium.
- Running sub-group default. `operational-design.md` section 21 names only the queue, attention and agent slots as collapsed. The check asserts those three. The popover branch leaves Running open (`ActivityPanel.tsx` line 436 in that branch); not asserted because the spec is silent.
- `review-and-app-only-04`. Dropped: the "Assign independent review" control has no wireframe or OX row.
- Class-based selectors in three checks (`workstreams-17` `.ws-tree-chevron`, `workstreams-19` `.ws-tree-task-name`, and the `details.activity-*` queries in the earlier draft). These are brittle; keep them only until the components expose stable roles. The `workspace-06` check uses summary text and `details` parents instead.
- Wireframe grid and column widths (`agents-11` 545px, `review-and-app-only-08` about 560px). The widths come from measured wireframe layout; tolerance is set to 20px and 500px.
- Super-linter (markdown on `audit/*.md`) and gitleaks were not run locally. Only the configs were read: `.github/linters/.markdown-lint.yml` disables MD013, MD041 and MD060, and the rest of the markdown rules are default.
- Audit checks pass or fail on fixture data. A daemon-backed build could move values (the same check still measures structure, not values).
- The `harness.ts` default `WIREFRAME` path does not resolve from a worktree. Set `WIREFRAME=` explicitly. This is documented in the config header and is not changed here.
- PR, CI and push have not been run. See the final report.
- `agents-18` passes on main without measuring anything: the device strip is not rendered on `#/agents` after #17. It still fails on be553261e. A stable selector for the strip would make it a real at-rest guard.
- Findings JSON classes lag section 1 for some items. For example, `findings/workstreams.json` still has `workstreams-20` as `wireframe-gap`, and section 1 reclassifies it to defect. Only `workstreams-03` (classification) and `workspace-07` (`covered_by`) were changed in DPES-1105.
- The blocked checks (`agents-06`, `review-and-app-only-06`) were not run. They are skipped by `test.fixme`.

## 7. Decisions log

Decisions from the user (DPES-1105). They govern the checks and this plan.

| Decision | Effect |
| --- | --- |
| TruffleHog findings are ignored. | Only Trivy gates PRs. Old TruffleHog fixture hits do not block. |
| Search placeholder (RC-22): show the honest list of what is actually searched, not the wireframe wording. | PR #21 updates `workspace-03`. Not edited in DPES-1105. |
| The connection strip stays in the content column. | No check. The strip is not moved to the sidebar. |
| The ambient Background control is compacted in the sidebar. | Supports the at-rest fit of `workstreams-01` on PR #21 (section 5a). |
| workspace-10 (objective coordinators) is intentional under D-4 and D-6. | Reclassified to intentional in section 1. No check. |
| workspace-11 uses a link role for Request review. | PR #28 updates the check. Not edited in DPES-1105. |
| Daemon follow-ups: DPES-1107 and DPES-1108. | `agents-06` and `review-and-app-only-06` are `test.fixme` (section 2). |

Check rules applied in DPES-1105: checks measure at rest and never scroll first. A check for an open defect fails on main. A check for a fix merged to main (#16, #17) fails on be553261e and passes on main.
