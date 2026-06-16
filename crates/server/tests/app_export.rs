use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::protocol::{Request, Response};
use server::stems::FakeSeparator;
use std::sync::Arc;
use std::time::Duration;

/// 2 s of 440 Hz mono sine at 44.1 kHz (same fixture as the decode tests).
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

fn test_app() -> App {
    App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Arc::new(FakeSeparator),
    )
}

fn dispatch(app: &mut App, cmd: &str, params: Value) -> Response {
    app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    })
}

fn import_fixture(app: &mut App, wav: &std::path::Path) -> i64 {
    let imported = dispatch(app, "song.import", json!({ "path": wav }));
    assert!(imported.ok, "import failed: {:?}", imported.error);
    imported.data["id"].as_i64().unwrap()
}

/// Pump `tick()` until an `export_progress` event reaches a terminal state.
/// Returns the terminal event's `data`.
fn await_export(app: &mut App) -> Value {
    for _ in 0..600 {
        for ev in app.tick() {
            if ev.event == "export_progress" {
                let state = ev.data["state"].as_str().unwrap_or("");
                if matches!(state, "done" | "failed" | "cancelled") {
                    return ev.data;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("export never reached a terminal state");
}

#[test]
fn export_caps_reports_mp3_availability_as_a_bool() {
    let mut app = test_app();
    let resp = dispatch(&mut app, "export.caps", Value::Null);
    assert!(resp.ok, "{:?}", resp.error);
    assert!(resp.data["mp3"].is_boolean(), "caps = {:?}", resp.data);
}

#[test]
fn export_rejects_a_missing_folder() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let mut app = test_app();
    let id = import_fixture(&mut app, &wav);

    let resp = dispatch(
        &mut app,
        "export.start",
        json!({
            "song_id": id,
            "dir": dir.path().join("nope"),
            "filename": "take",
            "format": "wav",
            "rate": 1.0,
            "gains": [],
        }),
    );
    assert!(!resp.ok, "a missing folder must be refused");
    assert!(
        resp.error.unwrap().to_lowercase().contains("folder"),
        "error should name the folder"
    );
}

#[test]
fn export_rejects_a_blank_filename() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let mut app = test_app();
    let id = import_fixture(&mut app, &wav);

    let resp = dispatch(
        &mut app,
        "export.start",
        json!({
            "song_id": id, "dir": dir.path(), "filename": "   ", "format": "wav",
            "rate": 1.0, "gains": [],
        }),
    );
    assert!(!resp.ok, "a blank filename must be refused");
}

#[test]
fn export_rejects_a_filename_with_a_path_separator() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let mut app = test_app();
    let id = import_fixture(&mut app, &wav);

    let resp = dispatch(
        &mut app,
        "export.start",
        json!({
            "song_id": id, "dir": dir.path(), "filename": "sub/take", "format": "wav",
            "rate": 1.0, "gains": [],
        }),
    );
    assert!(!resp.ok, "a filename with a slash must be refused");
}

#[test]
fn export_writes_a_wav_for_the_song() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let mut app = test_app();
    let id = import_fixture(&mut app, &wav);

    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let resp = dispatch(
        &mut app,
        "export.start",
        json!({
            "song_id": id,
            "dir": out,
            "filename": "drill-take",
            "format": "wav",
            "rate": 1.0,
            "semitones": 0.0,
            "cents": 0.0,
            "octave_up": false,
            "bass_focus": false,
            "gains": [],
        }),
    );
    assert!(resp.ok, "export.start failed: {:?}", resp.error);

    let done = await_export(&mut app);
    assert_eq!(done["state"], "done", "export did not finish: {done:?}");
    let path = done["path"].as_str().expect("done event missing path");
    assert!(path.ends_with(".wav"), "unexpected path {path}");
    let bytes = std::fs::metadata(path).unwrap().len();
    assert!(bytes > 0, "exported wav is empty");
}

#[test]
fn export_of_a_span_is_shorter_than_the_whole_song() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav); // 2 s source
    let mut app = test_app();
    let id = import_fixture(&mut app, &wav);
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();

    let base = json!({
        "song_id": id, "dir": out, "format": "wav",
        "rate": 1.0, "semitones": 0.0, "cents": 0.0, "octave_up": false,
        "bass_focus": false, "gains": [],
    });

    let mut whole = base.clone();
    whole["filename"] = json!("whole");
    assert!(dispatch(&mut app, "export.start", whole).ok);
    let whole_bytes = {
        let d = await_export(&mut app);
        std::fs::metadata(d["path"].as_str().unwrap())
            .unwrap()
            .len()
    };

    let mut span = base.clone();
    span["filename"] = json!("span");
    span["start_secs"] = json!(0.0);
    span["end_secs"] = json!(0.5);
    assert!(dispatch(&mut app, "export.start", span).ok);
    let span_bytes = {
        let d = await_export(&mut app);
        std::fs::metadata(d["path"].as_str().unwrap())
            .unwrap()
            .len()
    };

    assert!(
        span_bytes < whole_bytes,
        "0.5 s span ({span_bytes}) should be smaller than the 2 s song ({whole_bytes})"
    );
}
