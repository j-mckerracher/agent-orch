# M7-02 macOS WKWebView manual check

This runbook accompanies branch `orch/0eca0ef2-m7-02-close`. The browser source candidate before this evidence-only commit is `beb12e020`. Use the final HEAD recorded in the closure report, and record its full SHA before testing. Browser results do not establish native acceptance.

Run in the retained task worktree only:

```sh
cd /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-didactic-lamp/.agent-worktrees/0eca0ef2-m7-02-close/apps/desktop
pwd
git -C /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-didactic-lamp/.agent-worktrees/0eca0ef2-m7-02-close rev-parse --show-toplevel
git -C /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-didactic-lamp/.agent-worktrees/0eca0ef2-m7-02-close rev-parse HEAD
node --version
npm --version
CARGO_TARGET_DIR=/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-didactic-lamp/.agent-worktrees/0eca0ef2-m7-02-close/target/m7-02-close npm run tauri dev
```

Keep Cargo output isolated by setting `CARGO_TARGET_DIR=/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-didactic-lamp/.agent-worktrees/0eca0ef2-m7-02-close/target/m7-02-close` before the launch. Use a unique evidence directory under `target/m7-02-close-logs/native-<UTC timestamp>-<UUID>/`. Capture launch stdout/stderr and exit status. Preserve failures and rerun in a new directory after any source correction. Do not overwrite earlier evidence.

1. Confirm this is the native WKWebView window, not Chrome. Record macOS version, app revision, display scale and actual content dimensions. The default is 1280×800 and the configured minimum is 760×600. Window decorations can change content size; use content width for boundary observations.
2. Resize in order to 1280, 1151, 1150, 961, 960, 761 and 760 content pixels wide. Use height 800 except also check 760×600. At every size capture a screenshot and record whether the document, sidebar, top bar, main, activity or resource slot scrolls horizontally. Table-specific inner scrollers are allowed.
3. Above 960, confirm the activity panel is right of the workspace main. At 960 and below it appears before main. At 760 the sidebar becomes a top block capped at 42dvh, with its own vertical scrolling. Reach every navigation item and Appearance through that scroller. Confirm the shell body scrolls to main and resources, with no resource overlap intercepting activity controls.
4. With the native daemon connected and real test project/task data available, reach S1 project list, S2 project overview, S3 task list, S4 task detail, S5 coordinator and S6 review. Record exact project/task routes and data provenance. Preview is a separate fixture route; do not describe fixture data as native live evidence. If no suitable project or task exists, record the affected screen as unverified instead of creating unrelated work.
5. Visit Workspace, Workstreams, Agents & Sessions and Review & Decisions. Confirm the ConnectionStrip remains visible in every route. Check Back/Forward for canonical navigation and literal-colon and unknown hashes if the native address/developer interface allows it; otherwise record that native history portion unverified.
6. Use Tab and Shift+Tab, activate the skip link, navigate with Enter, and verify visible focus. Expand Workstreams with keyboard arrows and open/close usage disclosure with Enter/Escape. Confirm the selected nav uses aria-current=page. Record any inaccessible controls.
7. Toggle Appearance between light and dark, navigate to another page, and confirm the selected theme persists. Reopen the app and record window size and theme behavior without assuming persistence.
8. Close the owned dev app normally. Record process exit and preserve all evidence. Return an evidence-only report with full source SHA, exact commands/cwd/exits, per-step PASS/FAIL/UNVERIFIED, screenshots, and remaining gaps. Do not edit board.json, push, or change other worktrees. The coordinator owns acceptance.

The 760×600 minimum is the M7-02 decision. OX-X3 in docs/ui/wireframe.md specifies 1100×720; M7-00 must reconcile that documentation separately. Windows WebView2 and Windows npm test/build require their own native run and are not covered here.
