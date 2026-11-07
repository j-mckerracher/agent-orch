//! Standalone CLI client for the orchestration engine.
//!
//! The engine will run directly in this process; the CLI must work without
//! Tauri or any desktop dependency. No task execution is implemented yet,
//! so any invocation exits nonzero rather than silently succeeding.

fn main() {
    eprintln!("agent-orch: not yet implemented: task execution is not available in this scaffold");
    std::process::exit(1);
}
