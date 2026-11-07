# Project Name

Run several AI coding agents in parallel from a single orchestrator: each worker takes a task in its own git worktree, and every pull request comes with a locally generated proof of verification. Built on firstmate, with Claude Code, Codex, and GitHub Copilot CLI as workers.

**What problem it solves:** a short paragraph on the pain this removes, the manual step it
automates, or the capability it exposes. Link out to a design doc or ticket if one exists.

## Status

| | |
|---|---|
| **Maturity** | Experimental |
| **Owned by** | `@mc-innersource/mc-innersource-admin` |
| **Contributions** | Open — see [Contributing](#contributing) |

## Getting Started

### Prerequisites

- Runtime/toolchain and version (e.g. Python 3.11+, Node 20+, Go 1.22+)
- Access or credentials needed (e.g. a service account, VPN, a specific role)
- Anything that must exist first (a database, a cloud project, an API key)

### Install

```bash
git clone https://github.com/mc-innersource/motif-mcao.git
cd motif-mcao
# replace with this project's real install command
```

### Configuration

| Variable | Required | Default | Description |
|---|---|---|---|
| `EXAMPLE_API_URL` | yes | — | Base URL of the upstream service |
| `EXAMPLE_LOG_LEVEL` | no | `info` | Log verbosity |

If this project reads configuration from a `.env` file, commit a `.env.example` alongside it
and list the variables above. Never commit real secrets — see [Security](#security).

## Usage

The shortest path from a clean checkout to a working result:

```bash
# replace with a real, copy-pasteable example
```

Then add one or two realistic examples — the calls people actually make, not a
`hello-world`. Show expected output so readers can tell whether it worked.

## Development

```bash
# replace with this project's real commands
#   test    run the test suite
#   lint    lint and format checks
#   build   produce a build artifact
```

Describe anything non-obvious about the local loop: how to run against a fake vs. real
upstream, how to seed test data, and which tests are slow. Pull requests and pushes to `main` run
the repository's language-aware lint workflow in `.github/workflows/lint.yml` and the central
OPA/Rego controls through `.github/workflows/policy.yml`.

Default-branch policy runs maintain one **Repository policy findings** issue for warnings or
failures. They update it as findings change, close it after a complete clean run, and reopen it
if findings return. PR runs use annotations only. The policy job grants `issues: write` for this
reporting. To opt out, change `repository-policy-with-issues.yml` to `repository-policy.yml` in
`.github/workflows/policy.yml` and remove `issues: write`.

## Project Structure

Illustrative — replace with this project's real layout.

```text
.
├── src/            # application code
├── tests/          # test suite
├── catalog.yml     # Port.io software catalog entity for this repo
└── .github/        # CI workflows, CODEOWNERS, Dependabot configuration
```

## Contributing

This is an innersource project — contributions from anywhere in the organization are
welcome, and you don't need to be on the owning team.

1. Open an issue first for anything non-trivial so we can agree on the approach.
2. Branch from `main` (`feature/short-description`).
3. Keep changes focused, and add or update tests alongside them.
4. Open a pull request describing the *why*, not just the *what*. CI must pass, and a
   [CODEOWNER](.github/CODEOWNERS) review is required before merge.

Good first contributions are labeled `good first issue`. If you're unsure whether an idea
fits, ask in the issue tracker before building it.

## Support & Ownership

| | |
|---|---|
| **Maintainers** | `@mc-innersource/mc-innersource-admin` |
| **Questions & bugs** | [Open an issue](https://github.com/mc-innersource/motif-mcao/issues) |
| **Chat** | *none — open an issue* |
| **Response expectations** | e.g. best-effort within two business days |

## Remaining setup

Two things the automated setup deliberately does not do, because they need a decision or a
permission it does not have. Delete this section once both are done.

- [ ] **Configure the application-specific Dependabot context.** Run
      `/dependabot-app-context` in Claude Code once the app's manifests and CI exist. The reusable
      workflows are already pinned to the reviewed `mc-ci-reusable-workflows` v2.0.1 commit; the
      skill preserves those pins while it writes
      [`.github/ai-app-context.md`](.github/ai-app-context.md).
- [ ] **Allow Actions to open pull requests**, under **Settings → Actions → General**, so
      `/apply-fixes` can work. Check too that companion-PR CI runs when its base is
      `dependabot/**`; a workflow restricted to `branches: [main]` skips it.

## Security

Do not report security issues in public issues or pull requests. Report them through
[internal security contact / process] instead.

## License

State the license or internal-use terms that apply, and add a matching `LICENSE` file — the
template ships neither, because the right answer differs per project.

---

*Generated from
[`mc-innersource/mayo-innersource-template`](https://github.com/mc-innersource/mayo-innersource-template).
`.copier-answers.yml` records which template version — run `copier update` to pull in later
template improvements. Do not delete that file.*
