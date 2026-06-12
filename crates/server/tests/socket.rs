use engine::pipeline::EngineEvent;
use practice::store::Store;
use serde_json::Value;
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::socket::serve;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn start_server(
    name: &str,
) -> (
    server::socket::ServerHandle,
    Arc<Mutex<MockEngine>>,
    std::path::PathBuf,
) {
    let path =
        std::env::temp_dir().join(format!("earworm-test-{}-{name}.sock", std::process::id()));
    let mock = Arc::new(Mutex::new(MockEngine::default()));
    let app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Box::new(MockCapture::default()),
    );
    let handle = serve(Arc::new(Mutex::new(app)), &path, |_| {}).unwrap();
    (handle, mock, path)
}

fn send_line(stream: &mut UnixStream, line: &str) {
    stream.write_all(line.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
}

#[test]
fn request_response_roundtrip() {
    let (_handle, _mock, path) = start_server("roundtrip");
    let mut stream = UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    send_line(&mut stream, r#"{"id":7,"cmd":"song.list"}"#);
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(resp["id"], 7);
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["data"], serde_json::json!([]));
}

#[test]
fn subscribe_receives_events() {
    let (_handle, mock, path) = start_server("subscribe");
    let mut stream = UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    send_line(&mut stream, r#"{"id":1,"cmd":"subscribe"}"#);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(resp["ok"], true);

    mock.lock()
        .unwrap()
        .queued_events
        .push_back(EngineEvent::LoopWrapped);

    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let ev: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(ev["event"], "loop_wrapped");
}

#[test]
fn on_events_hook_receives_tick_events() {
    let path = std::env::temp_dir().join(format!("earworm-test-{}-hook.sock", std::process::id()));
    let mock = Arc::new(Mutex::new(MockEngine::default()));
    let app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Box::new(MockCapture::default()),
    );
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let _handle = serve(Arc::new(Mutex::new(app)), &path, move |events| {
        sink.lock()
            .unwrap()
            .extend(events.iter().map(|e| e.event.clone()));
    })
    .unwrap();

    mock.lock()
        .unwrap()
        .queued_events
        .push_back(EngineEvent::LoopWrapped);

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if seen.lock().unwrap().iter().any(|e| e == "loop_wrapped") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "hook never saw loop_wrapped"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn bad_json_gets_error_response() {
    let (_handle, _mock, path) = start_server("badjson");
    let mut stream = UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    send_line(&mut stream, "not json");
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(resp["ok"], false);
}
