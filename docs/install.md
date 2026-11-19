# Install

Install MCAO from a prebuilt installer. To build it yourself, see [Build from source](build-from-source.md).

## Before you install

- Supported platforms: macOS on Apple Silicon (arm64 only) and Windows x64.
- Install and sign in to the agent CLIs you want to use: `codex`, `claude`, `copilot`.
- MCAO finds the CLIs on `PATH`. To use another location, see [Configuration](configuration.md#agent-programs).

## Download

Get the installers from the [releases page](https://github.com/mc-innersource/motif-mcao/releases). See "Assets" for the release you want to install. Each release lists a macOS `.zip`, a Windows `-setup.exe`, and a `SHA256SUMS` file.

## Install on macOS

```sh
unzip MCAO-<version>-macos-arm64-unsigned.zip
mv MCAO.app /Applications/
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

## Next step

Continue with [Getting started](getting-started.md).
