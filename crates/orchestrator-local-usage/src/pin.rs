//! The reader version MCAO supports, and the hashes that prove the installed
//! files are the vetted build. The same JSON file is read by the Windows smoke
//! test in `scripts/usage-reader-smoke.test.mjs`, so there is one source.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const PIN_JSON: &str = include_str!("../codeburn-pin.json");

/// Contents of `codeburn-pin.json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    /// npm package name the reader must have.
    pub name: String,
    /// The only reader version MCAO supports, exactly.
    pub version: String,
    /// Lowest Node.js version the reader runs on (its own launcher checks this too).
    pub node_minimum: String,
    /// npm `dist.integrity` of the vetted tarball (sha512, base64).
    pub tarball_integrity: String,
    /// SHA-256 of each file the daemon relies on, relative to the package root.
    /// `dist/cli.js` is hashed without its first line when that line is a `#!`
    /// interpreter line, because package managers rewrite it (Homebrew does).
    pub files_sha256: BTreeMap<String, String>,
}

/// The pin, parsed once. A malformed pin file is a build defect, so it panics.
pub fn pin() -> &'static Pin {
    static PIN: OnceLock<Pin> = OnceLock::new();
    PIN.get_or_init(|| serde_json::from_str(PIN_JSON).expect("codeburn-pin.json is valid JSON"))
}

/// Path of the file whose hash is taken without its `#!` line.
pub(crate) const SHEBANG_STRIPPED: &str = "dist/cli.js";

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// SHA-256 of a file. `strip_shebang` drops a leading `#!` line first.
pub(crate) fn hash_file(path: &Path, strip_shebang: bool) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    let body = if strip_shebang && bytes.starts_with(b"#!") {
        match bytes.iter().position(|byte| *byte == b'\n') {
            Some(newline) => &bytes[newline + 1..],
            None => &[][..],
        }
    } else {
        &bytes[..]
    };
    Ok(sha256_hex(body))
}

/// `"v22.15.0"` or `"22.15.0"` to `(22, 15, 0)`.
pub(crate) fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let trimmed = text.trim().trim_start_matches('v');
    let mut parts = trimmed.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_names_the_supported_reader() {
        let pin = pin();
        assert_eq!(pin.name, "codeburn");
        assert_eq!(pin.version, "0.9.28");
        assert_eq!(parse_version(&pin.node_minimum), Some((22, 15, 0)));
        assert!(pin.tarball_integrity.starts_with("sha512-"));
        for file in ["dist/cli.js", "dist/main.js", "dist/parse-worker.js"] {
            let hash = pin.files_sha256.get(file).expect("pinned file");
            assert_eq!(hash.len(), 64, "{file} is a SHA-256 hex digest");
        }
    }

    #[test]
    fn sha256_matches_the_standard_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn the_shebang_line_is_ignored_only_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let with = dir.path().join("with.js");
        let without = dir.path().join("without.js");
        std::fs::write(&with, b"#!/usr/bin/env node\nconsole.log(1)\n").unwrap();
        std::fs::write(&without, b"console.log(1)\n").unwrap();
        assert_eq!(
            hash_file(&with, true).unwrap(),
            hash_file(&without, false).unwrap()
        );
        assert_ne!(
            hash_file(&with, false).unwrap(),
            hash_file(&without, false).unwrap()
        );
    }

    #[test]
    fn versions_parse_with_or_without_a_v_prefix() {
        assert_eq!(parse_version("v22.15.0\n"), Some((22, 15, 0)));
        assert_eq!(parse_version("26.13.0"), Some((26, 13, 0)));
        assert_eq!(parse_version("nope"), None);
    }
}
