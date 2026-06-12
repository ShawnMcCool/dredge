use engine::capture::CaptureNode;
use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::Arc;

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

fn two_nodes() -> Vec<CaptureNode> {
    vec![
        CaptureNode {
            id: 51,
            serial: 10051,
            app: "Spotify".into(),
            media: "Some Song".into(),
        },
        CaptureNode {
            id: 73,
            serial: 10073,
            app: "Firefox".into(),
            media: "".into(),
        },
    ]
}

#[test]
fn nodes_and_status_roundtrip() {
    let mock = MockCapture {
        nodes: two_nodes(),
        filled_secs: 12.5,
        ..Default::default()
    };
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Box::new(mock),
        Arc::new(FakeSeparator),
    );

    let nodes = req(&mut app, "capture.nodes", Value::Null);
    let nodes = nodes.as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["id"], 51);
    assert_eq!(nodes[0]["app"], "Spotify");
    assert_eq!(nodes[0]["media"], "Some Song");
    assert_eq!(nodes[1]["app"], "Firefox");

    let status = req(&mut app, "capture.status", Value::Null);
    assert_eq!(status["running"], false);

    req(&mut app, "capture.start", json!({"node_id": 51}));
    let status = req(&mut app, "capture.status", Value::Null);
    assert_eq!(status["running"], true);
    assert_eq!(status["filled_secs"], 12.5);
    assert_eq!(status["app"], "Spotify");
    assert_eq!(status["media"], "Some Song");

    req(&mut app, "capture.stop", Value::Null);
    let status = req(&mut app, "capture.status", Value::Null);
    assert_eq!(status["running"], false);
}

#[test]
fn grab_writes_wav_and_imports() {
    // 1 s of 440 Hz sine, interleaved stereo at 48 kHz
    let snapshot: Vec<f32> = (0..48_000)
        .flat_map(|i| {
            let t = i as f32 / 48_000.0;
            let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            [v, v]
        })
        .collect();
    let mock = MockCapture {
        nodes: two_nodes(),
        snapshot_buf: snapshot,
        filled_secs: 1.0,
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Box::new(mock),
        Arc::new(FakeSeparator),
    );
    app.set_captures_dir(dir.path().to_path_buf());

    req(&mut app, "capture.start", json!({"node_id": 51}));
    let song = req(&mut app, "capture.grab", json!({"last_secs": 1}));
    assert_eq!(song["title"], "Spotify — Some Song");

    let path = std::path::PathBuf::from(song["path"].as_str().unwrap());
    assert!(path.exists(), "wav not written: {}", path.display());
    assert!(path.starts_with(dir.path()));
    assert_eq!(path.extension().unwrap(), "wav");

    // the imported capture decodes like any library song
    let id = song["id"].as_i64().unwrap();
    let opened = req(&mut app, "song.open", json!({"song_id": id}));
    assert!(!opened["peaks"]["buckets"].as_array().unwrap().is_empty());
}

#[test]
fn grab_with_no_capture_errors() {
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    );
    let resp = app.dispatch(Request {
        id: 1,
        cmd: "capture.grab".into(),
        params: json!({"last_secs": 5}),
    });
    assert!(!resp.ok);
}
