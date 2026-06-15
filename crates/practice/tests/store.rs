use practice::model::*;
use practice::store::*;

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
                name_override: None,
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
                name_override: None,
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
fn loops_by_ids_batch_fetch() {
    let (store, song) = store_with_song();
    let mk = |name: &str| {
        store
            .insert_loop(
                song.id,
                NewLoop {
                    name,
                    name_override: None,
                    start: 0.0,
                    end: 1.0,
                    kind: LoopKind::Manual,
                },
            )
            .unwrap()
    };
    let a = mk("a");
    let b = mk("b");
    let _c = mk("c");

    assert!(store.loops_by_ids(&[]).unwrap().is_empty());
    let got = store.loops_by_ids(&[a.id, b.id, LoopId(999)]).unwrap();
    let mut ids: Vec<i64> = got.iter().map(|l| l.id.0).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![a.id.0, b.id.0],
        "fetches the existing ids, skips 999"
    );
}

#[test]
fn loop_override_roundtrips() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "verse 1",
                name_override: None,
                start: 0.0,
                end: 10.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();
    assert_eq!(l.name_override, None);
    let pinned = store
        .update_loop(l.id, "my name", Some("my name"), 0.0, 10.0)
        .unwrap();
    assert_eq!(pinned.name_override.as_deref(), Some("my name"));
    let back = store.list_loops(song.id).unwrap();
    assert_eq!(back[0].name_override.as_deref(), Some("my name"));
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
    assert!(!store.has_analysis(song.id).unwrap());

    let a = sample_analysis();
    store.save_analysis(song.id, &a).unwrap();
    assert_eq!(store.get_analysis(song.id).unwrap(), Some(a.clone()));
    assert!(store.has_analysis(song.id).unwrap());

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
fn open_migrates_legacy_v1_db() {
    // Build a pre-analysis (v1) database by hand — including the retired
    // plans/reps/resurfacing tables — then open it through Store. Migration
    // adds the analysis table (v2) and drops the practice-plan tables (v8).
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
            CREATE TABLE loops (
                id INTEGER PRIMARY KEY,
                song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                start_secs REAL NOT NULL,
                end_secs REAL NOT NULL,
                kind_json TEXT NOT NULL
            );
            CREATE TABLE sections (
                id INTEGER PRIMARY KEY,
                song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                start_secs REAL NOT NULL,
                end_secs REAL NOT NULL,
                position INTEGER NOT NULL
            );
            CREATE TABLE plans (
                id INTEGER PRIMARY KEY,
                song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                steps_json TEXT NOT NULL
            );
            CREATE TABLE reps (
                id INTEGER PRIMARY KEY,
                loop_id INTEGER NOT NULL REFERENCES loops(id) ON DELETE CASCADE,
                plan_id INTEGER REFERENCES plans(id) ON DELETE SET NULL,
                mode TEXT NOT NULL,
                rate REAL NOT NULL,
                rating TEXT,
                is_retest INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE resurfacing (
                loop_id INTEGER PRIMARY KEY REFERENCES loops(id) ON DELETE CASCADE,
                interval_idx INTEGER NOT NULL,
                due_on TEXT NOT NULL
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

    // reopening is a no-op migration
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
                name_override: None,
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
fn update_song_changes_title_and_artist() {
    let (store, song) = store_with_song();
    let updated = store
        .update_song(song.id, "New Title", Some("New Band"))
        .unwrap();
    assert_eq!(updated.title, "New Title");
    assert_eq!(updated.artist.as_deref(), Some("New Band"));
    // path and hash are untouched
    assert_eq!(updated.path, song.path);
    assert_eq!(updated.file_hash, song.file_hash);
    // persisted: a fresh list reflects the change
    let listed = store.list_songs().unwrap();
    assert_eq!(listed[0].title, "New Title");
    assert_eq!(listed[0].artist.as_deref(), Some("New Band"));
}

#[test]
fn update_song_clears_artist_when_none() {
    let (store, song) = store_with_song();
    // first set an artist
    store
        .update_song(song.id, &song.title, Some("Some Band"))
        .unwrap();
    // now clear it
    let cleared = store.update_song(song.id, &song.title, None).unwrap();
    assert_eq!(cleared.artist, None);
    // persisted
    let listed = store.list_songs().unwrap();
    assert_eq!(listed[0].artist, None);
}

#[test]
fn update_song_returns_not_found_for_stale_id() {
    let store = Store::open_in_memory().unwrap();
    let err = store.update_song(SongId(999), "Ghost", None).unwrap_err();
    assert!(
        matches!(err, practice::error::Error::NotFound),
        "expected NotFound, got {err:?}"
    );
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
