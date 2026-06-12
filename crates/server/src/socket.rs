use crate::app::App;
use crate::protocol::{Request, Response};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PUMP_INTERVAL: Duration = Duration::from_millis(50);

pub fn default_socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("earworm.sock")
}

pub struct ServerHandle {
    path: PathBuf,
    shutdown: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Bind the JSON-lines control socket and start the accept + pump loop.
/// One thread accepts clients and ticks the app every ~50 ms, broadcasting
/// tick events to subscribed clients; each client gets a reader thread.
pub fn serve(app: Arc<Mutex<App>>, path: &Path) -> std::io::Result<ServerHandle> {
    let _ = std::fs::remove_file(path); // stale socket from a dead process
    let listener = UnixListener::bind(path)?;
    listener.set_nonblocking(true)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let subscribers: Arc<Mutex<Vec<UnixStream>>> = Arc::new(Mutex::new(Vec::new()));

    let thread = {
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            while !shutdown.load(Ordering::SeqCst) {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let app = app.clone();
                            let subs = subscribers.clone();
                            std::thread::spawn(move || client_loop(stream, app, subs));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => break,
                    }
                }
                let events = app.lock().unwrap().tick();
                if !events.is_empty() {
                    let mut lines = String::new();
                    for ev in &events {
                        if let Ok(json) = serde_json::to_string(ev) {
                            lines.push_str(&json);
                            lines.push('\n');
                        }
                    }
                    subscribers
                        .lock()
                        .unwrap()
                        .retain_mut(|s| s.write_all(lines.as_bytes()).is_ok() && s.flush().is_ok());
                }
                std::thread::sleep(PUMP_INTERVAL);
            }
        })
    };

    Ok(ServerHandle {
        path: path.to_path_buf(),
        shutdown,
        thread: Some(thread),
    })
}

fn client_loop(stream: UnixStream, app: Arc<Mutex<App>>, subs: Arc<Mutex<Vec<UnixStream>>>) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    for line in BufReader::new(read_half).lines() {
        let Ok(line) = line else {
            return;
        };
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) if req.cmd == "subscribe" => match writer.try_clone() {
                Ok(clone) => {
                    subs.lock().unwrap().push(clone);
                    Response::ok(req.id, Value::Null)
                }
                Err(e) => Response::err(req.id, format!("subscribe failed: {e}")),
            },
            Ok(req) => app.lock().unwrap().dispatch(req),
            Err(e) => Response::err(0, format!("parse error: {e}")),
        };
        let Ok(mut json) = serde_json::to_string(&resp) else {
            return;
        };
        json.push('\n');
        if writer.write_all(json.as_bytes()).is_err() || writer.flush().is_err() {
            return;
        }
    }
}
