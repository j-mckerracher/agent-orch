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
mod notifications;
mod updates;

use bridge::{Bridge, HostError, Opened};
use notifications::{
    decision_notifications_get, decision_notifications_notice, decision_notifications_set, Service,
};
use orchestrator_ipc::Capability;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindow, Window, WindowEvent};
use tauri_plugin_window_state::StateFlags;
use updates::{desktop_update_check, desktop_update_install};

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

/// Rebuilds the window from its config; the macOS app outlives its last window.
#[cfg(target_os = "macos")]
fn open_main_window(app: &AppHandle) {
    if let Some(config) = app.config().app.windows.first() {
        let _ = tauri::WebviewWindowBuilder::from_config(app, config)
            .and_then(|builder| builder.build());
    }
}

/// A second launch of an updater-enabled build surfaces the running instance.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        #[cfg(target_os = "macos")]
        open_main_window(app);
    }
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
        } if app.webview_windows().is_empty() => open_main_window(app),
        RunEvent::Exit => {
            if let Some(service) = app.try_state::<Result<Service, String>>() {
                if let Ok(service) = service.inner() {
                    service.stop();
                }
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let updates_enabled = updates::enabled(context.config());
    let mut builder = tauri::Builder::default();
    // First, so a second instance exits before it can start anything.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if updates_enabled {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main_window(app)
        }));
    }
    if updates_enabled {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .manage(Bridge::from_environment())
        .manage(updates::Operation::default())
        .setup(|app| {
            let service = notifications::create_tray(app.handle())
                .map_err(|_| "Unable to create the notification tray.".to_owned())
                .and_then(|()| Service::new(app.handle().clone(), launch::data_dir()))
                .and_then(|service| {
                    service.start()?;
                    Ok(service)
                });
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            daemon_open,
            daemon_send,
            daemon_close,
            decision_notifications_get,
            decision_notifications_set,
            decision_notifications_notice,
            desktop_update_check,
            desktop_update_install
        ])
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Destroyed) {
                window.state::<Bridge>().close_window(window.label());
            }
        })
        .build(context)
        .expect("error while building tauri application")
        .run(on_run_event);
}
