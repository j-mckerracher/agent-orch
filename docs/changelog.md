# Changelog

<!-- markdownlint-configure-file { "MD004": false, "MD012": false, "MD024": { "siblings_only": true } } -->

Release-please writes each release's section from the conventional commit subjects that reach `main`.
Do not edit a generated section by hand. See [RELEASE.md](RELEASE.md#changelog).

## [0.1.1](https://github.com/mc-innersource/motif-mcao/compare/v0.1.0...v0.1.1) (2026-11-16)


### Features

* **desktop:** DPES-1105 - Let the model picker choose GitHub Copilot for chat ([89bfc8b](https://github.com/mc-innersource/motif-mcao/commit/89bfc8b2dc6baa9fc1d6f1fe1ff89a72a89bf617))
* **desktop:** DPES-1105 - Let the model picker choose GitHub Copilot for chat ([8b55696](https://github.com/mc-innersource/motif-mcao/commit/8b55696b449071b886fdd38b370811e607d3e326))


### Bug Fixes

* **desktop:** DPES-1105 - Keep not available for uncosted sessions, make the daemon test hermetic ([eada0d2](https://github.com/mc-innersource/motif-mcao/commit/eada0d2619c497855046c6a9aac3bf4ce028512c))
* **desktop:** DPES-1105 - Make the usage pill and the other tools opt-in work ([9587d24](https://github.com/mc-innersource/motif-mcao/commit/9587d24837b4173514b894a50206d3024929d26a))
* **desktop:** DPES-1105 - Make the usage pill and the other tools opt-in work ([ee54560](https://github.com/mc-innersource/motif-mcao/commit/ee54560a968cac2148d0f25796f643be63550bd9))


### Documentation

* **changelog:** DPES-1106 - Generate the changelog from commit subjects ([3c77f64](https://github.com/mc-innersource/motif-mcao/commit/3c77f645996f01d2045c244cfb84b47d5cb0c4c0))
* **design:** DPES-1105 - Design orchestrator actions and firstmate parity ([c37a8af](https://github.com/mc-innersource/motif-mcao/commit/c37a8af1b3d8d47426096dc67fae60e21d582bc1))
* **design:** DPES-1105 - Design orchestrator actions and firstmate parity ([692fbc6](https://github.com/mc-innersource/motif-mcao/commit/692fbc6311e5468c46640f0308f40bd592a5f459))
* **readme:** DPES-1106 - Show the unsigned install notice at the top ([ff144ea](https://github.com/mc-innersource/motif-mcao/commit/ff144ea819a0f6cff4f089c3c5d51d55ebc1448a))
* **readme:** DPES-1106 - Show the unsigned install notice at the top ([88f528e](https://github.com/mc-innersource/motif-mcao/commit/88f528ea07153bfb6db79f8615d65f1a19fb73f6))
* **release:** DPES-1106 - Describe the release-please release flow ([aeb52c0](https://github.com/mc-innersource/motif-mcao/commit/aeb52c04853b66ba44f7fdf0808d91b112e6657a))


### Tests

* **ci:** DPES-1105 - Count the usage controls tests as CI measured them ([518e74d](https://github.com/mc-innersource/motif-mcao/commit/518e74d0fe75ae9dcf7d92030b24ef80c3103b25))


### CI

* **gates:** DPES-1105 - Update expected test counts from CI run 43951278199 ([caaf9ed](https://github.com/mc-innersource/motif-mcao/commit/caaf9ed640bfcc6a0ccc51d7028e39182a9af758))
* **gates:** DPES-1106 - Read the installer version from the app config ([474b0e4](https://github.com/mc-innersource/motif-mcao/commit/474b0e4aec60f6277f8aa3a4b41a0574a1395776))
* **release:** DPES-1106 - Put the version's changelog section in the release notes ([77df643](https://github.com/mc-innersource/motif-mcao/commit/77df643e8bae8e85726509e27ed60e7803c16809))
* **workflows:** DPES-1106 - Open release pull requests with release-please ([fb3be4b](https://github.com/mc-innersource/motif-mcao/commit/fb3be4bb0db36e94a0415a0ef016daf190f52eb4))
* **workflows:** DPES-1106 - Open release pull requests with release-please ([3f54e9f](https://github.com/mc-innersource/motif-mcao/commit/3f54e9f914075d453d0255a5ac856326b9fe386f))

## v0.1.0

- Prevent the model selector from choosing or validating providers when none are launchable.
- Add a per-machine task branch prefix read from `<data_dir>/config/branch-prefix`; the default changes from `orchestrator/` to `moa/` (DPES-681). Existing `orchestrator/*` task branches stay valid for verification.
