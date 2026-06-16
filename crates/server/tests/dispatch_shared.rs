use practice::store::Store;
use serde_json::{json, Value};
use server::app::{dispatch_shared, App};
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};

/// 2 s of 440 Hz mono sine at 44.1 kHz (same pattern as engine's decode test).
fn write_test_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(44_100 * 2) {
        let t = i as f32 / 44_100.0;
        let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        w.write_sample((v * i16::MAX as f32) as i16).unwrap();
    }
    w.finalize().unwrap();
}

fn req(cmd: &str, params: Value) -> Request {
    Request {
        id: 1,
        cmd: cmd.into(),
        params,
    }
}

/// The phased path must produce byte-identical responses to the plain
/// single-lock `App::dispatch` — same payload, same serialization.
#[test]
fn song_open_parity_with_app_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Arc::new(FakeSeparator),
    );

    let imported = app.dispatch(req("song.import", json!({"path": wav})));
    assert!(imported.ok, "{:?}", imported.error);
    let id = imported.data["id"].as_i64().unwrap();

    let direct = app.dispatch(req("song.open", json!({"song_id": id})));
    assert!(direct.ok, "{:?}", direct.error);

    let app = Arc::new(Mutex::new(app));
    let phased = dispatch_shared(&app, req("song.open", json!({"song_id": id})));
    assert_eq!(
        serde_json::to_string(&direct).unwrap(),
        serde_json::to_string(&phased).unwrap(),
    );

    // dedupe parity too: re-import through both paths returns the same song
    let direct = app
        .lock()
        .unwrap()
        .dispatch(req("song.import", json!({"path": wav})));
    let phased = dispatch_shared(&app, req("song.import", json!({"path": wav})));
    assert_eq!(
        serde_json::to_string(&direct).unwrap(),
        serde_json::to_string(&phased).unwrap(),
    );
}
