//! Bridge between the webview and the shared `server::app::App` dispatcher.
//!
//! The UI is just another client of the same dispatch surface the control
//! socket exposes: one Tauri command in (`dispatch`), one event channel out
//! (`earworm://event`). The tick-pump lives in `server::socket::serve` — the
//! desktop forwards each tick batch to the webview via the `on_events` hook,
//! so there is exactly one pump no matter how many clients are attached.

use server::app::App;
use server::protocol::{Request, Response};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub struct AppState(pub Arc<Mutex<App>>);

/// Keeps the control socket alive for the lifetime of the Tauri app — UI and
/// shell scripts share one session.
pub struct SocketState {
    _handle: server::socket::ServerHandle,
}

impl SocketState {
    pub fn new(handle: server::socket::ServerHandle) -> Self {
        Self { _handle: handle }
    }
}

/// Opt-in diagnostics gate. `EARWORM_DEBUG=1` turns on the dispatch/UI
/// telemetry below; off by default so a normal run is quiet. Read once. The
/// crash/panic hook in `main.rs` is deliberately NOT gated on this — a crash
/// must always leave a trace.
pub fn debug_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("EARWORM_DEBUG").is_some())
}

/// Exposed to the webview so the frontend can match this gate (`trace.ts`).
#[tauri::command]
pub fn debug_flag() -> bool {
    debug_enabled()
}

/// Async + phased: the command runs off the GTK main thread (the window
/// keeps painting while it waits) and heavy commands decode outside the app
/// lock via `dispatch_shared`. `spawn_blocking` keeps the multi-second
/// decodes from tying up the async runtime's worker threads.
#[tauri::command]
pub async fn dispatch(state: tauri::State<'_, AppState>, req: Request) -> Result<Response, String> {
    let app = state.0.clone();
    // Telemetry (EARWORM_DEBUG): time every command and surface slow/failed/
    // panicked ones, so a wedged backend (the spinner-hang suspect) is visible
    // from earworm.log and can be told apart from a frozen webview that never
    // got the response. Off → none of this runs.
    let probe = debug_enabled().then(|| (req.id, req.cmd.clone(), std::time::Instant::now()));
    let res =
        tauri::async_runtime::spawn_blocking(move || server::app::dispatch_shared(&app, req)).await;
    if let Some((id, name, t0)) = probe {
        let dt = t0.elapsed().as_millis();
        match &res {
            Ok(resp) if dt > 800 || !resp.ok => {
                eprintln!("[disp] #{id} {name} -> ok={} {dt}ms", resp.ok)
            }
            Ok(_) => {}
            // a join error here means the command thread PANICKED — the panic
            // hook logs the backtrace; this ties it to the specific command.
            Err(e) => eprintln!("[disp] #{id} {name} -> JOIN ERROR after {dt}ms: {e}"),
        }
    }
    res.map_err(|e| e.to_string())
}

/// Telemetry bridge (EARWORM_DEBUG): the WebKitGTK webview's console isn't
/// reachable from outside, so the UI forwards its traces here and we print them
/// to stderr — they land in earworm.log interleaved with the backend's own
/// traces, giving one timeline. Deliberately does NOT touch the App mutex, so it
/// still works even if the dispatcher is wedged.
#[tauri::command]
pub fn ui_log(line: String) {
    if debug_enabled() {
        eprintln!("{line}");
    }
}

/// Dev affordance: `EARWORM_OPEN=<song id>` opens that song at launch,
/// overriding the remembered last song.
#[tauri::command]
pub fn initial_song() -> Option<i64> {
    std::env::var("EARWORM_OPEN").ok()?.parse().ok()
}

/// Confirmed exit (the exit modal's `exit` button).
#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

/// Start the shared socket + pump; tick events are mirrored to the webview.
pub fn start_server(
    handle: tauri::AppHandle,
    app: Arc<Mutex<App>>,
) -> std::io::Result<server::socket::ServerHandle> {
    let path = server::socket::default_socket_path();
    server::socket::serve(app, &path, move |events| {
        for ev in events {
            let _ = handle.emit("earworm://event", ev);
        }
    })
}
