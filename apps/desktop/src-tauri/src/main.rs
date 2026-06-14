#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod host;

use practice::store::Store;
use server::app::App;
use std::sync::{Arc, Mutex};
use tauri::Manager;

fn db_path() -> std::path::PathBuf {
    // dev/test affordance beside EARWORM_OPEN: point the store elsewhere
    if let Some(p) = std::env::var_os("EARWORM_DB") {
        return std::path::PathBuf::from(p);
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("earworm/earworm.db")
}

/// Capture every panic (any thread) with a backtrace to stderr — which the
/// logging redirect sends to `earworm.log`. A pump-thread panic only kills that
/// thread (and silently wedges the app); this makes such a death loud and
/// diagnosable instead of leaving a frozen window with no trace.
fn install_panic_logger() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!("=== earworm PANIC ===\n{info}\n--- backtrace ---\n{bt}\n=====================");
        default(info);
    }));
}

fn main() {
    server::logging::redirect_if_headless("earworm-desktop");
    install_panic_logger();
    // webkit2gtk's DMA-BUF renderer crashes the Wayland connection on this
    // stack (Hyprland + NVIDIA): "Error 71 (Protocol error) dispatching to
    // Wayland display". Disable it before the webview initializes unless the
    // user has set it themselves.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let db = db_path();
    if let Some(dir) = db.parent() {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("earworm: cannot create data dir {}: {e}", dir.display()));
    }
    let store = Store::open(&db)
        .unwrap_or_else(|e| panic!("earworm: cannot open db {}: {e}", db.display()));
    let engine = engine::Engine::start()
        .unwrap_or_else(|e| panic!("earworm: cannot start audio engine (PipeWire running?): {e}"));
    let app = Arc::new(Mutex::new(App::new(
        store,
        Box::new(engine),
        Box::new(server::capture_control::RealCapture::default()),
        Arc::new(server::stems::DemucsSeparator::default()),
    )));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(host::AppState(app.clone()))
        .invoke_handler(tauri::generate_handler![
            host::dispatch,
            host::initial_song,
            host::quit,
            host::ui_log,
            host::debug_flag
        ])
        .setup(move |tauri_app| {
            let server = host::start_server(tauri_app.handle().clone(), app.clone())?;
            tauri_app.manage(host::SocketState::new(server));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running earworm");
}
