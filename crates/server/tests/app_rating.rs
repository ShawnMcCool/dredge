use practice::model::{LoopId, LoopKind};
use practice::schedule::Resurfacing;
use practice::store::{NewLoop, NewSong, Store};
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::protocol::Request;
use time::macros::format_description;

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

/// Store with one song and one manual loop — no audio file needed.
fn seeded_store() -> (Store, i64, LoopId) {
    let store = Store::open_in_memory().unwrap();
    let song = store
        .insert_song(NewSong {
            title: "Song",
            artist: None,
            path: "/tmp/nonexistent.flac",
            file_hash: "hash-rating-test",
            duration_secs: 120.0,
        })
        .unwrap();
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
    (store, song.id.0, l.id)
}

fn fmt(d: time::Date) -> String {
    d.format(format_description!("[year]-[month]-[day]"))
        .unwrap()
}

#[test]
fn rating_solid_schedules_resurfacing() {
    let (store, _song_id, loop_id) = seeded_store();
    let mut app = App::new(store, Box::new(MockEngine::default()));
    let today = time::OffsetDateTime::now_utc().date();

    let first = req(
        &mut app,
        "rep.rate",
        json!({"loop_id": loop_id, "rating": "solid"}),
    );
    assert_eq!(first["interval_idx"], 0);
    assert_eq!(first["due_on"], fmt(today + time::Duration::days(1)));

    let second = req(
        &mut app,
        "rep.rate",
        json!({"loop_id": loop_id, "rating": "solid"}),
    );
    assert_eq!(second["interval_idx"], 1);
    assert_eq!(second["due_on"], fmt(today + time::Duration::days(2)));
}

#[test]
fn due_list_surfaces_overdue() {
    let (store, _song_id, loop_id) = seeded_store();
    let yesterday = time::OffsetDateTime::now_utc().date() - time::Duration::days(1);
    store
        .upsert_resurfacing(Resurfacing {
            loop_id,
            interval_idx: 1,
            due_on: yesterday,
        })
        .unwrap();
    let mut app = App::new(store, Box::new(MockEngine::default()));

    let due = req(&mut app, "due.list", Value::Null);
    let due = due.as_array().unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0]["loop_id"], loop_id.0);
    assert_eq!(due[0]["name"], "riff");
}

#[test]
fn retention_via_dispatch() {
    let (store, song_id, loop_id) = seeded_store();
    let mut app = App::new(store, Box::new(MockEngine::default()));

    req(
        &mut app,
        "rep.rate",
        json!({"loop_id": loop_id, "rating": "shaky", "is_retest": true}),
    );
    req(
        &mut app,
        "rep.rate",
        json!({"loop_id": loop_id, "rating": "solid", "is_retest": true}),
    );

    let retention = req(&mut app, "retention", json!({"song_id": song_id}));
    let rows = retention.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["loop_id"], loop_id.0);
    assert_eq!(rows[0]["rating"], "solid");
}
