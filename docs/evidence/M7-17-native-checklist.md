# M7-17 native checklist

Run against the final integration PR head in the native application. Do not substitute jsdom or fixture-only rendering for webview, DPI, screen-reader, or live-daemon checks.

1. **macOS WKWebView — responsive flow and obstruction**
   - Steps: Launch the app on the Workspace route at 900 CSS px height. Set the webview viewport to 1280, 1150, 960, 760, and 480 CSS px wide, one at a time. Expand Device resources; scroll the main content to its end; open and close an available composer and dialog. At each width, inspect the resource strip and the end of the page.
   - Expected: The resource strip stays in normal flow after main content; main content has bottom clearance and remains reachable; the strip does not cover content, composer controls, or dialogs at any width.
   - Evidence: `M7-17-macos-flow-1280.png`, `-1150.png`, `-960.png`, `-760.png`, and `-480.png` (include the measured viewport width in each capture).

2. **Windows WebView2 — 125% and 150% display scaling**
   - Steps: Set Windows Settings → System → Display → Scale to 125%, launch/restart the app, and repeat check 1 at each listed CSS viewport width. Repeat at 150%. Expand Device resources and exercise the same scroll, composer, and dialog states.
   - Expected: No overlap or clipped controls at either scaling factor or breakpoint; all main content and the resource strip remain reachable by scrolling.
   - Evidence: `M7-17-windows-dpi-125-{1280,1150,960,760,480}.png` and `M7-17-windows-dpi-150-{1280,1150,960,760,480}.png` (record actual CSS viewport widths).

3. **Native screen-reader semantics**
   - Steps: On macOS with VoiceOver and Windows with Narrator (or NVDA), navigate to Device resources, expand it, and move through CPU, Memory, GPU, and Disk. Also inspect the collapsed summary. Use a normal live connection and a fixture-mode run.
   - Expected: Each available reading announces its name, value, unit, scope, and observation age. Unavailable and unsupported are announced as distinct text, never as zero or healthy. Fixture values are identified as placeholder data; live mode has no placeholder badge.
   - Evidence: `M7-17-macos-voiceover.txt`, `M7-17-windows-screenreader.txt`, and paired screenshots showing fixture/live mode.

4. **Native live metrics source**
   - Steps: With the native app connected to a real local daemon, open Device resources and compare its displayed readings, units, scope, status, and age with the daemon's current host-metrics response. Confirm the run is not in fixture mode. A real agent process is not required for host metrics.
   - Expected: Live values/statuses reflect the daemon response and no D13 placeholder badge or illustrative fixture value is shown; unsupported host metrics remain explicit text.
   - Evidence: `M7-17-live-metrics.png` and a sanitized timestamped daemon-response excerpt, `M7-17-live-metrics.txt`.
