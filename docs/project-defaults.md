# Project defaults

Each project has defaults that apply to its tasks. Edit them on the project page under "Defaults for this project", then click Save project defaults.

## Delivery mode

| Mode | What happens to a finished task |
| --- | --- |
| Review only (`local-only`) | It waits for your review on this machine. Nothing leaves it. |
| Pull request, run checks (`direct-pr`) | It is checked, then offered as a pull request you open yourself. |
| Pull request, full verification (`no-mistakes`) | Like `direct-pr`, but the project needs checks and every one must pass first. |

## Base branch

The branch new task worktrees start from. Leave it empty to branch from the current checkout.

## Branch prefix

Task branches start with this prefix. Use letters, digits, `.`, `-`, `_`, and `/`, ending with `/`, up to 64 characters. It cannot start with `orchestrator/`.

Leave it empty to use the machine-wide prefix.

### Machine-wide prefix

Put the prefix on the first line of `config/branch-prefix` in the [data directory](configuration.md#data-directory), for example `team1/`. Without that file it is `moa/`.

## Checks

Checks are commands that must pass before a task is delivered. Enter one per line, up to 8. Each runs without a shell: the program and its arguments are split on spaces, and quotes and variables are not interpreted.

```text
cargo test --locked
npm test
```

## Merge method

Squash, Merge commit, or Rebase. This is the method used when a pull request is merged.

## The home directory

The [home directory](settings.md#home-directory) is a project whose tasks run in its folder, with no worktree and nothing to review or land. These defaults do not apply to it, and its tasks take ReportOnly delivery only.

## When the form is locked

The form is read-only if the running daemon was not started by this app. Close the app and reopen it once the daemon has stopped, about a minute after running work finishes.
