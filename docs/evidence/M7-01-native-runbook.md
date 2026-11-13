# M7-01 native theme checks

Run on the commit containing this runbook. Record `git rev-parse HEAD`, OS version, and results separately for macOS (WKWebView) and Windows (WebView2). Preserve screenshots and errors for failures. These steps require a graphical desktop and the project prerequisites for Tauri.

1. In a terminal at the checkout, run `git rev-parse HEAD`, then `cd apps/desktop`, `npm ci`, and `npm run tauri dev`. Wait for the native application window.
2. Use the theme toggle. Confirm the interface changes between light and dark. Choose dark, close and restart the application with `npm run tauri dev`; confirm dark remains. Repeat with light.
3. To check system following, close the app and clear the `agent-orch.theme` localStorage entry in the webview's developer tools (confirm the actual key in `src/ui/theme/theme.ts` if it changes). Set the OS appearance to light, start the app, and confirm light. Change the OS to dark while it is open and confirm dark. Select light explicitly, then change OS appearance again; confirm the explicit choice remains. Restore the OS appearance after the check.
4. Navigate using Tab and Shift+Tab; activate the theme toggle and disclosure controls with Enter or Space. Confirm the focused control has a clearly visible focus ring in both themes.
5. On Windows, inspect the interface text and confirm a legible system font fallback when Avenir Next is unavailable; check the computed font in WebView2 developer tools where available. On macOS, confirm the same interface is legible with the installed system fonts.
6. Record each result, the commit, OS and webview versions, and any screenshots or errors. Do not mark either platform passed without running its steps on that platform.
