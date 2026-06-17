use practice::notes::{Block, NotesDoc};
use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::stems::FakeSeparator;
use std::sync::Arc;

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
        Arc::new(FakeSeparator),
    );
    (app, dir, wav)
}

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(server::protocol::Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

/// Import a song, open it, set two sections both named "verse", return the song_id.
fn setup_two_verses(app: &mut App, wav: &std::path::Path) -> i64 {
    let song = req(app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(app, "song.open", json!({"song_id": id}));
    req(
        app,
        "section.replace",
        json!({
            "song_id": id,
            "sections": [
                {"name": "verse", "start": 0.0, "end": 1.0, "position": 0},
                {"name": "verse", "start": 1.0, "end": 2.0, "position": 1},
            ]
        }),
    );
    id
}

#[test]
fn section_notes_roundtrip_and_orphan() {
    let (mut app, _dir, wav) = test_app();
    let id = setup_two_verses(&mut app, &wav);

    // Save a note for "verse 2"
    let doc = NotesDoc {
        blocks: vec![Block::Text { text: "tab".into() }],
    };
    let res = req(
        &mut app,
        "section.notes.set",
        json!({ "label": "verse 2", "doc": doc }),
    );

    // The sections array must carry the note on verse 2 and no orphans
    let secs = res["sections"].as_array().unwrap();
    let v2 = secs.iter().find(|s| s["label"] == "verse 2").unwrap();
    assert_eq!(v2["notes"]["blocks"][0]["text"], "tab");
    assert!(res["orphan_notes"].as_array().unwrap().is_empty());

    // Rename "verse 2" to "bridge" → the note for "verse 2" should now be an orphan
    let res2 = req(
        &mut app,
        "section.replace",
        json!({
            "song_id": id,
            "sections": [
                {"name": "verse",  "start": 0.0, "end": 1.0, "position": 0},
                {"name": "bridge", "start": 1.0, "end": 2.0, "position": 1},
            ]
        }),
    );
    let orphans = res2["orphan_notes"].as_array().unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0]["label"], "verse 2");
    assert_eq!(orphans[0]["doc"]["blocks"][0]["text"], "tab");
}

#[test]
fn open_song_enriches_sections_with_label_and_notes() {
    let (mut app, _dir, wav) = test_app();
    let id = setup_two_verses(&mut app, &wav);

    // Save a note, then re-open the song and verify enrichment survives a round-trip
    let doc = NotesDoc {
        blocks: vec![Block::Text {
            text: "hello".into(),
        }],
    };
    req(
        &mut app,
        "section.notes.set",
        json!({ "label": "verse 1", "doc": doc }),
    );

    let opened = req(&mut app, "song.open", json!({"song_id": id}));
    let secs = opened["sections"].as_array().unwrap();
    let v1 = secs.iter().find(|s| s["label"] == "verse 1").unwrap();
    assert_eq!(v1["notes"]["blocks"][0]["text"], "hello");
    assert!(opened["orphan_notes"].as_array().unwrap().is_empty());
}

#[test]
fn section_notes_set_requires_open_song() {
    let (mut app, _dir, _wav) = test_app();
    let doc = NotesDoc::default();
    let resp = app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "section.notes.set".into(),
        params: json!({ "label": "verse 1", "doc": doc }),
    });
    assert!(!resp.ok);
    assert!(resp.error.unwrap().contains("no song open"));
}
