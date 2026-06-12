use practice::model::*;
use practice::schedule::Resurfacing;
use practice::store::*;
use time::macros::date;

fn store_with_song() -> (Store, Song) {
    let store = Store::open_in_memory().unwrap();
    let song = store
        .insert_song(NewSong {
            title: "Song",
            artist: Some("Band"),
            path: "/tmp/song.flac",
            file_hash: "abc123",
            duration_secs: 240.0,
        })
        .unwrap();
    (store, song)
}

#[test]
fn song_roundtrip_and_hash_lookup() {
    let (store, song) = store_with_song();
    assert_eq!(store.song_by_hash("abc123").unwrap().unwrap().id, song.id);
    assert!(store.song_by_hash("nope").unwrap().is_none());
    assert_eq!(store.list_songs().unwrap().len(), 1);
}

#[test]
fn sections_replace_atomically_in_position_order() {
    let (mut store, song) = store_with_song();
    store
        .replace_sections(
            song.id,
            &[
                NewSection {
                    name: "Chorus",
                    start: 30.0,
                    end: 50.0,
                    position: 1,
                },
                NewSection {
                    name: "Verse",
                    start: 10.0,
                    end: 30.0,
                    position: 0,
                },
            ],
        )
        .unwrap();
    let sections = store.list_sections(song.id).unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name, "Verse"); // ordered by position
                                           // replace drops the old set
    store
        .replace_sections(
            song.id,
            &[NewSection {
                name: "Solo",
                start: 0.0,
                end: 5.0,
                position: 0,
            }],
        )
        .unwrap();
    assert_eq!(store.list_sections(song.id).unwrap().len(), 1);
}

#[test]
fn loops_roundtrip_with_kind() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "Verse→Chorus",
                start: 28.0,
                end: 32.0,
                kind: LoopKind::Junction {
                    from_section: SectionId(1),
                    to_section: SectionId(2),
                },
            },
        )
        .unwrap();
    let loops = store.list_loops(song.id).unwrap();
    assert_eq!(loops, vec![l.clone()]);
    store.delete_loop(l.id).unwrap();
    assert!(store.list_loops(song.id).unwrap().is_empty());
}

#[test]
fn loop_by_id_finds_one_or_none() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "riff",
                start: 1.0,
                end: 2.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();
    assert_eq!(store.loop_by_id(l.id).unwrap(), Some(l.clone()));
    assert!(store.loop_by_id(LoopId(999)).unwrap().is_none());
}

#[test]
fn plans_roundtrip_steps_json() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "A",
                start: 0.0,
                end: 4.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();
    let steps = vec![PlanStep::PlayReps {
        loop_id: l.id,
        reps: 5,
        curve: TempoCurve::Dwell { rate: 0.9 },
    }];
    let plan = store.save_plan(song.id, "warmup", &steps).unwrap();
    assert_eq!(store.list_plans(song.id).unwrap(), vec![plan.clone()]);
    assert_eq!(plan.steps, steps);
}

#[test]
fn retention_reports_latest_retest_per_loop() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "A",
                start: 0.0,
                end: 4.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();
    store
        .record_rep(NewRep {
            loop_id: l.id,
            plan_id: None,
            mode: "play".into(),
            rate: 0.9,
            rating: Some(Rating::Shaky),
            is_retest: true,
        })
        .unwrap();
    store
        .record_rep(NewRep {
            loop_id: l.id,
            plan_id: None,
            mode: "play".into(),
            rate: 1.0,
            rating: Some(Rating::Solid),
            is_retest: true,
        })
        .unwrap();
    // non-retest reps don't count
    store
        .record_rep(NewRep {
            loop_id: l.id,
            plan_id: None,
            mode: "play".into(),
            rate: 1.0,
            rating: Some(Rating::Miss),
            is_retest: false,
        })
        .unwrap();
    let retention = store.retention(song.id).unwrap();
    assert_eq!(retention.len(), 1);
    assert_eq!(retention[0].0, l.id);
    assert_eq!(retention[0].1, Rating::Solid);
}

