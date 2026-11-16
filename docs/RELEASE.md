# Release process

How `motif-mcao` is versioned and released. release-please keeps one release pull request open
against `main`. Merging that pull request cuts the release: it creates the `v*` tag and a GitHub
release, and the release run builds the macOS and Windows installers and publishes them. The build
and publish jobs are described in [release-ci.md](release-ci.md).

## How a release happens

1. Merge work to `main` as usual. A merge that carries a conventional commit subject updates the
   release pull request (see [Versioning](#versioning) and [Changelog](#changelog)).
2. The release pull request is titled `chore(release): DPES-1106 - Release X.Y.Z` and comes from the
   branch `release-please--branches--main`. It bumps every version file and writes the version's
   section of `docs/changelog.md`.
3. Review and merge it. It needs one approving review from a code owner, because the `main` ruleset
   requires one. Every push to `main` updates the release pull request and dismisses approvals that
   were given before the push, so approve after the last merge you want in the release.
4. Merging it runs `.github/workflows/release-please.yml`. That run makes the release a draft, creates
   the annotated `vX.Y.Z` tag at the merge commit, and starts `release.yml` on the tag with `publish=true`.
5. `release.yml` runs the full verify (`ci.yml` and `audit.yml`), builds both platforms and publishes
   the release. Plan for about 40 to 60 minutes from the merge to a public release.
6. Check the release page. It lists the installers and `SHA256SUMS`, and it is a prerelease while any
   installer is unsigned.

The release pull request looks unfinished before you merge it, and that is expected. Its runs are
created by the token that release-please uses, so `CI`, `Lint` and `PR Validate` show as awaiting
approval until someone approves them. `PR Validate` fails on it, because release-please generates
its branch name and body and they do not match the pull request template. `main` has no required
status checks, so the failure does not block the merge.

## Versioning

- Releases follow [Semantic Versioning](https://semver.org/): `vMAJOR.MINOR.PATCH`, for example
  `v0.1.1`. The tag is the version with a `v` prefix; the version files carry the bare version.
- While the major version is `0`, a breaking change bumps **MINOR** (`v0.1.1` to `v0.2.0`). A breaking
  change is a `!` after the type (`feat!:`) or a `BREAKING CHANGE:` footer. Every other conventional
  commit that reaches `main` bumps **PATCH**, including `feat`, `fix` and `update`.
- From `v1.0.0`, the standard rules apply: MAJOR for breaking changes, MINOR for `feat`, PATCH for the rest.
- To set the next version yourself, put a `Release-As: X.Y.Z` footer on a commit that reaches `main`
  before the release pull request merges. release-please uses that version.
- There are no pre-release suffixes (`-rc.1`, `+build`) in alpha.
- Tags are **annotated** and **immutable**: a published tag is never moved or re-created. A mistake
  is fixed by the next release.

## Changelog

release-please writes each version's section of `docs/changelog.md` from the conventional commit
subjects that reach `main`. Do not edit a generated section by hand. There is no `Unreleased` section.

The changelog's header sets three markdownlint rules for that file only, to match what release-please
writes: MD012 off (two blank lines before each type heading), MD004 off (`*` list items, while the 0.1.0
section uses `-`) and MD024 `siblings_only` (every release repeats headings such as `### Features`).
Every other rule still applies, and gitleaks still scans the file. Keep the comment in the header, above
the first release section, where release-please leaves it unchanged.

- The commit subject is the changelog line, and it appears with its scope and Jira key as written.
  Write it for a reader of the release notes: `fix(desktop): DPES-1234 - Keep the tray open after a
  restart`. Start the description with a capital letter, as the commit rules require.
- These types appear in the notes: `feat` (Features), `fix` (Bug Fixes), `update` (Updates), `upgrade`
  (Upgrades), `perf` (Performance), `refactor` (Refactoring), `docs` (Documentation), `test` (Tests),
  `build` (Build), `ci` (CI) and `revert` (Reverts). `style` and `chore` do not appear.
- A hidden type still counts toward the bump. A merge whose commits have no conventional subject
  opens no release pull request.
- Merge commits can list a change twice. A pull request merged with a merge commit is read as its
  inner commits plus the merge commit, whose title line is the pull request title. When both carry the
  same conventional subject, the notes show it twice. A squash merge gives one commit and one entry,
  so squash merges are preferred. The ruleset allows them.

## Prerequisites

- Approval rights for the release pull request. Its code owner is `@mc-innersource/mc-innersource-admin`
  (see `.github/CODEOWNERS`).
- The repository variable `ALLOW_UNSIGNED_RELEASE` set to `true`. Releases are unsigned today, and a
  `v*` tag build fails without this variable (see [Unsigned releases](release-ci.md#unsigned-releases)).
- `main` is green before you merge the release pull request: the latest `ci.yml` and `audit.yml` runs
  on `main` passed.

## Version sources

release-please writes these files in the release pull request. They must all hold the same version,
and the publish job refuses a tag that is not `v` plus the version in each of the first four.

| File | Field |
| --- | --- |
| `Cargo.toml` | `[workspace.package] version` (every engine crate inherits it) |
| `apps/desktop/src-tauri/Cargo.toml` | `[package] version` |
| `apps/desktop/src-tauri/tauri.conf.json` | `version` (installer and app version) |
| `apps/desktop/package.json` | `version` |
| `Cargo.lock`, `apps/desktop/src-tauri/Cargo.lock` | the `version` of each local package (the workspace crates and the path dependencies) |
| `apps/desktop/package-lock.json` | the root `version` and the `packages[""]` version |

These are set by the `extra-files` in `release-please-config.json`. Do not edit them by hand on the
release branch: release-please rewrites the release branch on each run, which overwrites a hand edit.
Only a crate or package added later needs a new `extra-files` entry.

Check that they agree (run from the repository root):

```bash
grep -m1 '^version' Cargo.toml apps/desktop/src-tauri/Cargo.toml
node -p "['package.json','src-tauri/tauri.conf.json','package-lock.json'].map(f => f + ' ' + require('./apps/desktop/' + f).version).join('\n')"
```

## Release steps

Step 4 runs by itself. The others are yours.

1. **Merge work to `main`.** Each conventional commit updates the release pull request.
2. **Review the release pull request.** Read the proposed version and the changelog section. The
   changelog is the commit subjects, so a wrong line is fixed by editing the commit subject on its
   pull request before the release merges.
3. **Merge the release pull request** after its approval. The merge is the release decision.
4. **Wait for the release run.** `release-please.yml` creates the tag, and `release.yml` runs under the
   tag. Watch both in the Actions tab.
5. **Check the release page** (see [Verify a release](#verify-a-release)).
6. **Notify consumers.** Tell the maintainers of anything that pins a release (for example the
   `ai-local-setup` install skill's `MOA_VERSION`) to move to the new tag.

## First release after v0.1.0

v0.1.0 was cut by hand. Its tag is the commit `d0a7f6659`, and the manifest holds `0.1.0`. The
`bootstrap-sha` in `release-please-config.json` is that commit, so the first release pull request
collects only the commits after it and proposes `0.1.1`. release-please ignores `bootstrap-sha` once a
release pull request has merged, and it can then be removed.

## Hotfixes and manual releases

- **Hotfix.** Merge a `fix` commit to `main`. Its release pull request proposes the next patch version,
  and merging it ships the fix. There are no release branches, so a fix for an older version ships as
  the next version from `main`.
- **Re-run a failed release.** Re-run the failed job from its run page. If the tag exists but the run
  did not start, run the release by hand with `gh workflow run release.yml --ref vX.Y.Z -f publish=true`.
- **Release without a release pull request.** Only when release-please cannot do it. Create the annotated
  tag at a `main` commit whose version files already hold `X.Y.Z`, then push it:

  ```bash
  git checkout main && git pull --ff-only origin main
  git tag -a vX.Y.Z -m "Release vX.Y.Z"
  git push origin vX.Y.Z
  ```

  The tag push starts `release.yml`, and its publish job creates the release. Do not also merge a release
  pull request for the same version.

## Verify a release

```bash
git fetch origin --tags
git cat-file -t vX.Y.Z                       # prints "tag" (annotated, not "commit")
git tag -l --sort=v:refname 'v*'             # releases in semver order
git tag -l --format='%(objecttype) %(refname:short) %(subject)' 'v*'
git describe --tags vX.Y.Z                   # prints "vX.Y.Z"
git show vX.Y.Z:apps/desktop/src-tauri/tauri.conf.json | grep '"version"'
gh release view vX.Y.Z --json tagName,isPrerelease,isDraft,assets \
  --jq '{tag: .tagName, prerelease: .isPrerelease, draft: .isDraft, assets: [.assets[].name]}'
```

The release lists the two installers and `SHA256SUMS`. It is a prerelease when the build was unsigned.
A `draft: true` release has not been published yet, and the release run is still working on it.

A consumer pins a release by checking out its tag:

```bash
git fetch origin tag vX.Y.Z --no-tags
git checkout vX.Y.Z
```

## Rollback and mistakes

- **Release run failed before publishing.** The release is a draft, so nobody can see it. Fix the cause,
  then re-run the failed job. The draft and the tag are reused, so nothing needs to be deleted.
- **Tag pushed by mistake, and no build was approved and no consumer pinned it.** Cancel the `release`
  run in GitHub Actions, delete the draft release (`gh release delete vX.Y.Z --yes`), then delete the tag
  in both places:

  ```bash
  git tag -d vX.Y.Z
  git push origin :refs/tags/vX.Y.Z
  ```

- **Release already built or pinned.** Do not delete or move the tag. Fix forward with the next release
  (`vX.Y.(Z+1)`) and tell consumers to skip the bad version.
- **Release published by mistake, and nobody has downloaded it.** Delete the release and keep the tag:
  `gh release delete vX.Y.Z --yes`. A release that someone has downloaded or pinned is fixed forward, as above.
- **Release pull request merged with a wrong version.** The tag is created right after the merge. Fix
  forward with the next release. To pick the version before the next merge, put a `Release-As` footer on a
  commit to `main`.
- **Wrong commit tagged.** If the tag is still unused, delete it as above and tag the right commit.
  Otherwise fix forward.

Optional: tag signing (`git tag -s`) is allowed but not required in alpha. release-please's tags are
annotated by the release workflow, and they are not signed.
