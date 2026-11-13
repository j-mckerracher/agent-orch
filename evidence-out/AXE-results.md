Axe-fix branch: orch/0eca0ef2-m7-32-fix-axe
Exact tested commit: 32059d14b4a48da405b003cd2db299bda6694ee6; tree: 4d025a9a3b67d2e2447bf0096fd0051bdeca32c4.
The branch changes AppShell to expose the connection strip inside a named region, updates its focused test, and adds the Windows native-screen-reader checklist row.
AXE-G01 npm ci: exit 0. AXE-G02 AppShell.test.tsx: 1 file, 53 tests passed. AXE-G03 full desktop suite: 52 files, 1,253 tests passed. AXE-G04 build: exit 0; Vite emitted its >500 kB chunk advisory.
The branch worktree remained clean. Native WebView2/screen-reader and DPI verification remain BLOCKED because P13 could not confirm an active desktop session (query.exe is unavailable); this automated evidence does not prove the native accessibility fix.
