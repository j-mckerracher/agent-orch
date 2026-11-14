//! Path form handed to system Git on Windows.
//!
//! `std::fs::canonicalize` returns verbatim paths (`\\?\C:\...`). Git for
//! Windows parses a path argument as `//?/C:/...` and its `mkdir` guard
//! (`is_valid_win32_path` in compat/mingw.c) rejects `?` and `:` in a path
//! component, so `git worktree add` fails with
//! `could not create leading directories of '//?/C:/.../.git': Invalid argument`.
//! Plain drive and UNC paths are accepted. The plain path names the same
//! file, so a verbatim path is rewritten only when that is provably safe.

use std::path::{Path, PathBuf};

/// Longest plain path produced: below `MAX_PATH` with room for `\.git`.
/// Longer verbatim paths stay verbatim; Git then reports its own error.
const MAX_PLAIN_LEN: usize = 240;

/// `path` in the form Git for Windows accepts. A no-op off Windows.
pub(crate) fn for_git(path: &Path) -> PathBuf {
    if cfg!(windows) {
        if let Some(plain) = path.to_str().and_then(simplify_verbatim) {
            return PathBuf::from(plain);
        }
    }
    path.to_path_buf()
}

/// `\\?\C:\a\b` becomes `C:\a\b` and `\\?\UNC\srv\share\a` becomes
/// `\\srv\share\a`. Returns `None` (keep the verbatim path) when any
/// component would mean something else to the Win32 path parser: `.` or
/// `..`, `/`, trailing dot or space, reserved device names, illegal
/// characters, an empty component, or when the result is too long.
/// Other verbatim forms (`\\?\Volume{..}`, `\\?\GLOBALROOT`) are never touched.
fn simplify_verbatim(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    let bytes = rest.as_bytes();
    let (plain, tail) = if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
    {
        (rest.to_owned(), &rest[3..])
    } else if rest.len() > 4 && rest.is_char_boundary(4) && rest[..4].eq_ignore_ascii_case(r"UNC\")
    {
        let tail = &rest[4..];
        // A UNC root needs both a server and a share.
        if tail
            .split('\\')
            .take(2)
            .filter(|part| !part.is_empty())
            .count()
            < 2
        {
            return None;
        }
        (format!(r"\\{tail}"), tail)
    } else {
        return None;
    };
    let components = tail.strip_suffix('\\').unwrap_or(tail);
    if !components.is_empty() && !components.split('\\').all(is_plain_component) {
        return None;
    }
    (plain.len() < MAX_PLAIN_LEN).then_some(plain)
}

fn is_plain_component(name: &str) -> bool {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"|?*/".contains(c))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let (head, digit) = stem.as_bytes().split_at(stem.len().min(3));
    const RESERVED: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    let numbered = head.len() == 3
        && digit.len() == 1
        && (head.eq_ignore_ascii_case(b"COM") || head.eq_ignore_ascii_case(b"LPT"))
        && matches!(digit[0], b'1'..=b'9');
    !numbered && !RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_drive_and_unc_paths_become_plain_paths() {
        let cases = [
            (
                r"\\?\C:\Users\me\data\worktrees\accept-run",
                r"C:\Users\me\data\worktrees\accept-run",
            ),
            (r"\\?\c:\a b\é", r"c:\a b\é"),
            (r"\\?\C:\", r"C:\"),
            (r"\\?\UNC\srv\share\a\b", r"\\srv\share\a\b"),
            (r"\\?\unc\srv\share", r"\\srv\share"),
        ];
        for (verbatim, plain) in cases {
            assert_eq!(
                simplify_verbatim(verbatim).as_deref(),
                Some(plain),
                "{verbatim}"
            );
            assert_eq!(simplify_verbatim(plain), None, "already plain: {plain}");
        }
    }

    #[test]
    fn verbatim_paths_with_other_meaning_are_left_alone() {
        let long = format!(r"\\?\C:\{}", "a".repeat(MAX_PLAIN_LEN));
        let keep = [
            r"C:\plain\path",
            r"\\srv\share\a",
            r"\\.\C:\a",
            r"\\?\C:",
            r"\\?\C:a",
            r"\\?\UNC\srv",
            r"\\?\UNC\srv\",
            r"\\?\Volume{0000}\a",
            r"\\?\GLOBALROOT\Device\x",
            r"\\?\C:\a\..\b",
            r"\\?\C:\a\.\b",
            r"\\?\C:\a\\b",
            r"\\?\C:\a/b",
            r"\\?\C:\a\b.",
            r"\\?\C:\a\b ",
            r"\\?\C:\a\b:stream",
            r"\\?\C:\a\b?c",
            r"\\?\C:\a\CON",
            r"\\?\C:\a\con.txt",
            r"\\?\C:\a\Com1",
            r"\\?\C:\a\lpt9 .x",
            r"\\?\C:\a\NUL",
            r"\\?\C:\a\CONOUT$",
            &long,
        ];
        for path in keep {
            assert_eq!(simplify_verbatim(path), None, "{path}");
        }
    }

    #[test]
    fn near_reserved_names_are_plain() {
        for name in [
            "CONSOLE", "COM0", "COM10", "LPT", "auxx", "nul0", "comma", "ééé", "éé",
        ] {
            let path = format!(r"\\?\C:\a\{name}");
            assert!(simplify_verbatim(&path).is_some(), "{name}");
        }
    }

    #[test]
    fn for_git_is_a_no_op_off_windows() {
        let path = Path::new(r"\\?\C:\a\b");
        assert_eq!(for_git(path) == path, !cfg!(windows));
    }
}
