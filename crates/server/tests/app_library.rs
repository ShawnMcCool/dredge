use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::Arc;

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
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
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

    // create takes no name — with no sections the server names it from the
    // timestamp fallback
    let l = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "start": 0.0, "end": 1.0}),
    );
    let loop_id = l["id"].as_i64().unwrap();
    assert_eq!(l["name"], "riff 0:00.0–0:01.0");
    assert_eq!(l["name_override"], serde_json::Value::Null);

    // a non-empty name pins a manual override
    let named = req(
        &mut app,
        "loop.update",
        json!({"loop_id": loop_id, "name": "intro"}),
    );
    assert_eq!(named["name"], "intro");
    assert_eq!(named["name_override"], "intro");

    // partial update: move the end only — the override holds, start is kept
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
fn loop_naming_dynamic_override_and_fit() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();

    // two verse sections then a chorus
    req(
        &mut app,
        "section.replace",
        json!({"song_id": id, "sections": [
            {"name": "verse",  "start": 0.0, "end": 1.0, "position": 0},
            {"name": "verse",  "start": 1.0, "end": 2.0, "position": 1},
            {"name": "chorus", "start": 2.0, "end": 3.0, "position": 2}
        ]}),
    );

    // a loop exactly over the 2nd verse → dynamic name "verse 2"
    let l = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "start": 1.0, "end": 2.0}),
    );
    let lid = l["id"].as_i64().unwrap();
    assert_eq!(l["name"], "verse 2");
    assert_eq!(l["name_override"], serde_json::Value::Null);

    // pin then clear → reverts to the dynamic name
    req(
        &mut app,
        "loop.update",
        json!({"loop_id": lid, "name": "my riff"}),
    );
    let cleared = req(&mut app, "loop.update", json!({"loop_id": lid, "name": ""}));
    assert_eq!(cleared["name"], "verse 2");
    assert_eq!(cleared["name_override"], serde_json::Value::Null);

    // a sloppy hand-drawn loop, then fit snaps its edges to section boundaries
    let h = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "start": 1.1, "end": 2.9}),
    );
    let hid = h["id"].as_i64().unwrap();
    assert_eq!(h["name"], "sub verse 2 → sub chorus 1");
    let fit = req(&mut app, "loop.fit", json!({"loop_id": hid}));
    assert_eq!(fit["start"], 1.0);
    assert_eq!(fit["end"], 3.0);
    assert_eq!(fit["name"], "verse 2 → chorus 1");
}

#[test]
fn update_changes_metadata_and_syncs_sidecar() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));
    // create a loop so a sidecar exists to be rewritten
    req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "x", "start": 0.0, "end": 1.0}),
    );

    let updated = req(
        &mut app,
        "song.update",
        json!({"song_id": id, "title": "Renamed", "artist": "New Band"}),
    );
    assert_eq!(updated["title"], "Renamed");
    assert_eq!(updated["artist"], "New Band");

    // persisted in the library list
    let listed = req(&mut app, "song.list", Value::Null);
    assert_eq!(listed[0]["title"], "Renamed");
    // sidecar reflects the new title
    let sc = practice::sidecar::read_sidecar(&wav).unwrap().unwrap();
    assert_eq!(sc.song.title, "Renamed");
    assert_eq!(sc.song.artist.as_deref(), Some("New Band"));

    // a socket/script client may omit `artist` entirely — that clears it
    let cleared = req(
        &mut app,
        "song.update",
        json!({"song_id": id, "title": "Renamed"}),
    );
    assert!(cleared["artist"].is_null());
}

#[test]
fn import_emits_library_changed() {
    let (mut app, _dir, wav) = test_app();
    req(&mut app, "song.import", json!({"path": wav}));
    let events = app.tick();
    assert!(
        events.iter().any(|e| e.event == "library_changed"),
        "expected library_changed in {events:?}"
    );

    // re-import dedupes by hash — nothing changed, nothing announced
    req(&mut app, "song.import", json!({"path": wav}));
    let events = app.tick();
    assert!(!events.iter().any(|e| e.event == "library_changed"));
}

#[test]
fn delete_removes_song_clears_open_and_sweeps_sidecar() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));
    // a loop write produces a sidecar next to the audio file
    req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "x", "start": 0.0, "end": 1.0}),
    );
    assert!(practice::sidecar::read_sidecar(&wav).unwrap().is_some());
    let _ = app.tick(); // drain the import's library_changed

    req(&mut app, "song.delete", json!({"song_id": id}));

    // gone from the library
    let listed = req(&mut app, "song.list", Value::Null);
    assert!(listed.as_array().unwrap().is_empty());
    // open song cleared (status reports a null song_id)
    let status = req(&mut app, "status", Value::Null);
    assert!(status["song_id"].is_null());
    // sidecar swept
    assert!(practice::sidecar::read_sidecar(&wav).unwrap().is_none());
    // the original audio file is untouched
    assert!(wav.exists());
    // library_changed announced
    let events = app.tick();
    assert!(
        events.iter().any(|e| e.event == "library_changed"),
        "expected library_changed in {events:?}"
    );
}

#[test]
fn delete_while_quick_session_clears_active_plan() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));

    // start an ephemeral quick session — sets active_plan + ephemeral
    req(
        &mut app,
        "practice.quick",
        json!({"start": 0.0, "end": 1.0}),
    );
    // sanity: a plan is now active
    let status = req(&mut app, "status", Value::Null);
    assert!(!status["plan"].is_null());

    req(&mut app, "song.delete", json!({"song_id": id}));

    // deleting the open song mid-session tears down the active session so the
    // tick pump can't drive engine commands against the now-unloaded engine
    let status = req(&mut app, "status", Value::Null);
    assert!(status["song_id"].is_null());
    assert!(
        status["plan"].is_null(),
        "active plan should be torn down on delete"
    );
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
