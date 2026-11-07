//! Long-lived orchestration daemon.
//!
//! The daemon owns running agent sessions so closing a client window does not
//! kill in-flight work. It composes the workspace crates behind a local IPC
//! server; it launches on demand and is not a system service.
//!
//! Placeholder: process boundary only; it holds no IPC server or engine yet.

fn main() {
    eprintln!("orchestrator-daemon: placeholder; no IPC server or engine yet");
}
