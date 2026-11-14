fn main() {
    // The UI may call only these app commands; capabilities grant them by name.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["daemon_open", "daemon_send", "daemon_close"]),
    ))
    .expect("failed to run tauri-build");
}
