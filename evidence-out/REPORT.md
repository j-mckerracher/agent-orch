# M7 final r1 Windows acceptance rerun — 6c17442

**Result:** BLOCKED at mandatory preflight. No acceptance gate was run.

- Source commit: `6c17442eaee5243b48034dfc53d7f43d25f1ab82`
- Source tree: `9b9612ab6084c644e344603028149084118cc889` (expected and locally verified)
- Rerun runbook branch/tip: `docs/m7-final-r1-rerun-6c17442` / `eea5a3e3e5ab4d700c041c37b31353a5b9d83a41`
- Original runbook branch/tip: `docs/m7-final-r1-windows-acceptance-6c17442` / `003d66790652e8a61da29ef680273f20575dac61`
- Windows: Microsoft Windows NT 10.0.26200.0, build 26200.8655
- Host versions: see `versions.txt`. Acceptance setup was not entered.
- Preflight: C: had 1,884,020,736 bytes free (1.75 GiB). PowerShell `Temp` resolves to the same C: volume. No other filesystem volume was available. The addendum requires at least 20 GiB free on the build/temp volume.
- `CARGO_TARGET_DIR` and `TEST_TMP`: not redirected; no other volume met the requirement.

WINDOWS ACCEPTANCE: INCONCLUSIVE

Section 2 (27-gate harness): NOT RUN; mandatory preflight stopped the rerun before any gate.
Section 3 (release/package/install gates): NOT RUN; mandatory preflight stop.
Section 4 (conversation runtime diagnostic): NOT RUN; mandatory preflight stop.
Section 5 (M7-25 live exchange): NOT RUN; mandatory preflight stop.
Section 6 (M7-26 estimated-cost check): NOT RUN; mandatory preflight stop.
Section 7 (M7-32 UI checklist): NOT RUN; mandatory preflight stop.
Section 8 (N-1 upgrade): NOT RUN; mandatory preflight stop.

No partial gates were run. Gate counts and acceptance results remain unmeasured. Every acceptance item remains UNPROVEN by this rerun. `checkout_clean_after_run`: NOT APPLICABLE; no acceptance source checkout was created. The source pin and tree above were verified from the existing Git object database and advertised remote assignment ref.
