//! Tauri host for the agent orchestrator.
//!
//! This crate is a thin desktop shell. It must not import `orchestrator-core`
//! or other engine crates for orchestration logic: commands forward to the
//! engine daemon over local IPC, so the UI process staying alive is never a
//! precondition for running agent work.
//!
//! Scaffold: one window and a `ping` command to prove the invoke bridge;
//! real commands are not implemented yet.

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
