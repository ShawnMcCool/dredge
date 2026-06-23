use engine::device::AudioDevice;
use practice::store::Store;
use serde_json::Value;
use server::app::App;
use server::control::MockEngine;
use server::stems::FakeSeparator;
use std::sync::Arc;

fn make_app() -> App {
    App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Arc::new(FakeSeparator),
    )
}

fn dev(id: &str) -> AudioDevice {
    AudioDevice {
        id: id.to_owned(),
        name: format!("Device {id}"),
        is_default: false,
    }
}

#[test]
fn tuner_start_then_tick_emits_tuner_pitch() {
    let mut app = make_app();
    app.set_tuner(Box::new(server::tuner::MockTuner {
        inputs: vec![dev("3")],
        running: false,
    }));

    app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "tuner.start".into(),
        params: serde_json::json!({ "device_id": "3" }),
    });

    let events = app.tick();
    let pitch = events.iter().find(|e| e.event == "tuner_pitch").unwrap();
    assert_eq!(pitch.data["hz"], 110.0);
}

#[test]
fn tuner_start_unknown_id_errors() {
    let mut app = make_app();
    app.set_tuner(Box::new(server::tuner::MockTuner {
        inputs: vec![dev("5")],
        running: false,
    }));

    let resp = app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "tuner.start".into(),
        params: serde_json::json!({ "device_id": "99" }),
    });
    assert!(!resp.ok);
}

#[test]
fn tuner_stop_clears_running() {
    let mut app = make_app();
    let mock = server::tuner::MockTuner {
        inputs: vec![dev("7")],
        running: false,
    };
    app.set_tuner(Box::new(mock));

    app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "tuner.start".into(),
        params: serde_json::json!({ "device_id": "7" }),
    });
    let stop_resp = app.dispatch(server::protocol::Request {
        id: 2,
        cmd: "tuner.stop".into(),
        params: Value::Null,
    });
    assert!(stop_resp.ok);
}
