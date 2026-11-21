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

## Give every button feedback

Every button that does something shows what happened (Nielsen heuristics 1, 4 and 9): a pending state while it runs, a specific success, and a specific failure with a way to recover. Use the one shared piece in `apps/desktop/src/ui/action.tsx`:

- `useAction()` runs the action. Pass it a function that resolves with an `Outcome` and may throw. A thrown error is shown as the failure.
- `<ActionStatus action={action} />` goes beside the control, outside any condition that a live update could unmount. It announces the result, clears a success after a few seconds, and keeps a failure with a Retry button.
- `<Button pending={action.pending}>` shows a spinner at the same width and ignores clicks, without disabling the button. Disable the other controls while one runs.

Size the feedback to the action: a short, self-clearing confirmation for a minor one, and a result that stays until replaced, plus an alert on failure, for a destructive or irreversible one. A control that only navigates or toggles something that visibly changes at once needs none of this. `SettingsPages.feedback.test.tsx` fails when a Settings button has no feedback, so add a new button to its table.

## Name branches and commits

- Branch: `type/DPES-NNNN_slug`, for example `feat/DPES-NNNN_short-slug`.
- Commit and PR title: `type(scope): DPES-NNNN - Description`, with a capital letter after the dash.
- User-visible changes use `feat`, `fix`, or `update`. Release-please turns the subject into the changelog entry.

## Open the pull request

Use the [organization PR template](https://github.com/mc-innersource/.github). Fill in the Jira work-item block and list every CI run under Workflow Runs. Open the pull request as ready for review, not as a draft, and do not merge while a check is red.

## Cut a release

Merging the release pull request that release-please opens cuts a release. See [Release process](RELEASE.md).
