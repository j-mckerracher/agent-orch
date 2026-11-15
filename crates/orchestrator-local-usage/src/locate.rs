//! Finding the reader on this machine, and the environment it runs in.
//!
//! Windows installs of npm packages leave `name.cmd` shims. A `.cmd` cannot be
//! started without a shell, so the reader is not launched through its shim:
//! the shim's folder is used to find `node_modules\codeburn\dist\cli.js`, and
//! that file is run with `node.exe`. On Unix the `codeburn` entry (a symlink for
//! npm and Homebrew installs) is canonicalised to the same `dist/cli.js`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Folders searched after `PATH`. A daemon started from a desktop app often
/// has a short `PATH` that lacks Homebrew, so the common global folders are
/// checked too. On Windows the npm global folder is the usual one.
pub(crate) fn extra_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(|appdata| vec![PathBuf::from(appdata).join("npm")])
            .unwrap_or_default()
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    }
}

/// `PATH` entries first, then `extra`.
pub(crate) fn search_dirs(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(extra.iter().cloned());
    dirs
}

/// The first existing file named `names` (in order) across `dirs`.
pub(crate) fn find_program(names: &[&str], dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// The command names for the reader and for Node.
pub(crate) fn reader_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["codeburn.cmd"]
    } else {
        &["codeburn"]
    }
}

pub(crate) fn node_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["node.exe"]
    } else {
        &["node"]
    }
}

/// The `dist/cli.js` a found `codeburn` command starts, or `None` when the
/// layout is not an npm package.
pub(crate) fn reader_entry(found: &Path) -> Option<PathBuf> {
    let entry = if cfg!(windows) {
        found
            .parent()?
            .join("node_modules")
            .join("codeburn")
            .join("dist")
            .join("cli.js")
    } else {
        found.canonicalize().ok()?
    };
    let is_entry = entry.file_name()? == "cli.js"
        && entry.parent()?.file_name()? == "dist"
        && entry.parent()?.parent()?.file_name()? == "codeburn";
    (is_entry && entry.is_file()).then_some(entry)
}

/// The environment the reader runs in: an allowlist, plus fixed settings that
/// keep it offline and inside MCAO's own cache. Proxy variables, `NODE_OPTIONS`
/// and anything else are not passed.
pub(crate) fn reader_env(cache_dir: &Path) -> Vec<(OsString, OsString)> {
    const PASS: &[&str] = &[
        "PATH",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        "TEMP",
        "TMP",
        "TMPDIR",
        "TZ",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_CONFIG_DIRS",
        "CODEX_HOME",
    ];
    let mut env: Vec<(OsString, OsString)> = PASS
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
        .collect();
    env.push(("CODEBURN_PRICING_SNAPSHOT_ONLY".into(), "1".into()));
    env.push(("CODEBURN_FX_NO_FETCH".into(), "1".into()));
    env.push((
        "CODEBURN_CACHE_DIR".into(),
        cache_dir.as_os_str().to_owned(),
    ));
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_order_is_path_then_the_global_folders() {
        let dirs = search_dirs(&[PathBuf::from("/opt/extra")]);
        assert_eq!(dirs.last(), Some(&PathBuf::from("/opt/extra")));
    }

    #[test]
    fn the_reader_entry_must_be_an_npm_dist_cli() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("codeburn");
        std::fs::write(&bogus, b"#!/bin/sh\n").unwrap();
        assert_eq!(
            reader_entry(&bogus),
            None,
            "a plain script is not the npm layout"
        );

        let package = dir.path().join("node_modules").join("codeburn");
        let dist = package.join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let cli = dist.join("cli.js");
        std::fs::write(&cli, b"#!/usr/bin/env node\n").unwrap();
        #[cfg(unix)]
        {
            let bin = dir.path().join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            let link = bin.join("codeburn");
            std::os::unix::fs::symlink(&cli, &link).unwrap();
            let found = reader_entry(&link).expect("symlink resolves to dist/cli.js");
            assert_eq!(found.file_name().unwrap(), "cli.js");
        }
        #[cfg(windows)]
        {
            let shim = dir.path().join("codeburn.cmd");
            std::fs::write(&shim, b"@echo off\r\n").unwrap();
            assert_eq!(reader_entry(&shim).as_deref(), Some(cli.as_path()));
        }
    }

    #[test]
    fn the_environment_is_an_allowlist_with_offline_settings() {
        let env = reader_env(Path::new("/cache"));
        let names: Vec<String> = env
            .iter()
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        assert!(!names
            .iter()
            .any(|name| name.contains("PROXY") || name == "NODE_OPTIONS"));
        assert!(env
            .iter()
            .any(|(key, value)| key == "CODEBURN_PRICING_SNAPSHOT_ONLY" && value == "1"));
        assert!(env
            .iter()
            .any(|(key, value)| key == "CODEBURN_FX_NO_FETCH" && value == "1"));
        assert!(env
            .iter()
            .any(|(key, value)| key == "CODEBURN_CACHE_DIR" && value == "/cache"));
    }
}
