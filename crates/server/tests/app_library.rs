use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::protocol::Request;

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

fn test_app() -> (App, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
    );
    (app, dir, wav)
}

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

#[test]
fn import_then_list_then_open() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    assert_eq!(song["title"], "riff");
    let id = song["id"].as_i64().unwrap();

    let listed = req(&mut app, "song.list", Value::Null);
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let opened = req(&mut app, "song.open", json!({"song_id": id}));
    assert!(!opened["peaks"]["buckets"].as_array().unwrap().is_empty());

    // re-import same path dedupes by hash
    let again = req(&mut app, "song.import", json!({"path": wav}));
    assert_eq!(again["id"].as_i64().unwrap(), id);
}

#[test]
fn sections_autoderive_junctions() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));

    let out = req(
        &mut app,
        "section.replace",
        json!({"song_id": id, "sections": [
            {"name": "A", "start": 0.0, "end": 1.0, "position": 0},
            {"name": "B", "start": 1.0, "end": 2.0, "position": 1},
        ]}),
    );
    let junctions = out["junctions"].as_array().unwrap();
    assert_eq!(junctions.len(), 1);
    let j = &junctions[0];
    assert_eq!(j["name"], "A→B");
    assert_eq!(j["kind"]["kind"], "junction");
    // tail/head 2.0 clamped to section bounds: max(1-2, 0)=0, min(1+2, 2)=2
    assert_eq!(j["start"], 0.0);
    assert_eq!(j["end"], 2.0);

    let loops = req(&mut app, "loop.list", json!({"song_id": id}));
    assert_eq!(loops.as_array().unwrap().len(), 1);
}

#[test]
fn loops_and_plans_roundtrip() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();

    let l = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "intro", "start": 0.0, "end": 1.0}),
    );
    let loop_id = l["id"].as_i64().unwrap();

    let p = req(
        &mut app,
        "plan.save",
        json!({"song_id": id, "name": "daily", "steps": [
            {"step": "play_reps", "loop_id": loop_id, "reps": 3,
             "curve": {"curve": "dwell", "rate": 0.9}},
        ]}),
    );
    assert_eq!(p["name"], "daily");

    let plans = req(&mut app, "plan.list", json!({"song_id": id}));
    assert_eq!(plans.as_array().unwrap().len(), 1);

    // sidecar written next to the audio file and parses
    let sc = practice::sidecar::read_sidecar(&wav).unwrap().unwrap();
    assert_eq!(sc.loops.len(), 1);
    assert_eq!(sc.plans.len(), 1);
}

#[test]
fn loop_update_moves_and_renames() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();

    let l = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "intro", "start": 0.0, "end": 1.0}),
    );
    let loop_id = l["id"].as_i64().unwrap();

    // partial update: move the end only — name and start keep their values
    let moved = req(
        &mut app,
        "loop.update",
        json!({"loop_id": loop_id, "end": 1.5}),
    );
    assert_eq!(moved["name"], "intro");
    assert_eq!(moved["start"], 0.0);
    assert_eq!(moved["end"], 1.5);

    // rename + move together
    let renamed = req(
        &mut app,
        "loop.update",
        json!({"loop_id": loop_id, "name": "verse", "start": 0.25}),
    );
    assert_eq!(renamed["name"], "verse");
    assert_eq!(renamed["start"], 0.25);
    assert_eq!(renamed["end"], 1.5);

    // persisted + mirrored to the sidecar
    let loops = req(&mut app, "loop.list", json!({"song_id": id}));
    assert_eq!(loops[0]["name"], "verse");
    let sc = practice::sidecar::read_sidecar(&wav).unwrap().unwrap();
    assert_eq!(sc.loops[0].name, "verse");

    // unknown loop errors cleanly
    let resp = app.dispatch(Request {
        id: 9,
        cmd: "loop.update".into(),
        params: json!({"loop_id": 999, "end": 2.0}),
    });
    assert!(!resp.ok);
}

#[test]
fn unknown_command_errors() {
    let (mut app, _dir, _wav) = test_app();
    let resp = app.dispatch(Request {
        id: 9,
        cmd: "bogus".into(),
        params: Value::Null,
    });
    assert!(!resp.ok);
    assert!(resp.error.unwrap().contains("unknown"));
}
