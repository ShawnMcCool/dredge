// dredged — headless dredge: real engine + control socket.
// Usage: dredged [--socket <path>] [--db <path>]
// Defaults: $XDG_RUNTIME_DIR/dredge.sock, ~/.local/share/dredge/dredge.db

use practice::store::Store;
use server::app::App;
use server::socket::{default_socket_path, serve};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn default_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("dredge/dredge.db")
}

fn parse_args() -> Result<(PathBuf, PathBuf), String> {
    let mut socket = default_socket_path();
    let mut db = default_db_path();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => socket = PathBuf::from(args.next().ok_or("--socket needs a path")?),
            "--db" => db = PathBuf::from(args.next().ok_or("--db needs a path")?),
            "--help" | "-h" => {
                println!("usage: dredged [--socket <path>] [--db <path>]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok((socket, db))
}

fn main() {
    server::logging::redirect_if_headless("dredged");
    let (socket_path, db_path) = match parse_args() {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("dredged: {e}");
            std::process::exit(2);
        }
    };

    if let Some(dir) = db_path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("dredged: cannot create data dir {}: {e}", dir.display());
            std::process::exit(1);
        }
    }
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dredged: cannot open db {}: {e}", db_path.display());
            std::process::exit(1);
        }
    };
    let engine = match engine::Engine::start() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("dredged: cannot start audio engine: {e}");
            std::process::exit(1);
        }
    };

    let app = Arc::new(Mutex::new(App::new(
        store,
        Box::new(engine),
        Arc::new(server::stems::DemucsSeparator::default()),
    )));
    app.lock().unwrap().start_midi();
    let _handle = match serve(app, &socket_path, |_| {}) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("dredged: cannot bind socket {}: {e}", socket_path.display());
            std::process::exit(1);
        }
    };
    eprintln!(
        "dredged: listening on {} (db: {})",
        socket_path.display(),
        db_path.display()
    );

    // Park the main thread; Ctrl-C kills the process, ServerHandle Drop
    // cleanup is best-effort.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
