//! Analysis through the dispatcher: background job, cache, song.open payload,
//! downbeat-aware junction derivation. Hermetic via FakeAnalyzer.

use practice::store::Store;
use serde_json::{json, Value};
use server::analysis::{fake_analysis, Analyzer, FakeAnalyzer};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

fn write_test_wav(path: &std::path::Path) {
    // 1 s of 440 Hz stereo sine at 48 kHz
    let samples: Vec<f32> = (0..48_000)
        .flat_map(|i| {
            let v = (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            [v, v]
        })
        .collect();
    engine::capture::write_wav(path, &samples).unwrap();
}

struct Ctx {
    app: App,
    song_id: i64,
    _dir: tempfile::TempDir,
}

fn setup(analyzer: Arc<dyn Analyzer>) -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("song.wav");
    write_test_wav(&wav);
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(Arc::new(Mutex::new(MockEngine::default()))),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    );
    app.set_analyzer(analyzer);
    let song = req(
        &mut app,
        "song.import",
        json!({"path": wav.to_string_lossy()}),
    );
    let song_id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": song_id}));
    Ctx {
        app,
        song_id,
        _dir: dir,
    }
}

/// Poll tick() until an analysis_progress event lands (≤10 s); returns its data.
fn wait_for_progress(app: &mut App) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        for ev in app.tick() {
            if ev.event == "analysis_progress" {
                return ev.data;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no analysis_progress event within 10 s");
}

#[test]
fn run_reports_done_then_caches() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));

    let status = req(
        &mut ctx.app,
        "analysis.status",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(status["state"], "none");

    let out = req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(out["state"], "running");

    let progress = wait_for_progress(&mut ctx.app);
    assert_eq!(progress["state"], "done");
    assert_eq!(progress["song_id"], ctx.song_id);

    let status = req(
        &mut ctx.app,
        "analysis.status",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(status["state"], "cached");

    // a second run never re-analyzes
    let out = req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(out["state"], "cached");

    let got = req(
        &mut ctx.app,
        "analysis.get",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(got, serde_json::to_value(fake_analysis()).unwrap());
}

#[test]
fn failures_are_reported_not_cached() {
    struct FailingAnalyzer;
    impl Analyzer for FailingAnalyzer {
        fn analyze(&self, _audio: &std::path::Path) -> Result<practice::model::Analysis, String> {
            Err("model exploded".into())
        }
        fn is_available(&self) -> bool {
            true
        }
    }

    let mut ctx = setup(Arc::new(FailingAnalyzer));
    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id}),
    );
    let progress = wait_for_progress(&mut ctx.app);
    assert_eq!(progress["state"], "failed");
    assert_eq!(progress["error"], "model exploded");
    let status = req(
        &mut ctx.app,
        "analysis.status",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(status["state"], "none");
}

#[test]
fn run_unavailable_errors_helpfully() {
    struct NeverAvailable;
    impl Analyzer for NeverAvailable {
        fn analyze(&self, _audio: &std::path::Path) -> Result<practice::model::Analysis, String> {
            Err("unreachable".into())
        }
        fn is_available(&self) -> bool {
            false
        }
    }

    let mut ctx = setup(Arc::new(NeverAvailable));
    let resp = ctx.app.dispatch(Request {
        id: 1,
        cmd: "analysis.run".into(),
        params: json!({"song_id": ctx.song_id}),
    });
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert!(err.contains("scripts/analyze"), "error was: {err}");
}

#[test]
fn open_returns_cached_analysis() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));

    let opened = req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}));
    assert_eq!(opened["analysis"], Value::Null);

    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id}),
    );
    wait_for_progress(&mut ctx.app);

    let opened = req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}));
    assert_eq!(
        opened["analysis"],
        serde_json::to_value(fake_analysis()).unwrap()
    );
}

#[test]
fn junctions_snap_to_downbeats_when_analysis_exists() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));
    // boundary at 4.5 s: binary-exact, and strictly between downbeats 4 and 6
    let sections = json!([
        {"name": "A", "start": 0.0, "end": 4.5, "position": 0},
        {"name": "B", "start": 4.5, "end": 10.0, "position": 1},
    ]);

    // without analysis: fixed 2 s tail/head windows
    let out = req(
        &mut ctx.app,
        "section.replace",
        json!({"song_id": ctx.song_id, "sections": sections}),
    );
    assert_eq!(out["junctions"][0]["start"], 2.5);
    assert_eq!(out["junctions"][0]["end"], 6.5);

    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id}),
    );
    wait_for_progress(&mut ctx.app);

    // with downbeats every 2 s: boundary 4.9 snaps to the enclosing pair
    let out = req(
        &mut ctx.app,
        "section.replace",
        json!({"song_id": ctx.song_id, "sections": sections}),
    );
    assert_eq!(out["junctions"][0]["name"], "A→B");
    assert_eq!(out["junctions"][0]["start"], 4.0);
    assert_eq!(out["junctions"][0]["end"], 6.0);

    // junctions.derive goes through the same downbeat-aware path
    let derived = req(
        &mut ctx.app,
        "junctions.derive",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(derived[0]["start"], 4.0);
    assert_eq!(derived[0]["end"], 6.0);
}
