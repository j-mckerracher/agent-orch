# M7-32 Windows manual acceptance checklist

Tester: Windows acceptance runner  
Date: 2026-11-11  
Build: commit `6c17442eaee5243b48034dfc53d7f43d25f1ab82`; tree `9b9612ab6084c644e344603028149084118cc889`  
OS: Windows 11 Education, build 26200  
WebView2: registry runtime `157.0.4341.46` (additional installed entries `157.0.4341.42`, loader `1.3.315.17`)  
Screenshot availability: none. All screenshot cells say `NOT AVAILABLE`; see `screens/SCREENSHOT-ACCESS-BLOCKED.txt` for the exact tool errors.

`PASS` is limited to the environmental or automated fact stated in the row. The visual/manual checklist remains incomplete because no screenshot or UI state could be captured.

| # | Checklist row | Result | Screenshot | Notes |
|---:|---|---|---|---|
| 1 | Windows 11 supported environment and OS build recorded | PASS (environment recorded) | NOT AVAILABLE | Windows 11 Education build 26200. Windows 10 22H2 applicability was not established. |
| 2 | WebView2 runtime version recorded | PASS (version recorded) | NOT AVAILABLE | Registry reports 157.0.4341.46, 157.0.4341.42 and loader 1.3.315.17. |
| 3 | WebView supports `color-mix()` | NOT RUN | NOT AVAILABLE | No browser inspection. |
| 4 | WebView supports `100dvh` | NOT RUN | NOT AVAILABLE | No browser inspection. |
| 5 | WebView supports `:has()` | NOT RUN | NOT AVAILABLE | No browser inspection. |
| 6 | WebView supports `prefers-reduced-motion` | NOT RUN | NOT AVAILABLE | No browser inspection. |
| 7 | WebView supports `prefers-color-scheme` | NOT RUN | NOT AVAILABLE | No browser inspection. |
| 8 | NSIS install | PASS (automated install-smoke) | NOT AVAILABLE | Silent per-user install passed with no elevation prompt. |
| 9 | Installed app launch | PARTIAL / NOT RUN visually | NOT AVAILABLE | The run-specific `agent-orch-desktop.exe` process started (PID 18888); window state and rendered UI could not be captured. |
| 10 | Upgrade | UNPROVEN | NOT AVAILABLE | No earlier different-version package artifact was available. |
| 11 | Uninstall | PASS (automated and manual uninstaller exit 0) | NOT AVAILABLE | App process was absent after uninstall. `manual-install` retained `orchestrator-daemon.exe`; this residual is recorded and was not removed. |
| 12 | Light and dark follow Windows app-mode setting when no preference is stored | NOT RUN | NOT AVAILABLE | App-mode registry values recorded; actual UI response not observed. |
| 13 | Theme toggle persists across restarts | NOT RUN | NOT AVAILABLE | No UI interaction/restart verification. |
| 14 | Aquatic contrast theme and forced-colors visibility | NOT RUN | NOT AVAILABLE | No theme switch or screenshot. |
| 15 | Desert contrast theme and forced-colors visibility | NOT RUN | NOT AVAILABLE | No theme switch or screenshot. |
| 16 | Dusk contrast theme and forced-colors visibility | NOT RUN | NOT AVAILABLE | No theme switch or screenshot. |
| 17 | Night sky contrast theme and forced-colors visibility | NOT RUN | NOT AVAILABLE | No theme switch or screenshot. |
| 18 | Text, borders, focus rings, badges, placeholders and disabled controls remain visible in contrast themes | NOT RUN | NOT AVAILABLE | No visual inspection. |
| 19 | Color is not the only status carrier | NOT RUN | NOT AVAILABLE | No visual inspection. |
| 20 | Layout at widths 1280, 1150, 960, 760, 480 and 390; minimum window size noted | NOT RUN | NOT AVAILABLE | No window resizing. |
| 21 | Display scaling 100%, 125%, 150% and 200%; clipping and scroll checked | NOT RUN | NOT AVAILABLE | DPI/scale could not be changed or visually inspected; `LogPixels` was not measured. |
| 22 | Segoe UI fallback metrics for long titles, IDs and paths | NOT RUN | NOT AVAILABLE | No visual inspection. |
| 23 | Sticky resource strip does not obscure focused controls at widths <=960 | NOT RUN | NOT AVAILABLE | No keyboard or responsive inspection. |
| 24 | Composer is not covered by Activity panel at widths <=960 | NOT RUN | NOT AVAILABLE | No responsive inspection. |
| 25 | First Tab stop is Skip to main content and moves focus to main region | NOT RUN | NOT AVAILABLE | No keyboard interaction. |
| 26 | All controls reachable with Tab/Shift+Tab and visible focus ring in both themes | NOT RUN | NOT AVAILABLE | No keyboard/theme interaction. |
| 27 | Workstream tree supports Up/Down/Home/End | NOT RUN | NOT AVAILABLE | No keyboard interaction. |
| 28 | Target picker opens with Enter and Escape restores trigger focus | NOT RUN | NOT AVAILABLE | No keyboard interaction. |
| 29 | Ctrl+Enter sends a conversation message | NOT RUN | NOT AVAILABLE | No keyboard interaction. |
| 30 | Review Accept/Reject/Request changes work by keyboard; destructive actions confirm | NOT RUN | NOT AVAILABLE | No keyboard interaction. |
| 31 | No keyboard trap in dialogs, menus or Activity panel | NOT RUN | NOT AVAILABLE | No keyboard interaction. |
| 32 | Narrator/NVDA landmarks announce banner, primary navigation, main and complementary regions | NOT RUN | NOT AVAILABLE | Narrator/NVDA not exercised; required screenshot unavailable. |
| 33 | Connection state changes announce Connected, Reconnecting and stale data | NOT RUN | NOT AVAILABLE | No screen-reader interaction. |
| 34 | Mutation outcomes announce created, linked, sent, accepted, rejected and failed | NOT RUN | NOT AVAILABLE | No screen-reader interaction. |
| 35 | PLACEHOLDER, DAEMON and unsupported labels are read with reasons | NOT RUN | NOT AVAILABLE | No screen-reader interaction. |
| 36 | Disabled controls expose reason via `aria-describedby` | NOT RUN | NOT AVAILABLE | No UI/assistive-technology inspection. |
| 37 | Workstream tree announces level, position and expanded/collapsed | NOT RUN | NOT AVAILABLE | No screen-reader interaction. |
| 38 | Route change focus and title behavior | NOT RUN | NOT AVAILABLE | No UI interaction. |
| 39 | Windows Show animations off maps to reduced motion; transitions/ambient background stop | NOT RUN | NOT AVAILABLE | No setting change or visual inspection. |
| 40 | Stop daemon shows Reconnecting/stale data and no fixture data | NOT RUN | NOT AVAILABLE | No live UI state observation; required screenshot unavailable. |
| 41 | Restart daemon shows Connected and refreshed data | NOT RUN | NOT AVAILABLE | No live UI state observation; required screenshot unavailable. |
| 42 | Host relaunches daemon after idle exit | NOT RUN | NOT AVAILABLE | Real host UI path not observed. |
| 43 | Windows named-pipe handshake, reconnect and shutdown on app exit | NOT RUN manually | NOT AVAILABLE | Automated `windows-transport-tests` gate passed (14 passed, 42 filtered); this does not establish the real desktop-host path. |
| 44 | Approval resolve on real candidate; one call per decision and idempotent retry | NOT RUN | NOT AVAILABLE | No real candidate/UI interaction; required screenshot unavailable. |
| 45 | Create/update/link workstream and send message notices; no fixture IDs on wire | NOT RUN | NOT AVAILABLE | No UI/network inspection. |
| 46 | `?fixture=1` mutations never reach daemon | NOT RUN | NOT AVAILABLE | No UI/network inspection. |
| 47 | Live mode contains no illustrative 20/24 slots, $26.00, CPU 41/44 values | NOT RUN | NOT AVAILABLE | No rendered UI inspection. |
| 48 | Host metrics come from Windows daemon; unavailable GPU/CPU states explain why | NOT RUN | NOT AVAILABLE | No rendered UI inspection. |
| 49 | Unsupported actions disabled with reason; PLACEHOLDER only on placeholder data | NOT RUN | NOT AVAILABLE | No rendered UI inspection. |
| 50 | Production CSP applies in Tauri WebView; no eval/external URLs and fonts load | NOT RUN | NOT AVAILABLE | No WebView inspection. |
| 51 | Repository/worktree paths with spaces and non-ASCII work | NOT RUN | NOT AVAILABLE | No UI path test. |

The UI tool failure is a blocker, not a product pass or failure. Automated evidence does not substitute for the screenshot-backed M7-32 rows.
