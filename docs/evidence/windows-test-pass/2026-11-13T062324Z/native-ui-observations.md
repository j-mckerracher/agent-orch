# Native UI observations

## M7-13

The Tauri app opened from the isolated source checkout. Its default daemon data store returned SQLite error code 1546, reported as a disk I/O error, and the daemon exited with code 1. The app displayed `scheduler_state_unreadable` and no cached workstreams. No Windows named-pipe endpoint or real approval candidate appeared. No approve or request-changes action ran. This attempt did not use a fresh isolated daemon data directory and is not a native acceptance pass.

## M7-17

The Tauri app and a real daemon launched with a fresh run-specific data directory at source `350cd9dd993da25072fb7a7f4af6bcdd0a030af0`. Process inspection found the daemon and its named pipe. The Computer Use surface could not read the cursor. It returned `GetCursorPos failed: Access is denied. (0x80070005)` on the initial attempt and the required retry. No more UI actions were run. No metrics values or screenshot were captured. The native metrics check remains UNVERIFIED, and this launch predates the final integration pin `930e0b89f7d44dc6a636f54a135c346991fdb270`.