#[test]
fn resurfacing_upserts() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "A",
                start: 0.0,
                end: 4.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();
    let r1 = Resurfacing {
        loop_id: l.id,
        interval_idx: 0,
        due_on: date!(2026 - 06 - 13),
    };
    let r2 = Resurfacing {
        loop_id: l.id,
        interval_idx: 1,
        due_on: date!(2026 - 06 - 15),
    };
    store.upsert_resurfacing(r1).unwrap();
    store.upsert_resurfacing(r2).unwrap();
    assert_eq!(store.all_resurfacing().unwrap(), vec![r2]);
}

fn sample_analysis() -> Analysis {
    Analysis {
        bpm: Some(157.89),
        beats: vec![16.28, 17.06, 17.44],
        downbeats: vec![16.28, 17.82],
        sections: vec![AnalysisSection {
            label: "A".into(),
            start: 0.0,
            end: 16.28,
        }],
        engine: "beat_this+novelty".into(),
    }
}

#[test]
fn analysis_roundtrips_and_upserts() {
    let (store, song) = store_with_song();
    assert!(store.get_analysis(song.id).unwrap().is_none());

    let a = sample_analysis();
    store.save_analysis(song.id, &a).unwrap();
    assert_eq!(store.get_analysis(song.id).unwrap(), Some(a.clone()));

    // re-analysis overwrites in place
    let b = Analysis {
        bpm: None,
        engine: "songformer".into(),
        ..a
    };
    store.save_analysis(song.id, &b).unwrap();
    assert_eq!(store.get_analysis(song.id).unwrap(), Some(b));
}

#[test]
fn deleting_song_cascades_analysis() {
    let (store, song) = store_with_song();
    store.save_analysis(song.id, &sample_analysis()).unwrap();
    store.delete_song(song.id).unwrap();
    assert!(store.get_analysis(song.id).unwrap().is_none());
}

#[test]
fn open_migrates_v1_db_to_v2() {
    // Build a pre-analysis (v1) database by hand, then open it through Store.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE songs (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                artist TEXT,
                path TEXT NOT NULL,
                file_hash TEXT NOT NULL UNIQUE,
                duration_secs REAL NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO songs (title, path, file_hash, duration_secs)
            VALUES ('Old', '/tmp/old.wav', 'hash-v1', 60.0);",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    let store = Store::open(&path).unwrap();
    let song = store.song_by_hash("hash-v1").unwrap().unwrap();
    // v1 data survives, and the new analysis table is usable immediately
    let a = sample_analysis();
    store.save_analysis(song.id, &a).unwrap();
    assert_eq!(store.get_analysis(song.id).unwrap(), Some(a));

    // reopening a v2 db is a no-op migration
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.get_analysis(song.id).unwrap().is_some());
}

#[test]
fn deleting_song_cascades() {
    let (store, song) = store_with_song();
    store
        .insert_loop(
            song.id,
            NewLoop {
                name: "A",
                start: 0.0,
                end: 4.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();
    store.delete_song(song.id).unwrap();
    assert!(store.list_loops(song.id).unwrap().is_empty());
    assert!(store.list_songs().unwrap().is_empty());
}

#[test]
fn settings_roundtrip_arbitrary_json() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.get_setting("ui_scale").unwrap().is_none());
    assert!(store.all_settings().unwrap().is_empty());

    store
        .set_setting("ui_scale", &serde_json::json!(1.75))
        .unwrap();
    store
        .set_setting("grid_snap_default", &serde_json::json!(false))
        .unwrap();
    assert_eq!(
        store.get_setting("ui_scale").unwrap(),
        Some(serde_json::json!(1.75))
    );

    // upsert overwrites in place
    store
        .set_setting("ui_scale", &serde_json::json!(2.0))
        .unwrap();
    assert_eq!(
        store.all_settings().unwrap(),
        vec![
            ("grid_snap_default".into(), serde_json::json!(false)),
            ("ui_scale".into(), serde_json::json!(2.0)),
        ]
    );
}
