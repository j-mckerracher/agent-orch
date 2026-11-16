# Release process

How maintainers version and release `motif-mcao`. Releases are manual in alpha: a maintainer
bumps the version, merges, and pushes an annotated tag. The tag starts the release build
described in [release-ci.md](release-ci.md). When both platform builds pass and the `release`
environment approves the publish job, the tag becomes a GitHub release with the macOS and Windows
installers attached. A push to `main` and a manual run publish nothing.

## Versioning

- Releases follow [Semantic Versioning](https://semver.org/): `vMAJOR.MINOR.PATCH`, for example
  `v0.1.0`. The tag is the version with a `v` prefix; manifests carry the bare version.
- While the major version is `0`, the product is alpha and any release may break compatibility:
  - bump **MINOR** (`v0.1.0` → `v0.2.0`) for new features or any breaking change;
  - bump **PATCH** (`v0.1.0` → `v0.1.1`) for fixes only.
- From `v1.0.0`: MAJOR for breaking changes, MINOR for compatible features, PATCH for fixes.
- No pre-release or build-metadata suffixes (`-rc.1`, `+build`) in alpha.
- Tags are **annotated** (`git tag -a`), never lightweight, and **immutable**: a published tag is
  never moved or re-created. A mistake is fixed by the next patch release.

## Remotes and lineage

| Remote | URL | Purpose |
| --- | --- | --- |
| `origin` | `https://github.com/mc-innersource/motif-mcao` | The product repository. Branches and release tags are pushed here. |

There is no `upstream` remote. `motif-mcao` is a Rust/Tauri rewrite, not a tracking fork: its
design is derived from [firstmate](https://github.com/kunchenguid/firstmate) (MIT), reference
commit `c15e2b28225e240a5387ff4f31fc084548217eaf`, but it shares no git history with it, so
there are no upstream merges and no `UPSTREAM` file. Record any idea taken from a later firstmate
commit in the changelog entry that adopts it.

## Prerequisites

- `git`, `node`/`npm` and a Rust toolchain matching [README.md](../README.md#building).
- Maintainer rights on `mc-innersource/motif-mcao`, including permission to create `v*` tags
  under the repository's tag ruleset, and permission to approve deployments to the `release`
  environment.
- The repository variable `ALLOW_UNSIGNED_RELEASE` set to `true`. Releases are unsigned today, and a `v*`
  tag build fails without this variable (see [Unsigned releases](release-ci.md#unsigned-releases)).
- `main` is green: the latest `ci.yml` and `audit.yml` runs on `main` passed.

## Version sources

Every release updates these together. They must all hold the same version. The publish job
refuses a tag that is not `v` plus the version in each of the first four files.

| File | Field |
| --- | --- |
| `Cargo.toml` | `[workspace.package] version` (every engine crate inherits it) |
| `apps/desktop/src-tauri/Cargo.toml` | `[package] version` |
| `apps/desktop/src-tauri/tauri.conf.json` | `version` (installer and app version) |
| `apps/desktop/package.json` | `version` |
| `Cargo.lock`, `apps/desktop/src-tauri/Cargo.lock`, `apps/desktop/package-lock.json` | regenerated, never edited by hand |

Check that they agree (run from the repository root):

```bash
grep -m1 '^version' Cargo.toml apps/desktop/src-tauri/Cargo.toml
node -p "['package.json','src-tauri/tauri.conf.json','package-lock.json'].map(f => f + ' ' + require('./apps/desktop/' + f).version).join('\n')"
```

## Release steps

Replace `X.Y.Z` with the new version and `DPES-NNN` with the release's Jira work item.

1. **Bump the version on a branch.**

   ```bash
   git checkout main && git pull --ff-only origin main
   git checkout -b chore/release-vX.Y.Z
   # Edit the version in Cargo.toml and apps/desktop/src-tauri/Cargo.toml, then:
   cargo update --workspace
   cargo update --workspace --manifest-path apps/desktop/src-tauri/Cargo.toml
   (cd apps/desktop && npm version X.Y.Z --no-git-tag-version)
   # Edit the version in apps/desktop/src-tauri/tauri.conf.json.
   ```

   Run the version check above; every line must print `X.Y.Z`.

2. **Update the changelog.** In [changelog.md](changelog.md), rename `## Unreleased` to
   `## vX.Y.Z` and add a new empty `## Unreleased` above it.

3. **Open and merge the release PR** titled `chore(release): DPES-NNN - Release vX.Y.Z`. CI must
   pass before merge.

4. **Tag the merge commit on `main`.**

   ```bash
   git checkout main && git pull --ff-only origin main
   git log -1 --oneline   # confirm this is the release PR's merge commit
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

5. **Approve the release.** The tag push starts `.github/workflows/release.yml`. The build jobs wait
   for approval of the `release` environment, because they read the signing secrets. Once both builds
   pass, the `publish` job waits for a second approval of the same environment. Check the built
   installers before you approve it: approving publishes them. With no required reviewers on the
   environment, neither job waits. Then check the release page: it lists the installers and
   `SHA256SUMS`, and it is a prerelease when any installer is unsigned. See
   [Publishing a release](release-ci.md#publishing-a-release).

6. **Notify consumers.** Tell the maintainers of anything that pins a release (for example the
   `ai-local-setup` install skill's `MOA_VERSION`) to move to `vX.Y.Z`.

## First release (v0.1.0)

The manifests already hold `0.1.0`, and the changelog records `v0.1.0`, so steps 1 to 3 are done
by the PR that introduced this document. After it merges, run step 4 with `X.Y.Z = 0.1.0`:

Releases are unsigned today, so the repository variable `ALLOW_UNSIGNED_RELEASE` must be `true` before
this tag is pushed. The coordinator sets it once the PR that adds the publish job merges; the tag build
fails at the signing check without it. v0.1.0 is then a prerelease whose installers need the steps in its
notes to open on macOS and Windows (see [Unsigned releases](release-ci.md#unsigned-releases)).

```bash
git checkout main && git pull --ff-only origin main
git tag -a v0.1.0 -m "Release v0.1.0: alpha release"
git push origin v0.1.0
```

Then continue with steps 5 and 6.

## Verify a release

```bash
git fetch origin --tags
git cat-file -t vX.Y.Z                       # prints "tag" (annotated, not "commit")
git tag -l --sort=v:refname 'v*'             # releases in semver order
git tag -l --format='%(objecttype) %(refname:short) %(subject)' 'v*'
git describe --tags vX.Y.Z                   # prints "vX.Y.Z"
git show vX.Y.Z:apps/desktop/src-tauri/tauri.conf.json | grep '"version"'
gh release view vX.Y.Z --json tagName,isPrerelease,assets \
  --jq '{tag: .tagName, prerelease: .isPrerelease, assets: [.assets[].name]}'
```

The release lists the two installers and `SHA256SUMS`. It is a prerelease when the build was unsigned.

A consumer pins a release by checking out its tag:

```bash
git fetch origin tag vX.Y.Z --no-tags
git checkout vX.Y.Z
```

## Rollback and mistakes

- **Tag pushed by mistake, and no build was approved and no consumer pinned it.** Cancel the
  `release` run in GitHub Actions, then delete the tag in both places:

  ```bash
  git tag -d vX.Y.Z
  git push origin :refs/tags/vX.Y.Z
  ```

- **Release already built or pinned.** Do not delete or move the tag. Fix forward with the next
  patch release (`vX.Y.(Z+1)`) and tell consumers to skip the bad version.
- **Release published by mistake, and nobody has downloaded it.** Delete the release and keep the
  tag: `gh release delete vX.Y.Z --yes`. A release that someone has downloaded or pinned is fixed
  forward, as above.
- **The publish job failed after it created the release** (for example, an upload failed).
  `gh release create` refuses an existing release, so delete it with
  `gh release delete vX.Y.Z --yes` (the tag stays), then re-run the `publish` job from the run page.
- **Version bump merged but not yet tagged and no longer wanted.** Revert the release PR's merge
  commit with a new PR (`git revert -m 1 <merge-sha>`). No tag cleanup is needed.
- **Wrong commit tagged.** If the tag is still unused, delete it as above and tag the right
  commit. Otherwise fix forward.

Optional: tag signing (`git tag -s`) is allowed but not required in alpha.
