# Install

Install MCAO from a prebuilt installer. To build it yourself, see [Build from source](build-from-source.md).

## Before you install

- Supported platforms: macOS on Apple Silicon (arm64 only) and Windows x64.
- Install and sign in to the agent CLIs you want to use: `codex`, `claude`, `copilot`.
- MCAO finds the CLIs on `PATH`. To use another location, see [Configuration](configuration.md#agent-programs).
- To receive [updates in the app](#update-the-app), install the GitHub CLI (`gh`) and sign in with an account that can read this repository.

## Download

Get the installers from the [releases page](https://github.com/mc-innersource/motif-mcao/releases). See "Assets" for the release you want to install. Each release lists a macOS `.zip`, a Windows `-setup.exe`, and a `SHA256SUMS` file.

## Install on macOS

```sh
unzip MCAO-<version>-macos-arm64-unsigned.zip
mv MCAO.app /Applications/
```

To download and install from the command line instead, use `gh`. A download made this way is not quarantined, so macOS opens it without the [first-launch prompt](#macos).

```sh
gh release download v<version> --repo mc-innersource/motif-mcao \
  --pattern 'MCAO-<version>-macos-arm64-unsigned.zip' --pattern SHA256SUMS
shasum -a 256 -c SHA256SUMS --ignore-missing
ditto -x -k MCAO-<version>-macos-arm64-unsigned.zip .
ditto MCAO.app /Applications/MCAO.app
```

## Install on Windows

Run `MCAO-<version>-windows-x64-unsigned-setup.exe` and follow the installer.

## First launch of an unsigned build

The installers are not code-signed or notarized, so the OS blocks or warns on the first launch. Nobody has verified these prompts on a clean machine yet.

### macOS

Try to open MCAO once. Then open System Settings, Privacy & Security, and click Open Anyway. Alternatively, remove the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/MCAO.app
```

### Windows

SmartScreen shows "Windows protected your PC". Choose More info, then Run anyway.

## Verify a download

Compare the file's SHA-256 digest with its line in `SHA256SUMS`.

```sh
# macOS
shasum -a 256 <file>
```

```powershell
# Windows
Get-FileHash <file>
```

## Update the app

Version 0.1.38 is the first release that can update itself. Install it, or a later release, by hand once. Earlier builds contain no update key and never offer an update.

From then on the app checks GitHub Releases when it starts and every 30 minutes while its window is visible. To check at once, click **Check for updates**. When a newer release exists, the app shows **Install and restart**. The app downloads the update, checks its signature against the key built into the app, replaces itself, and restarts.

Updates need:

- The GitHub CLI (`gh`) on `PATH`. MCAO uses it to find and download updates.
- A `gh` login that can read `mc-innersource/motif-mcao`. Run `gh auth login --hostname github.com`.
- No running agent work. If work is still running, the app shows "Work is still running". Finish it, then try again.

The app stays quiet when a background check fails, for example when `gh` is missing. **Check for updates** shows the error.

## Next step

Continue with [Getting started](getting-started.md).
