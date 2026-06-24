//! metronome.set through the dispatcher: forwards a SetMetronome to the engine
//! and persists the config minus the transient `running` flag. Needs no song.

use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request {
        id: 1,
        cmd: cmd.into(),
        params,
    });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

#[test]
fn metronome_set_forwards_to_engine_and_persists_without_running() {
    let mock = Arc::new(Mutex::new(MockEngine::default()));
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Arc::new(FakeSeparator),
    );

    // metronome.set running:true forwards a SetMetronome to the engine and
    // persists all but `running`.
    req(
        &mut app,
        "metronome.set",
        json!({
            "running": true, "bpm": 100.0, "beats_per_bar": 3,
            "cadence": "bar", "kit": "kick_snare"
        }),
    );

    let sent = mock.lock().unwrap().sent.clone();
    assert!(
        sent.iter().any(|c| matches!(
            c,
            engine::pipeline::EngineCmd::SetMetronome {
                running: true,
                beats_per_bar: 3,
                ..
            }
        )),
        "SetMetronome reached the engine"
    );

    // The persisted setting omits `running`.
    let all = req(&mut app, "settings.get_all", json!({}));
    assert_eq!(all["metronome"]["bpm"], json!(100.0));
    assert_eq!(all["metronome"]["beats_per_bar"], json!(3));
    assert_eq!(all["metronome"]["cadence"], json!("bar"));
    assert_eq!(all["metronome"]["kit"], json!("kick_snare"));
    assert!(
        all["metronome"].get("running").is_none(),
        "running is transient"
    );
}
