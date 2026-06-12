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

#[tauri::command]
pub fn dispatch(state: tauri::State<AppState>, req: Request) -> Response {
    state.0.lock().unwrap().dispatch(req)
}

/// Dev affordance: `EARWORM_OPEN=<song id>` opens that song at launch,
/// overriding the remembered last song.
#[tauri::command]
pub fn initial_song() -> Option<i64> {
    std::env::var("EARWORM_OPEN").ok()?.parse().ok()
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
