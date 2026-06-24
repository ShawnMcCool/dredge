//! Section-click commands through the dispatcher: master arm + per-section
//! toggle build a beat schedule and push it to the engine. Hermetic via
//! FakeAnalyzer (which yields a 120-BPM beat grid + auto-committed sections).

use practice::store::Store;
use serde_json::{json, Value};
use server::analysis::{Analyzer, FakeAnalyzer};
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
    // ~10 s of 440 Hz stereo sine at 48 kHz (covers the fake beat grid)
    let samples: Vec<f32> = (0..480_000)
        .flat_map(|i| {
            let v = (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            [v, v]
        })
        .collect();
    engine::capture::write_wav(path, &samples).unwrap();
}

/// Poll tick() until the analysis_progress "done" event lands (≤10 s).
fn wait_for_done(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        for ev in app.tick() {
            if ev.event == "analysis_progress" && ev.data["state"] == "done" {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no analysis done event within 10 s");
}

struct Ctx {
    app: App,
    mock: Arc<Mutex<MockEngine>>,
    song_id: i64,
    _dir: tempfile::TempDir,
}

/// Import + open a song, analyze it (FakeAnalyzer), wait for sections to land.
/// After this the open song has a beat grid and ≥1 committed section.
fn setup() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("song.wav");
    write_test_wav(&wav);

    let mock = Arc::new(Mutex::new(MockEngine::default()));
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Arc::new(FakeSeparator),
    );
    app.set_analyzer(Arc::new(FakeAnalyzer) as Arc<dyn Analyzer>);
    app.set_library_root(dir.path().join("library"));

    let song = req(
        &mut app,
        "song.import",
        json!({"path": wav.to_string_lossy()}),
    );
    let song_id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": song_id}));

    req(&mut app, "analysis.run", json!({"song_id": song_id}));
    wait_for_done(&mut app);

    Ctx {
        app,
        mock,
        song_id,
        _dir: dir,
    }
}

#[test]
fn master_arm_and_section_toggle_build_then_clear_schedule() {
    let mut ctx = setup();

    // Sections were auto-committed by analysis; grab one to mark.
    let sections = req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}))["sections"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!sections.is_empty(), "analysis auto-committed sections");
    let section_id = sections[0]["id"].as_i64().unwrap();

    // Master arm on, but no section marked yet → schedule is empty.
    req(&mut ctx.app, "sectionclick.set", json!({ "enabled": true }));
    assert!(
        ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "armed but no section marked → empty"
    );

    // Mark a section → non-empty schedule pushed to the engine.
    let res = req(
        &mut ctx.app,
        "section.click.set",
        json!({ "section_id": section_id, "on": true }),
    );
    assert!(
        res["sections"].is_array(),
        "section.click.set returns the sections payload"
    );
    assert!(
        !ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "marked section + armed → non-empty schedule"
    );

    // Disable the master arm → schedule cleared even though the section
    // stays marked.
    req(
        &mut ctx.app,
        "sectionclick.set",
        json!({ "enabled": false }),
    );
    assert!(
        ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "master off → empty schedule"
    );

    // Re-arming rebuilds the schedule from the still-marked section, proving
    // the per-section flag persisted across the master toggle.
    req(&mut ctx.app, "sectionclick.set", json!({ "enabled": true }));
    assert!(
        !ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "re-arm rebuilds from the marked section"
    );
}

/// A structure edit (`section.replace`) must preserve each surviving section's
/// click_guide flag AND recompute the schedule, instead of silently dropping
/// the marks and leaving the engine clicking stale spans.
#[test]
fn section_replace_preserves_click_guide_and_recomputes() {
    let mut ctx = setup();

    let sections = req(&mut ctx.app, "song.open", json!({"song_id": ctx.song_id}))["sections"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!sections.is_empty(), "analysis auto-committed sections");
    let section_id = sections[0]["id"].as_i64().unwrap();

    // Arm + mark a section → non-empty schedule.
    req(&mut ctx.app, "sectionclick.set", json!({ "enabled": true }));
    req(
        &mut ctx.app,
        "section.click.set",
        json!({ "section_id": section_id, "on": true }),
    );
    assert!(
        !ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "marked + armed → non-empty schedule before the edit"
    );

    // Re-send the same sections through section.replace WITH click_guide:true on
    // the marked one (mirrors what the editor's save() now sends).
    let payload: Vec<Value> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "name": s["name"],
                "start": s["start"],
                "end": s["end"],
                "position": i,
                "click_guide": s["id"].as_i64() == Some(section_id),
            })
        })
        .collect();
    let res = req(
        &mut ctx.app,
        "section.replace",
        json!({ "song_id": ctx.song_id, "sections": payload }),
    );

    // (a) The returned sections still carry the flag.
    let out = res["sections"].as_array().unwrap();
    let marked = out
        .iter()
        .find(|s| s["name"] == sections[0]["name"])
        .expect("marked section survives the replace");
    assert_eq!(
        marked["click_guide"],
        json!(true),
        "click_guide survives section.replace"
    );

    // (b) The schedule was recomputed and is still non-empty (recompute fired +
    //     flag preserved). No master-arm toggle was needed.
    assert!(
        !ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "section.replace recomputes the schedule from the preserved flag"
    );

    // A subsequent replace that omits/clears click_guide clears the schedule.
    let cleared: Vec<Value> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "name": s["name"],
                "start": s["start"],
                "end": s["end"],
                "position": i,
                "click_guide": false,
            })
        })
        .collect();
    req(
        &mut ctx.app,
        "section.replace",
        json!({ "song_id": ctx.song_id, "sections": cleared }),
    );
    assert!(
        ctx.mock.lock().unwrap().click_schedule.is_empty(),
        "clearing the flag via section.replace empties the schedule"
    );
}
