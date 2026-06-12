use engine::pipeline::{EngineCmd, EngineEvent};
use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::{Event, Request};
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};
use time::macros::format_description;

type SharedMock = Arc<Mutex<MockEngine>>;

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

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

fn req_err(app: &mut App, cmd: &str, params: Value) -> String {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(!resp.ok, "{cmd} unexpectedly succeeded: {:?}", resp.data);
    resp.error.unwrap()
}

/// App over a file-backed store with the test WAV imported and open —
/// no loops, no plans: quick practice needs neither.
fn quick_app() -> (App, SharedMock, tempfile::TempDir, i64) {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let db = dir.path().join("test.db");
    let mock: SharedMock = Arc::new(Mutex::new(MockEngine::default()));
    let mut app = App::new(
        Store::open(&db).unwrap(),
        Box::new(mock.clone()),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    );
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));
    mock.lock().unwrap().sent.clear();
    (app, mock, dir, id)
}

fn wrap_tick(app: &mut App, mock: &SharedMock) -> Vec<Event> {
    mock.lock()
        .unwrap()
        .queued_events
        .push_back(EngineEvent::LoopWrapped);
    app.tick()
}

/// Drive the 8-rep quick session (2 listen + 6 play) to completion.
fn finish_session(app: &mut App, mock: &SharedMock) -> Vec<Vec<Event>> {
    (0..8).map(|_| wrap_tick(app, mock)).collect()
}

fn reps_count(dir: &tempfile::TempDir) -> i64 {
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    conn.query_row("SELECT COUNT(*) FROM reps", [], |row| row.get(0))
        .unwrap()
}

fn fmt(d: time::Date) -> String {
    d.format(format_description!("[year]-[month]-[day]"))
        .unwrap()
}

#[test]
fn quick_starts_listen_first_immediately() {
    let (mut app, mock, _dir, _song_id) = quick_app();
    req(
        &mut app,
        "practice.quick",
        json!({"start": 0.5, "end": 1.5}),
    );
    let sent = mock.lock().unwrap().sent.clone();
    assert_eq!(
        sent[sent.len() - 4..],
        [
            EngineCmd::SetLoopSecs {
                start: 0.5,
                end: 1.5
            },
            EngineCmd::SetRate(1.0),
            EngineCmd::Mute(false),
            EngineCmd::Play,
        ]
    );
    let status = req(&mut app, "status", Value::Null);
    assert_eq!(status["plan"]["mode"], "listen");
    assert_eq!(status["plan"]["plan_id"], 0);
}

#[test]
fn quick_session_progression_and_no_rep_rows() {
    let (mut app, mock, dir, song_id) = quick_app();
    req(
        &mut app,
        "practice.quick",
        json!({"start": 0.5, "end": 1.5}),
    );

    let ticks = finish_session(&mut app, &mock);

    let rep_changes: Vec<Value> = ticks
        .iter()
        .flatten()
        .filter(|e| e.event == "rep_changed")
        .map(|e| e.data.clone())
        .collect();
    // rep 1 (listen) is applied at start; 7 transitions follow
    assert_eq!(rep_changes.len(), 7);
    let modes: Vec<&str> = rep_changes
        .iter()
        .map(|d| d["mode"].as_str().unwrap())
        .collect();
    assert_eq!(
        modes,
        ["listen", "play", "play", "play", "play", "play", "play"]
    );
    let rates: Vec<f64> = rep_changes
        .iter()
        .map(|d| d["rate"].as_f64().unwrap())
        .collect();
    // listen at 1.0, then oscillate(0.7, 1.0, 3) over 6 play reps
    assert_eq!(rates, [1.0, 0.7, 0.7, 1.0, 0.7, 0.7, 1.0]);
    let last_tick = ticks.last().unwrap();
    assert!(last_tick.iter().any(|e| e.event == "plan_finished"));
    assert_eq!(mock.lock().unwrap().sent.last(), Some(&EngineCmd::Pause));

    // nothing persisted: no rep rows, no loops, nothing scheduled
    assert_eq!(reps_count(&dir), 0);
    let loops = req(&mut app, "loop.list", json!({"song_id": song_id}));
    assert!(loops.as_array().unwrap().is_empty());
    let due = req(&mut app, "due.list", Value::Null);
    assert!(due.as_array().unwrap().is_empty());
}

#[test]
fn quick_rate_persists_loop_rep_and_schedule() {
    let (mut app, mock, dir, song_id) = quick_app();
    req(
        &mut app,
        "practice.quick",
        json!({"start": 0.5, "end": 1.5}),
    );
    finish_session(&mut app, &mock);

    let out = req(&mut app, "practice.quick_rate", json!({"rating": "solid"}));
    assert_eq!(out["loop"]["name"], "riff 0:00.5–0:01.5");
    assert_eq!(out["interval_idx"], 0);
    let today = time::OffsetDateTime::now_utc().date();
    assert_eq!(out["due_on"], fmt(today + time::Duration::days(1)));

    let loops = req(&mut app, "loop.list", json!({"song_id": song_id}));
    let loops = loops.as_array().unwrap();
    assert_eq!(loops.len(), 1);
    assert_eq!(loops[0]["name"], "riff 0:00.5–0:01.5");

    // exactly one rep row: the rated play rep against the persisted loop
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    let (loop_id, mode, rating): (i64, String, String) = conn
        .query_row("SELECT loop_id, mode, rating FROM reps", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(loop_id, loops[0]["id"].as_i64().unwrap());
    assert_eq!(mode, "play");
    assert_eq!(rating, "solid");

    // due tomorrow, so today's due list stays empty
    let due = req(&mut app, "due.list", Value::Null);
    assert!(due.as_array().unwrap().is_empty());
}

#[test]
fn quick_discard_leaves_no_trace() {
    let (mut app, mock, dir, song_id) = quick_app();
    req(
        &mut app,
        "practice.quick",
        json!({"start": 0.5, "end": 1.5}),
    );
    finish_session(&mut app, &mock);

    req(&mut app, "practice.quick_discard", Value::Null);
    let loops = req(&mut app, "loop.list", json!({"song_id": song_id}));
    assert!(loops.as_array().unwrap().is_empty());
    assert_eq!(reps_count(&dir), 0);

    // the discarded session can no longer be rated
    req_err(&mut app, "practice.quick_rate", json!({"rating": "solid"}));

    // and a fresh quick session starts cleanly
    req(
        &mut app,
        "practice.quick",
        json!({"start": 0.25, "end": 1.0}),
    );
    let status = req(&mut app, "status", Value::Null);
    assert_eq!(status["plan"]["mode"], "listen");
}

#[test]
fn quick_requires_open_song_and_valid_span() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let mut app = App::new(
        Store::open(&dir.path().join("test.db")).unwrap(),
        Box::new(MockEngine::default()),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    );
    let song = req(&mut app, "song.import", json!({"path": wav}));

    let err = req_err(
        &mut app,
        "practice.quick",
        json!({"start": 0.5, "end": 1.5}),
    );
    assert!(err.contains("no song open"), "{err}");

    req(&mut app, "song.open", json!({"song_id": song["id"]}));
    req_err(
        &mut app,
        "practice.quick",
        json!({"start": 1.5, "end": 0.5}),
    );
    req_err(
        &mut app,
        "practice.quick",
        json!({"start": 1.0, "end": 1.0}),
    );
    req_err(
        &mut app,
        "practice.quick",
        json!({"start": -0.5, "end": 1.0}),
    );
}
