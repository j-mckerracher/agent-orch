//! Resolving an agent executable to the one path every probe and launch uses.

use std::path::{Path, PathBuf};

/// Where `program` could be inside `dir`, in preference order.
///
/// On Windows an npm-style install leaves an extensionless `sh` script next to
/// `name.cmd`; the script cannot be started, so a bare name only matches the
/// files Windows can run, `.exe` before `.cmd`.
fn candidates(dir: &Path, program: &Path, windows: bool) -> Vec<PathBuf> {
    let has_extension = program.extension().is_some();
    if !windows || has_extension {
        return vec![dir.join(program)];
    }
    let name = program.to_string_lossy();
    vec![
        dir.join(format!("{name}.exe")),
        dir.join(format!("{name}.cmd")),
    ]
}

/// An explicit path is used as-is; a bare name is resolved against `PATH`.
pub(crate) fn locate_program(program: &Path) -> Option<PathBuf> {
    if program.is_file() {
        return Some(program.to_path_buf());
    }
    let pathvar = std::env::var_os("PATH")?;
    std::env::split_paths(&pathvar)
        .flat_map(|dir| candidates(&dir, program, cfg!(windows)))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_prefers_exe_then_cmd_and_never_the_extensionless_script() {
        let found = candidates(Path::new("bin"), Path::new("codex"), true);
        assert_eq!(
            found,
            vec![
                Path::new("bin").join("codex.exe"),
                Path::new("bin").join("codex.cmd")
            ]
        );
        assert_eq!(
            candidates(Path::new("bin"), Path::new("codex.exe"), true),
            vec![Path::new("bin").join("codex.exe")]
        );
    }

    #[test]
    fn other_hosts_look_for_the_name_itself() {
        assert_eq!(
            candidates(Path::new("bin"), Path::new("codex"), false),
            vec![Path::new("bin").join("codex")]
        );
    }
}
