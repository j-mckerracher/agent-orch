# M1-03 Windows verification attempt

- Run date UTC: 2026-11-08T14:04:55Z
- OS and architecture: Microsoft Windows 10.0.26200, x64
- Rust version: rustc 1.96.0 (ac68faa20 2026-05-25)
- Cargo version: cargo 1.96.0 (30a34c682 2026-05-25)
- Source commit: `649c137d7bcfcf2445d2a107162862cde0a1c0ed`
- Command: `cargo test --locked --workspace`
- Result: INCONCLUSIVE (evidence capture invalid)
- Cargo exit code: 0
- Timed out: false
- Process tree required forced termination: false
- Test totals and failures: Not certified; a second wrapper was started against this same output directory before the first invocation had completed.
- Limitations or failure summary: The overlapping wrappers wrote to the same stdout and stderr files. `stderr.bin` contains 1,541 NUL bytes, so the output cannot reliably establish the test totals. The attempt is preserved without replacing its logs; a separate clean attempt follows.
