//! Command-line length validation, run before any process is spawned.
//!
//! Windows `CreateProcess` caps a whole command line at 32,767 UTF-16 units
//! (including the terminating NUL), and the OS refuses a longer one with a
//! generic spawn error. A `.cmd` or `.bat` launcher is limited by `cmd.exe`
//! to 8,191 units. Linux caps one argument at 128 KiB and every platform caps
//! the argument and environment block. Checking here turns that into one clear
//! error that names the limit and the way out. Nothing is ever truncated: an
//! objective that does not fit is rejected, because a silently shortened
//! instruction would run an agent on a different task than the one submitted.
//!
//! The Windows count reproduces how Rust's `std::process::Command` builds the
//! command line, so it is exact for a normal executable and conservative for
//! a script. The functions are pure and compile on every platform, so the
//! Windows rules are tested on every host.

use std::path::{Path, PathBuf};

/// `CreateProcessW` command-line limit in UTF-16 units, NUL included.
pub const WINDOWS_COMMAND_LINE_MAX_UNITS: usize = 32_767;
/// `cmd.exe` command-line limit in UTF-16 units, which applies to `.cmd` and
/// `.bat` launchers such as npm's `codex.cmd` shim.
pub const WINDOWS_CMD_SCRIPT_MAX_UNITS: usize = 8_191;
/// Extra units `std` adds when it runs a script through `cmd.exe /d /c "..."`.
const CMD_SCRIPT_OVERHEAD_UNITS: usize = 32;
/// Linux `MAX_ARG_STRLEN`: one argument, NUL included.
pub const UNIX_ARGUMENT_MAX_BYTES: usize = 131_072;

/// A command line that the operating system would refuse to start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the command line for {program} is {length} {unit} long, but this system allows at most {limit}{longest}. \
     Nothing was started and nothing was truncated. Shorten the task objective (put long instructions in a \
     file in the repository and refer to that file by name) and submit it again"
)]
pub struct CommandLineTooLong {
    pub program: PathBuf,
    /// Measured size, in `unit`.
    pub length: usize,
    /// Largest size that is allowed, in `unit`.
    pub limit: usize,
    /// `"characters"` (UTF-16 units) on Windows, `"bytes"` elsewhere.
    pub unit: &'static str,
    /// Names the longest argument when one argument is the cause.
    pub longest: String,
}

/// Checks `program` and `args` against the limit of the platform this build
/// runs on.
pub fn check_command_line(program: &Path, args: &[String]) -> Result<(), CommandLineTooLong> {
    if cfg!(windows) {
        check_windows(program, args)
    } else {
        check_unix(program, args, unix_total_max_bytes())
    }
}

/// Windows rules, applied whatever the host: the whole command line, with the
/// quoting `CreateProcess` needs, must fit.
pub fn check_windows(program: &Path, args: &[String]) -> Result<(), CommandLineTooLong> {
    let is_script = program
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        });
    let (limit, overhead) = if is_script {
        (WINDOWS_CMD_SCRIPT_MAX_UNITS, CMD_SCRIPT_OVERHEAD_UNITS)
    } else {
        (WINDOWS_COMMAND_LINE_MAX_UNITS, 0)
    };
    let length = windows_command_line_units(program, args) + overhead;
    if length <= limit {
        return Ok(());
    }
    Err(CommandLineTooLong {
        program: program.to_path_buf(),
        length,
        limit,
        unit: "characters",
        longest: longest_argument(args, windows_argument_units),
    })
}

/// Unix rules, applied whatever the host: one argument must fit in
/// `UNIX_ARGUMENT_MAX_BYTES` and all arguments in `total_max_bytes`.
pub fn check_unix(
    program: &Path,
    args: &[String],
    total_max_bytes: usize,
) -> Result<(), CommandLineTooLong> {
    let too_long = |length, limit, longest| {
        Err(CommandLineTooLong {
            program: program.to_path_buf(),
            length,
            limit,
            unit: "bytes",
            longest,
        })
    };
    if let Some(longest) = args.iter().map(|arg| arg.len() + 1).max() {
        if longest > UNIX_ARGUMENT_MAX_BYTES {
            return too_long(
                longest,
                UNIX_ARGUMENT_MAX_BYTES,
                longest_argument(args, |arg| arg.len() + 1),
            );
        }
    }
    let total = args.iter().map(|arg| arg.len() + 1).sum::<usize>() + program.as_os_str().len() + 1;
    if total > total_max_bytes {
        return too_long(
            total,
            total_max_bytes,
            longest_argument(args, |arg| arg.len() + 1),
        );
    }
    Ok(())
}

