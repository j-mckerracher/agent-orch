//! Per-machine prefix for task branch names (`<prefix><task-slug>`).
//!
//! The prefix lives in `<data_dir>/config/branch-prefix`, one line such as
//! `team1/`. A missing, unreadable, or blank file selects [`BranchPrefix::default`]
//! (`moa/`). A value that is not a safe Git ref prefix is an error rather than a
//! silent fallback, so a typo cannot quietly rename every task branch.

use std::fs;
use std::path::Path;

use crate::Error;

/// Prefix used when no `config/branch-prefix` value is set.
pub const DEFAULT_BRANCH_PREFIX: &str = "moa/";

/// Location of the prefix file relative to the data directory.
pub const BRANCH_PREFIX_FILE: &str = "config/branch-prefix";

/// A validated branch-name prefix: one or more `/`-terminated components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchPrefix(String);

impl BranchPrefix {
    /// Validate `value` after trimming surrounding whitespace.
    ///
    /// Accepts ASCII letters, digits, `.`, `_`, `-`, and `/` only. The value
    /// must end in `/`, every component must be non-empty, and no component may
    /// start with `-` or `.` or end in `.lock`, so the result is always a valid
    /// ref prefix and never reads as a Git option.
    pub fn parse(value: &str) -> Result<Self, Error> {
        let value = value.trim();
        let invalid = |reason: &'static str| Error::InvalidBranchPrefix {
            value: value.to_owned(),
            reason,
        };
        let Some(components) = value.strip_suffix('/') else {
            return Err(invalid("must end with `/`"));
        };
        for component in components.split('/') {
            if component.is_empty() {
                return Err(invalid("must not start with `/` or contain `//`"));
            }
            if !component
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return Err(invalid(
                    "may contain only letters, digits, `.`, `_`, `-`, `/`",
                ));
            }
            if component.starts_with(['-', '.']) || component.ends_with(".lock") {
                return Err(invalid(
                    "components must not start with `-` or `.` or end with `.lock`",
                ));
            }
        }
        Ok(Self(value.to_owned()))
    }

    /// Read `<data_dir>/config/branch-prefix`.
    ///
    /// Returns the default when the file is absent, unreadable, or blank.
    pub fn read(data_dir: &Path) -> Result<Self, Error> {
        match fs::read_to_string(data_dir.join(BRANCH_PREFIX_FILE)) {
            Ok(content) if !content.trim().is_empty() => Self::parse(&content),
            _ => Ok(Self::default()),
        }
    }

    /// The prefix, including its trailing `/`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BranchPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Default for BranchPrefix {
    fn default() -> Self {
        Self(DEFAULT_BRANCH_PREFIX.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn data_dir_with(content: Option<&[u8]>) -> TempDir {
        let dir = TempDir::new().unwrap();
        if let Some(content) = content {
            fs::create_dir_all(dir.path().join("config")).unwrap();
            fs::write(dir.path().join(BRANCH_PREFIX_FILE), content).unwrap();
        }
        dir
    }

    #[test]
    fn absent_file_reads_default() {
        let dir = data_dir_with(None);
        assert_eq!(BranchPrefix::read(dir.path()).unwrap().as_str(), "moa/");
    }

    #[test]
    fn file_value_is_read() {
        let dir = data_dir_with(Some(b"team1/\n"));
        assert_eq!(BranchPrefix::read(dir.path()).unwrap().as_str(), "team1/");
    }

    #[test]
    fn surrounding_whitespace_is_stripped() {
        let dir = data_dir_with(Some(b"  dev/ \t\r\n"));
        assert_eq!(BranchPrefix::read(dir.path()).unwrap().as_str(), "dev/");
    }

    #[test]
    fn blank_file_reads_default() {
        let dir = data_dir_with(Some(b" \n"));
        assert_eq!(BranchPrefix::read(dir.path()).unwrap().as_str(), "moa/");
        let dir = data_dir_with(Some(b""));
        assert_eq!(BranchPrefix::read(dir.path()).unwrap().as_str(), "moa/");
    }

    #[test]
    fn unreadable_file_reads_default() {
        // A directory at the file path cannot be read as a string.
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(BRANCH_PREFIX_FILE)).unwrap();
        assert_eq!(BranchPrefix::read(dir.path()).unwrap().as_str(), "moa/");
    }

    #[test]
    fn nested_prefix_is_accepted() {
        assert_eq!(
            BranchPrefix::parse("work/agents/").unwrap().as_str(),
            "work/agents/"
        );
    }

    #[test]
    fn unsafe_values_are_rejected() {
        for value in [
            "team1",
            "/team1/",
            "a//b/",
            "../",
            "a/../b/",
            "-x/",
            ".hidden/",
            "a.lock/",
            "has space/",
            "a:b/",
            "a~1/",
            "a\\b/",
            "a*/",
            "é/",
            "/",
        ] {
            assert!(
                matches!(
                    BranchPrefix::parse(value),
                    Err(Error::InvalidBranchPrefix { .. })
                ),
                "{value:?} should be rejected"
            );
        }
    }

    #[test]
    fn invalid_file_value_is_an_error_not_a_fallback() {
        let dir = data_dir_with(Some(b"no-slash"));
        assert!(matches!(
            BranchPrefix::read(dir.path()),
            Err(Error::InvalidBranchPrefix { .. })
        ));
    }
}
