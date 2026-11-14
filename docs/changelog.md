# Changelog

## Unreleased

- Add a per-machine task branch prefix read from `<data_dir>/config/branch-prefix`; the default changes from `orchestrator/` to `moa/` (DPES-681). Existing `orchestrator/*` task branches stay valid for verification.

## v0.1.0

- Prevent the model selector from choosing or validating providers when none are launchable.