/// Half of the system argument-and-environment limit, which leaves room for
/// the environment the child receives.
fn unix_total_max_bytes() -> usize {
    #[cfg(unix)]
    {
        // SAFETY: sysconf only reads a system constant.
        let reported = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        if let Ok(reported) = usize::try_from(reported) {
            if reported > 0 {
                return reported / 2;
            }
        }
    }
    256 * 1024
}

fn longest_argument(args: &[String], size: impl Fn(&str) -> usize) -> String {
    args.iter()
        .enumerate()
        .max_by_key(|(_, arg)| size(arg))
        .map(|(index, arg)| format!("; argument {} alone is {} of it", index + 1, size(arg)))
        .unwrap_or_default()
}

/// Length of the command line `std::process::Command` builds on Windows for
/// `program` and `args`, in UTF-16 units, including the terminating NUL.
pub fn windows_command_line_units(program: &Path, args: &[String]) -> usize {
    // The program name is always quoted: two quotes around it.
    let program_units = program.to_string_lossy().encode_utf16().count() + 2;
    let args_units: usize = args.iter().map(|arg| 1 + windows_argument_units(arg)).sum();
    program_units + args_units + 1
}

/// Units one argument takes once quoted and escaped so that
/// `CommandLineToArgvW` recovers it unchanged.
fn windows_argument_units(arg: &str) -> usize {
    let quote = arg.is_empty() || arg.contains([' ', '\t']);
    let mut units = if quote { 2 } else { 0 };
    let mut backslashes = 0;
    for unit in arg.encode_utf16() {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else {
            if unit == u16::from(b'"') {
                units += backslashes + 1;
            }
            backslashes = 0;
        }
        units += 1;
    }
    if quote {
        units += backslashes;
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(objective: &str) -> Vec<String> {
        ["exec", "--json", "--approve-for-me", "--", objective]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_windows_count_follows_the_quoting_std_applies() {
        let program = Path::new(r"C:\bin\codex.exe");
        // "C:\bin\codex.exe" (16 + 2) + NUL, no arguments.
        assert_eq!(windows_command_line_units(program, &[]), 19);
        let units = |arg: &str| windows_argument_units(arg);
        assert_eq!(units("plain"), 5);
        assert_eq!(units(""), 2, "an empty argument is quoted");
        assert_eq!(units("two words"), 11, "a space forces quotes");
        assert_eq!(units(r#"say "hi""#), 8 + 2 + 2, "quote chars are escaped");
        assert_eq!(
            units(r"dir\ x\"),
            7 + 2 + 1,
            "trailing backslashes double before the closing quote"
        );
        assert_eq!(units(r"a\b"), 3, "backslashes alone are literal");
        assert_eq!(units("é😀"), 3, "units are UTF-16, so an emoji is two");
        // Separator space plus the argument.
        assert_eq!(
            windows_command_line_units(program, &["a".to_owned(), "b c".to_owned()]),
            19 + 2 + 6
        );
    }

    #[test]
    fn a_65000_character_objective_is_refused_on_windows_with_an_actionable_message() {
        let objective = "x".repeat(65_000);
        let program = Path::new(r"C:\Users\dev\AppData\Roaming\npm\codex.exe");
        let error = check_windows(program, &args(&objective)).unwrap_err();
        assert_eq!(error.limit, WINDOWS_COMMAND_LINE_MAX_UNITS);
        assert!(error.length > 65_000);
        assert_eq!(error.unit, "characters");
        let message = error.to_string();
        assert!(message.contains("32767"), "{message}");
        assert!(message.contains("argument 5"), "{message}");
        assert!(
            message.contains("nothing was truncated") || message.contains("Nothing was truncated")
        );
        assert!(message.contains("Shorten the task objective"), "{message}");
    }

    #[test]
    fn the_windows_limit_is_exact_at_the_boundary() {
        let program = Path::new(r"C:\bin\codex.exe");
        let overhead = windows_command_line_units(program, &args(""));
        // An empty objective costs two units (its quotes), so the largest
        // plain objective that fits is the limit minus the rest.
        let fits = WINDOWS_COMMAND_LINE_MAX_UNITS - (overhead - 2);
        assert!(check_windows(program, &args(&"x".repeat(fits))).is_ok());
        assert!(check_windows(program, &args(&"x".repeat(fits + 1))).is_err());
    }

    #[test]
    fn escaping_counts_toward_the_windows_limit() {
        let program = Path::new(r"C:\bin\codex.exe");
        // 20,000 quotes are 40,000 units once escaped, though only 20,000 chars.
        let objective = "\"".repeat(20_000);
        assert!(check_windows(program, &args(&objective)).is_err());
        assert!(check_windows(program, &args(&"x".repeat(20_000))).is_ok());
    }

    #[test]
    fn a_cmd_script_launcher_has_the_smaller_cmd_exe_limit() {
        let objective = "x".repeat(10_000);
        assert!(check_windows(Path::new(r"C:\bin\codex.exe"), &args(&objective)).is_ok());
        let error = check_windows(Path::new(r"C:\bin\codex.CMD"), &args(&objective)).unwrap_err();
        assert_eq!(error.limit, WINDOWS_CMD_SCRIPT_MAX_UNITS);
        assert!(check_windows(Path::new(r"C:\bin\codex.bat"), &args(&"x".repeat(9_000))).is_err());
        assert!(check_windows(Path::new(r"C:\bin\codex.cmd"), &args(&"x".repeat(7_000))).is_ok());
    }

    #[test]
    fn a_normal_objective_is_accepted_on_every_platform() {
        let objective = "Fix the failing parser test and keep the public API.";
        assert!(check_windows(Path::new("codex"), &args(objective)).is_ok());
        assert!(check_unix(Path::new("codex"), &args(objective), 1 << 20).is_ok());
        assert!(check_command_line(Path::new("codex"), &args(objective)).is_ok());
    }

    #[test]
    fn the_unix_rules_cap_one_argument_and_the_total() {
        let program = Path::new("/usr/bin/codex");
        let fits = "x".repeat(UNIX_ARGUMENT_MAX_BYTES - 1);
        assert!(check_unix(program, &args(&fits), 1 << 20).is_ok());
        let one = "x".repeat(UNIX_ARGUMENT_MAX_BYTES);
        let error = check_unix(program, &args(&one), 1 << 20).unwrap_err();
        assert_eq!(
            (error.limit, error.unit),
            (UNIX_ARGUMENT_MAX_BYTES, "bytes")
        );
        assert!(error.longest.contains("argument 5"), "{error}");
        let many: Vec<String> = (0..10).map(|_| "y".repeat(100_000)).collect();
        assert!(check_unix(program, &many, 500_000).is_err());
        assert!(check_unix(program, &many, 2_000_000).is_ok());
    }

    #[test]
    fn a_too_long_command_line_is_refused_before_any_process_is_started() {
        use crate::supervisor::{ChildSupervisor, LaunchRequest, SupervisorError};
        // Over every platform's limit, and the program does not exist: a
        // spawn attempt would report `Launch`, so `CommandLineTooLong` shows
        // the check ran first.
        let request = LaunchRequest {
            program: PathBuf::from("no-such-program-for-command-line-test"),
            args: args(&"x".repeat(2 * 1024 * 1024)),
            working_dir: std::env::current_dir().unwrap(),
            env_names: Vec::new(),
        };
        match ChildSupervisor::launch(&request) {
            Err(SupervisorError::CommandLineTooLong(error)) => {
                assert!(error.length > 2 * 1024 * 1024);
            }
            Err(other) => panic!("expected CommandLineTooLong, got {other}"),
            Ok(_) => panic!("a command line over every platform limit was launched"),
        }
    }
}
