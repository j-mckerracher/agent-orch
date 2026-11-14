# M7-21 native checklist

Run against the final M7-21 integration head in the native application with a real local daemon. Record the full source SHA. Do not substitute fixtures or jsdom for these checks. Use the comparison tolerances in [`host-metrics.md`](../host-metrics.md).

1. **macOS live readings and observation age**
   - Steps: Open the native app with a live daemon connection (not fixture mode), expand Device resources, and compare the displayed CPU, memory, disk, and supported GPU readings with a contemporaneous `host.metrics` response and the OS monitors listed in `host-metrics.md`. Check units, scopes, states, and that the displayed age corresponds to each `observed_at` timestamp.
   - Expected: Values agree within the documented tolerances; unsupported/unavailable readings remain explicit text; no D13 placeholder badge or fixture values appear.
   - Evidence: Screenshot of the expanded strip and sanitized timestamped `host.metrics` response, including tested source SHA.

2. **Windows live readings and unsupported GPU**
   - Steps: Repeat check 1 on Windows, comparing CPU, memory, and disk with Task Manager and the daemon response. Inspect the GPU row.
   - Expected: Readings and observation age reflect the daemon response; GPU remains explicitly unsupported text (Windows has no vetted GPU source); no fabricated GPU value, placeholder badge, or fixture value appears.
   - Evidence: Screenshot and sanitized timestamped `host.metrics` response, including tested source SHA.

3. **Native webview visibility pauses polling**
   - Steps: On macOS and Windows, record `host.metrics` dispatch timestamps from a daemon-side RPC trace while the app is visible. Hide/minimize the app for at least two `sample_interval_ms` periods, then restore it and continue recording.
   - Expected: No new `host.metrics` dispatch occurs while hidden; a fresh request occurs when the app becomes visible; displayed observation age catches up from the source timestamp.
   - Evidence: Timestamped dispatch trace and before/after screenshots for each OS.
