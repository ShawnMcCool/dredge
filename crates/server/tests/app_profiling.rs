//! Profiling + analysis device control through the dispatcher. Hermetic.
use practice::store::Store;
use serde_json::{json, Value};
use server::analysis::{fake_analysis, songformer_venv_present, Analyzer, FakeAnalyzer};
use server::app::App;
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
        Arc::new(FakeSeparator),
    );
    app.set_analyzer(analyzer);
    app.set_library_root(dir.path().join("library"));
    let song = req(
        &mut app,
        "song.import",
        json!({"path": wav.to_string_lossy()}),
    );
    let song_id = song["id"].as_i64().unwrap();
    Ctx {
        app,
        song_id,
        _dir: dir,
    }
}

/// Poll tick() until a named event lands (<=15 s); returns its data.
fn wait_for_event(app: &mut App, name: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        for ev in app.tick() {
            if ev.event == name {
                return ev.data;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no {name} event within 15 s");
}

// gpu call -> novelty, cpu call -> songformer (exercises the recovery branch)
struct DeviceAwareAnalyzer;
impl Analyzer for DeviceAwareAnalyzer {
    fn analyze(
        &self,
        _a: &std::path::Path,
        force_cpu: bool,
        _cancel: &server::proc::CancelToken,
    ) -> Result<practice::model::Analysis, String> {
        let mut x = fake_analysis();
        x.engine = if force_cpu {
            "songformer".into()
        } else {
            "beat_this+novelty".into()
        };
        Ok(x)
    }
    fn is_available(&self) -> bool {
        true
    }
}

#[test]
fn analysis_cpu_setting_forces_cpu_and_profiles() {
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer));
    req(
        &mut ctx.app,
        "settings.set",
        json!({"key":"analysis_device","value":"cpu"}),
    );
    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id, "force": true}),
    );
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["op"], "analysis");
    assert_eq!(data["device"], "cpu");
    assert_eq!(data["engine"], "songformer");
    assert!(data["total_ms"].as_u64().is_some());
}

#[test]
fn analysis_auto_recovers_to_cpu_when_songformer_present() {
    // Only meaningful when a songformer venv exists; gate on the probe so a
    // machine without it still passes deterministically.
    if !songformer_venv_present() {
        return;
    }
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer)); // default setting = auto
    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id, "force": true}),
    );
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["engine"], "songformer");
    assert_eq!(data["device"], "cpu");
    let stages = data["stages"].as_array().unwrap();
    assert!(stages.iter().any(|s| s["name"] == "analyze (cpu)"));
}

#[test]
fn stems_separate_records_a_profile() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));
    req(
        &mut ctx.app,
        "stems.separate",
        json!({"song_id": ctx.song_id}),
    );
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["op"], "stems");
    assert!(data["total_ms"].as_u64().is_some());
}

#[test]
fn profiles_list_returns_recorded_runs() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));
    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id, "force": true}),
    );
    wait_for_event(&mut ctx.app, "profile_run");
    let v = req(&mut ctx.app, "profiles.list", json!({"limit": 10}));
    let arr = v.as_array().unwrap();
    assert!(
        arr.iter().any(|r| r["op"] == "analysis"),
        "lists the analysis run"
    );
}

#[test]
fn analysis_with_reporter_still_completes_and_profiles() {
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer));
    req(
        &mut ctx.app,
        "settings.set",
        json!({"key":"analysis_device","value":"cpu"}),
    );
    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id, "force": true}),
    );
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["op"], "analysis");
    assert_eq!(data["engine"], "songformer");
}

#[test]
fn analysis_profile_includes_max_metrics() {
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer));
    req(
        &mut ctx.app,
        "settings.set",
        json!({"key":"analysis_device","value":"cpu"}),
    );
    req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id, "force": true}),
    );
    let data = wait_for_event(&mut ctx.app, "profile_run");
    // sampler isn't spawned in tests, so observe() never runs → max_cpu_pct is 0,
    // but it must be present (the worker stamped the maxima onto the profile).
    assert!(
        data["max_cpu_pct"].is_number(),
        "max_cpu_pct present: {data}"
    );
}
