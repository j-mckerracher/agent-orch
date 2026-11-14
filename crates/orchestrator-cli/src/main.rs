//! Standalone command entry point. Normal invocations parse the public CLI;
//! the private capture mode tees Codex JSONL and stderr while remaining inside
//! the supervised process group. Updated: 2026-11-07T22:31:40-07:00.

use std::ffi::OsStr;
use std::process;

use clap::Parser;
use orchestrator_cli::cli::Cli;

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("__codex_capture"))
    {
        process::exit(orchestrator_cli::workflow::codex_capture_wrapper(
            &args[1..],
        ));
    }
    let cli = Cli::parse_from(std::iter::once(std::ffi::OsString::from("agent-orch")).chain(args));
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("agent-orch: cannot start async runtime: {error}");
            process::exit(1);
        }
    };
    match runtime.block_on(orchestrator_cli::cli::execute(cli)) {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("agent-orch: {error}");
            process::exit(1);
        }
    }
}
