//! Tauri host for the agent orchestrator.
//!
//! A thin desktop shell. It must not import `orchestrator-core` or other
//! engine crates for orchestration logic: commands forward the webview's
//! JSON-RPC to the engine daemon over local IPC, and the daemon keeps running
//! agent work whether or not any window exists.
//!
//! Lifecycle: closing a window closes only that window's connections. On
//! macOS the app stays running after its last window closes and reopening
//! (dock click) makes a new window, which reconnects and reauthenticates. On
//! Windows closing the window exits the app; launching it again reconnects.
//! Neither path asks the daemon to stop.

mod bridge;
mod launch;

use bridge::{Bridge, HostError, Opened};
use orchestrator_ipc::Capability;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindow, Window, WindowEvent};

/// Opens an authenticated daemon connection for the calling window. Frames
/// arrive on `on_frame` as `{kind: "message", message}` and one final
/// `{kind: "closed", reason}`.
#[tauri::command]
async fn daemon_open(
    window: WebviewWindow,
    bridge: State<'_, Bridge>,
    requested: Vec<Capability>,
    on_frame: Channel<Value>,
) -> Result<Opened, HostError> {
    let bridge = bridge.inner().clone();
    let label = window.label().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        bridge.open(&label, &requested, move |payload| {
            on_frame.send(payload).is_ok()
        })
    })
    .await
    .map_err(|_| HostError {
        code: "io",
        message: "the connection task failed".into(),
    })?
}

#[tauri::command]
fn daemon_send(
    window: Window,
    bridge: State<'_, Bridge>,
    connection_id: u64,
    message: Value,
) -> Result<(), HostError> {
    bridge.send(window.label(), connection_id, message)
}

#[tauri::command]
fn daemon_close(window: Window, bridge: State<'_, Bridge>, connection_id: u64) {
    bridge.close(window.label(), connection_id);
}

#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        // Last window closed: keep the app (and its dock icon) alive.
        #[cfg(target_os = "macos")]
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } if app.webview_windows().is_empty() => {
            if let Some(config) = app.config().app.windows.first() {
                let _ = tauri::WebviewWindowBuilder::from_config(app, config)
                    .and_then(|builder| builder.build());
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Bridge::from_environment())
        .invoke_handler(tauri::generate_handler![
            daemon_open,
            daemon_send,
            daemon_close
        ])
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Destroyed) {
                window.state::<Bridge>().close_window(window.label());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(on_run_event);
}
