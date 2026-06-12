//! Stem separation through the dispatcher: background job, cache, auto-load,
//! per-stem gains. Hermetic via FakeSeparator.

use engine::pipeline::EngineCmd;
use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::{FakeSeparator, StemSeparator, STEM_NAMES};
use std::path::PathBuf;
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
    mock: Arc<Mutex<MockEngine>>,
    stems_dir: PathBuf,
    song_id: i64,
    file_hash: String,
    _dir: tempfile::TempDir,
}

fn setup(separator: Arc<dyn StemSeparator>) -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("song.wav");
    write_test_wav(&wav);
    let mock = Arc::new(Mutex::new(MockEngine::default()));
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Box::new(MockCapture::default()),
        separator,
    );
    let stems_dir = dir.path().join("stems");
    app.set_stems_dir(stems_dir.clone());
    let song = req(
        &mut app,
        "song.import",
        json!({"path": wav.to_string_lossy()}),
    );
    let song_id = song["id"].as_i64().unwrap();
    let file_hash = song["file_hash"].as_str().unwrap().to_owned();
    req(&mut app, "song.open", json!({"song_id": song_id}));
    Ctx {
        app,
        mock,
        stems_dir,
        song_id,
        file_hash,
        _dir: dir,
    }
}

/// Poll tick() until a stems_progress event lands (≤10 s); returns its data.
fn wait_for_progress(app: &mut App) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        for ev in app.tick() {
            if ev.event == "stems_progress" {
                return ev.data;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no stems_progress event within 10 s");
}

#[test]
fn separate_runs_and_reports_done() {
    let mut ctx = setup(Arc::new(FakeSeparator));

    let out = req(
        &mut ctx.app,
        "stems.separate",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(out["state"], "running");

    let progress = wait_for_progress(&mut ctx.app);
    assert_eq!(progress["state"], "done");
    assert_eq!(progress["song_id"], ctx.song_id);

    let status = req(
        &mut ctx.app,
        "stems.status",
        json!({"song_id": ctx.song_id}),
    );
    assert_eq!(status["state"], "cached");

    let cache = ctx.stems_dir.join(&ctx.file_hash);
    for name in STEM_NAMES {
        assert!(
            cache.join(format!("{name}.wav")).is_file(),
            "missing {name}.wav"
        );
    }
}

#[test]
fn open_autoloads_cached_stems() {
    let mut ctx = setup(Arc::new(FakeSeparator));

    // first open had no cache: single-buffer load, stems: false
    assert_eq!(
        ctx.mock
            .lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .stems
            .len(),
        1
    );

    req(
        &mut ctx.app,
        "stems.separate",
        json!({"song_id": ctx.song_id}),
    );
    let progress = wait_for_progress(&mut ctx.app);
    assert_eq!(progress["state"], "done");

    let opened = req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}));
    assert_eq!(opened["stems"], true);
    let mock = ctx.mock.lock().unwrap();
    assert_eq!(mock.loaded.as_ref().unwrap().stems.len(), 4);
}

/// Seed a pre-plan-13 stem cache entry: 1 s of stereo sine at 44.1 kHz.
fn write_44k_stem(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..44_100 {
        let v = (i as f32 / 44_100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        let s = (v * i16::MAX as f32) as i16;
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn open_lazy_upgrades_legacy_44k_cache_to_48k() {
    let mut ctx = setup(Arc::new(FakeSeparator));

    // seed a legacy 44.1 kHz cache by hand (pre-normalization separations)
    let cache = ctx.stems_dir.join(&ctx.file_hash);
    std::fs::create_dir_all(&cache).unwrap();
    for name in STEM_NAMES {
        write_44k_stem(&cache.join(format!("{name}.wav")));
    }

    // one open: stems load fine AND every cache WAV is rewritten at 48 kHz
    let opened = req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}));
    assert_eq!(opened["stems"], true);
    assert_eq!(ctx.mock.lock().unwrap().loaded.as_ref().unwrap().stems.len(), 4);
    for name in STEM_NAMES {
        let path = cache.join(format!("{name}.wav"));
        assert_eq!(
            engine::capture::wav_header_rate(&path).unwrap(),
            48_000,
            "{name}.wav not upgraded"
        );
    }
}

#[test]
fn gains_route_to_engine() {
    let mut ctx = setup(Arc::new(FakeSeparator));
    req(
        &mut ctx.app,
        "stems.separate",
        json!({"song_id": ctx.song_id}),
    );
    wait_for_progress(&mut ctx.app);
    req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}));

    ctx.mock.lock().unwrap().sent.clear();
    req(
        &mut ctx.app,
        "stems.gains",
        json!({"gains": [1.0, 1.0, 0.0, 1.0]}),
    );

    let sent = ctx.mock.lock().unwrap().sent.clone();
    let gains: Vec<EngineCmd> = sent
        .into_iter()
        .filter(|c| matches!(c, EngineCmd::SetStemGain { .. }))
        .collect();
    assert_eq!(gains.len(), 4);
    assert!(gains.contains(&EngineCmd::SetStemGain { idx: 2, gain: 0.0 }));
}

#[test]
fn separate_unavailable_errors_helpfully() {
    struct NeverAvailable;
    impl StemSeparator for NeverAvailable {
        fn separate(
            &self,
            _audio: &std::path::Path,
            _out_dir: &std::path::Path,
        ) -> Result<Vec<PathBuf>, String> {
            Err("unreachable".into())
        }
        fn is_available(&self) -> bool {
            false
        }
    }

    let mut ctx = setup(Arc::new(NeverAvailable));
    let resp = ctx.app.dispatch(Request {
        id: 1,
        cmd: "stems.separate".into(),
        params: json!({"song_id": ctx.song_id}),
    });
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert!(err.contains("not installed"), "error was: {err}");
    assert!(err.contains("uv tool install demucs"), "error was: {err}");
}
