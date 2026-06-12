use engine::pipeline::{EngineCmd, EngineEvent};
use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::protocol::{Event, Request};
use std::sync::{Arc, Mutex};

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

/// App over a file-backed store with two manual loops and a three-step plan:
/// listen_first(A, 1) → play_reps(A, 2, dwell 0.8) → recall_test(B, 1, rate 1.0).
fn plan_app() -> (App, SharedMock, tempfile::TempDir, i64) {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("riff.wav");
    write_test_wav(&wav);
    let db = dir.path().join("test.db");
    let mock: SharedMock = Arc::new(Mutex::new(MockEngine::default()));
    let mut app = App::new(Store::open(&db).unwrap(), Box::new(mock.clone()));

    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));
    let a = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "A", "start": 0.0, "end": 1.0}),
    )["id"]
        .as_i64()
        .unwrap();
    let b = req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "B", "start": 1.0, "end": 2.0}),
    )["id"]
        .as_i64()
        .unwrap();
    let plan = req(
        &mut app,
        "plan.save",
        json!({"song_id": id, "name": "drill", "steps": [
            {"step": "listen_first", "loop_id": a, "reps": 1},
            {"step": "play_reps", "loop_id": a, "reps": 2,
             "curve": {"curve": "dwell", "rate": 0.8}},
            {"step": "recall_test", "loop_id": b, "alternations": 1, "rate": 1.0},
        ]}),
    );
    let plan_id = plan["id"].as_i64().unwrap();
    mock.lock().unwrap().sent.clear();
    (app, mock, dir, plan_id)
}

#[test]
fn plan_start_applies_first_rep() {
    let (mut app, mock, _dir, plan_id) = plan_app();
    req(&mut app, "plan.start", json!({"plan_id": plan_id}));
    let sent = mock.lock().unwrap().sent.clone();
    assert_eq!(
        sent[sent.len() - 4..],
        [
            EngineCmd::SetLoopSecs {
                start: 0.0,
                end: 1.0
            },
            EngineCmd::SetRate(1.0),
            EngineCmd::Mute(false),
            EngineCmd::Play,
        ]
    );
}

#[test]
fn wraps_drive_progression_through_modes() {
    let (mut app, mock, dir, plan_id) = plan_app();
    req(&mut app, "plan.start", json!({"plan_id": plan_id}));

    // one wrap per tick; 5 reps total → 5 wraps to finish
    let mut ticks: Vec<Vec<Event>> = Vec::new();
    for _ in 0..5 {
        mock.lock()
            .unwrap()
            .queued_events
            .push_back(EngineEvent::LoopWrapped);
        ticks.push(app.tick());
    }

    let rep_changes: Vec<Value> = ticks
        .iter()
        .flatten()
        .filter(|e| e.event == "rep_changed")
        .map(|e| e.data.clone())
        .collect();
    assert_eq!(rep_changes.len(), 4);
    let modes: Vec<&str> = rep_changes
        .iter()
        .map(|d| d["mode"].as_str().unwrap())
        .collect();
    assert_eq!(modes, ["play", "play", "play", "recall_silent"]);
    let rates: Vec<f64> = rep_changes
        .iter()
        .map(|d| d["rate"].as_f64().unwrap())
        .collect();
    assert_eq!(rates, [0.8, 0.8, 1.0, 1.0]);
    // wrap 3 moves to loop B (recall audible half)
    assert_eq!(rep_changes[2]["step_idx"], 2);

    // wrap 4: silent half → engine got Mute(true)
    assert!(mock.lock().unwrap().sent.contains(&EngineCmd::Mute(true)));

    // wrap 5: plan finished → Pause
    let last_tick = ticks.last().unwrap();
    assert!(last_tick.iter().any(|e| e.event == "plan_finished"));
    assert_eq!(mock.lock().unwrap().sent.last(), Some(&EngineCmd::Pause));

    // 5 unrated rep rows journaled
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reps WHERE rating IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(n, 5);
}

#[test]
fn skip_step_jumps() {
    let (mut app, mock, _dir, plan_id) = plan_app();
    req(&mut app, "plan.start", json!({"plan_id": plan_id}));
    let skipped = req(&mut app, "plan.skip_step", Value::Null);
    assert_eq!(skipped["step_idx"], 1);
    assert!(mock.lock().unwrap().sent.contains(&EngineCmd::SetRate(0.8)));
}

#[test]
fn status_reports_plan_state() {
    let (mut app, _mock, _dir, plan_id) = plan_app();
    req(&mut app, "plan.start", json!({"plan_id": plan_id}));
    let status = req(&mut app, "status", Value::Null);
    assert_eq!(status["plan"]["step_idx"], 0);
    assert_eq!(status["plan"]["mode"], "listen");
}
