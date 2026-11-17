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

/// Longest prefix a project may hold. Equals the `project_defaults` column bound.
pub const MAX_BRANCH_PREFIX_LEN: usize = 64;

/// Longest branch name `is_safe_branch_name` accepts. Equals the `base_branch` column bound.
pub const MAX_BRANCH_NAME_LEN: usize = 100;

/// Namespace of MCAO's own review refs (`refs/heads/orchestrator/review/`). No prefix may use it.
const RESERVED_NAMESPACE: &str = "orchestrator/";

/// Whether `name` is a branch name safe to pass to Git and to store: 1 to
/// [`MAX_BRANCH_NAME_LEN`] bytes of ASCII letters, digits, `.`, `_`, `-` and `/`
/// only (so no space, control character, `@{`, `~`, `^`, `:` or backslash). No
/// component may be empty, start with `-` or `.`, or end in `.lock`, so the name
/// has no leading dash, no `..`, no leading, trailing or doubled slash, and no
/// trailing `.lock`. A trailing `.` and the name `HEAD` are refused.
pub fn is_safe_branch_name(name: &str) -> bool {
    // `..` is refused anywhere in a ref name; the per-component checks below allow a single `.`.
    if name.is_empty()
        || name.len() > MAX_BRANCH_NAME_LEN
        || name == "HEAD"
        || name.ends_with('.')
        || name.contains("..")
    {
        return false;
    }
    name.split('/').all(|component| {
        !component.is_empty()
            && component
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            && !component.starts_with(['-', '.'])
            && !component.ends_with(".lock")
    })
}

/// A validated branch-name prefix: one or more `/`-terminated components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchPrefix(String);

impl BranchPrefix {
    /// Validate `value` after trimming surrounding whitespace.
    ///
    /// Accepts ASCII letters, digits, `.`, `_`, `-`, and `/` only. The value
    /// must end in `/`, every component must be non-empty, and no component may
    /// start with `-` or `.` or end in `.lock`, so the result is always a valid
    /// ref prefix and never reads as a Git option. This is the rule the
    /// machine-wide prefix file has always followed; a project's prefix goes
    /// through [`BranchPrefix::parse_for_project`], which is stricter.
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

    /// [`BranchPrefix::parse`] plus the rules for a project's own prefix: at most
    /// [`MAX_BRANCH_PREFIX_LEN`] bytes, no `..`, and never inside the
    /// `orchestrator/` namespace, which holds MCAO's review refs.
    pub fn parse_for_project(value: &str) -> Result<Self, Error> {
        let prefix = Self::parse(value)?;
        let invalid = |reason: &'static str| Error::InvalidBranchPrefix {
            value: prefix.0.clone(),
            reason,
        };
        if prefix.0.len() > MAX_BRANCH_PREFIX_LEN {
            return Err(invalid("is too long"));
        }
        if prefix.0.starts_with(RESERVED_NAMESPACE) {
            return Err(invalid("must not use the reserved `orchestrator/` namespace"));
        }
        if prefix.0.contains("..") {
            return Err(invalid("must not contain `..`"));
        }
        Ok(prefix)
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
    fn a_project_prefix_in_the_orchestrator_namespace_is_refused() {
        for value in ["orchestrator/", "orchestrator/review/", "orchestrator/x/"] {
            assert!(
                matches!(
                    BranchPrefix::parse_for_project(value),
                    Err(Error::InvalidBranchPrefix { .. })
                ),
                "{value:?} should be rejected"
            );
        }
        assert!(BranchPrefix::parse_for_project("orchestrator-x/").is_ok());
        assert!(BranchPrefix::parse_for_project("team/orchestrator/").is_ok());
    }

    #[test]
    fn the_machine_wide_prefix_file_still_accepts_what_it_always_did() {
        let dir = data_dir_with(Some(b"orchestrator/\n"));
        assert_eq!(
            BranchPrefix::read(dir.path()).unwrap().as_str(),
            "orchestrator/"
        );
        assert!(BranchPrefix::parse("a..b/").is_ok());
        let long = format!("{}/", "a".repeat(MAX_BRANCH_PREFIX_LEN));
        assert!(BranchPrefix::parse(&long).is_ok());
    }

    #[test]
    fn a_project_prefix_over_the_length_bound_or_with_dots_is_refused() {
        let longest = format!("{}/", "a".repeat(MAX_BRANCH_PREFIX_LEN - 1));
        assert!(BranchPrefix::parse_for_project(&longest).is_ok());
        let too_long = format!("{}/", "a".repeat(MAX_BRANCH_PREFIX_LEN));
        assert!(BranchPrefix::parse_for_project(&too_long).is_err());
        assert!(BranchPrefix::parse_for_project("a..b/").is_err());
    }

    #[test]
    fn safe_branch_names_are_accepted() {
        for name in ["main", "release/1.2", "feat/DPES-1105_x", "a.b", "v1.0.0"] {
            assert!(is_safe_branch_name(name), "{name:?} should be accepted");
        }
        assert!(is_safe_branch_name(&"a".repeat(MAX_BRANCH_NAME_LEN)));
    }

    #[test]
    fn every_unsafe_branch_name_is_rejected() {
        let too_long = "a".repeat(MAX_BRANCH_NAME_LEN + 1);
        for name in [
            "",
            "HEAD",
            "-x",
            "--upload-pack=x",
            "a/-b",
            "a..b",
            "..",
            "a/../b",
            ".hidden",
            "a/.b",
            "a b",
            " a",
            "a\tb",
            "a\nb",
            "a\u{7}b",
            "a\u{7f}b",
            "a@{1}",
            "@{u}",
            "x.lock",
            "a/x.lock",
            "a/",
            "/a",
            "a//b",
            "a.",
            "a~1",
            "a^",
            "a:b",
            "a?",
            "a*",
            "a[b",
            "a\\b",
            "é",
            too_long.as_str(),
        ] {
            assert!(!is_safe_branch_name(name), "{name:?} should be rejected");
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
