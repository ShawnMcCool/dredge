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
