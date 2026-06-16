use engine::pipeline::EngineEvent;
use practice::store::Store;
use server::app::App;
use server::control::MockEngine;
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};

type SharedMock = Arc<Mutex<MockEngine>>;

fn app() -> (App, SharedMock) {
    let mock: SharedMock = Arc::new(Mutex::new(MockEngine::default()));
    let app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Arc::new(FakeSeparator),
    );
    (app, mock)
}

fn queue_pos(mock: &SharedMock, secs: f64, rate: f64, playing: bool) {
    mock.lock()
        .unwrap()
        .queued_events
        .push_back(EngineEvent::Position {
            secs,
            rate,
            playing,
        });
}

fn position_events(events: &[server::protocol::Event]) -> usize {
    events.iter().filter(|e| e.event == "position").count()
}

/// A paused song re-emits an identical Position every audio callback. The pump
/// must broadcast it once, then stay silent until it actually changes — that is
/// the idle-CPU fix.
#[test]
fn idle_position_broadcast_once_then_suppressed() {
    let (mut app, mock) = app();

    // First idle tick: the position is new, so it is broadcast.
    queue_pos(&mock, 12.0, 1.0, false);
    let first = app.tick();
    assert_eq!(position_events(&first), 1, "first idle position broadcasts");

    // Subsequent identical ticks: nothing changed, so no position event.
    for _ in 0..5 {
        queue_pos(&mock, 12.0, 1.0, false);
        let again = app.tick();
        assert_eq!(
            position_events(&again),
            0,
            "unchanged position must not re-broadcast"
        );
    }
}

/// When the position actually changes (e.g. playback advances, or rate/play
/// state flips) it must be broadcast again.
#[test]
fn changed_position_is_rebroadcast() {
    let (mut app, mock) = app();

    queue_pos(&mock, 12.0, 1.0, false);
    assert_eq!(position_events(&app.tick()), 1);

    // secs advanced
    queue_pos(&mock, 12.5, 1.0, false);
    assert_eq!(
        position_events(&app.tick()),
        1,
        "advanced secs rebroadcasts"
    );

    // play state flipped
    queue_pos(&mock, 12.5, 1.0, true);
    assert_eq!(
        position_events(&app.tick()),
        1,
        "play-state change rebroadcasts"
    );

    // rate changed
    queue_pos(&mock, 12.5, 0.75, true);
    assert_eq!(position_events(&app.tick()), 1, "rate change rebroadcasts");

    // identical again -> suppressed
    queue_pos(&mock, 12.5, 0.75, true);
    assert_eq!(
        position_events(&app.tick()),
        0,
        "identical again suppressed"
    );
}
