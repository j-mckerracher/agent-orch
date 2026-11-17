# Contributing

Read this before you open a pull request. Build steps are in [Build from source](build-from-source.md).

## Run the tests

From the repository root:

```sh
cargo test --locked --workspace
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/desktop test
npm --prefix apps/desktop run test:e2e
npm --prefix apps/desktop run test:browser
node --test "scripts/**/*.test.mjs"
```

`test:e2e` runs against a real daemon over a Unix socket, so CI runs it on macOS only. `test:browser` runs Playwright layout checks.

## Run the CI gates locally

```sh
node scripts/ci/run-gates.mjs --list
```

This prints the gates for your platform. A gate passes only when its command exits 0 and its output has the expected counts and names, which live in `scripts/ci/expected.mjs`. When a change adds or removes tests, update them in the same pull request from real CI logs. Details are in [Release CI](release-ci.md).

## Run the security scan

Run Trivy from the repository root before you open or update a pull request. It must exit 0.

```sh
trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .
```

### If the scan fails

Fix the root cause of a finding. Do not suppress findings or lower the severity without agreement from the maintainers. Treat any secret the scan finds as compromised and rotate it.

## Name branches and commits

- Branch: `type/DPES-NNNN_slug`, for example `feat/DPES-NNNN_short-slug`.
- Commit and PR title: `type(scope): DPES-NNNN - Description`, with a capital letter after the dash.
- User-visible changes use `feat`, `fix`, or `update`. Release-please turns the subject into the changelog entry.

## Open the pull request

Use the [organization PR template](https://github.com/mc-innersource/.github). Fill in the Jira work-item block and list every CI run under Workflow Runs. Open the pull request as ready for review, not as a draft, and do not merge while a check is red.

## Cut a release

Merging the release pull request that release-please opens cuts a release. See [Release process](RELEASE.md).
