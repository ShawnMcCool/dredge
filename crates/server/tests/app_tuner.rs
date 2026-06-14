use engine::capture::CaptureNode;
use practice::store::Store;
use serde_json::Value;
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::stems::FakeSeparator;
use std::sync::Arc;

fn make_app() -> App {
    App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    )
}

#[test]
fn tuner_start_then_tick_emits_tuner_pitch() {
    let mut app = make_app();
    app.set_tuner(Box::new(server::tuner::MockTuner {
        inputs: vec![CaptureNode {
            id: 3,
            serial: 3,
            app: "Iface".into(),
            media: String::new(),
        }],
        running: false,
    }));

    app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "tuner.start".into(),
        params: serde_json::json!({ "node_id": 3 }),
    });

    let events = app.tick();
    let pitch = events.iter().find(|e| e.event == "tuner_pitch").unwrap();
    assert_eq!(pitch.data["hz"], 110.0);
}

#[test]
fn tuner_inputs_returns_list() {
    let mut app = make_app();
    app.set_tuner(Box::new(server::tuner::MockTuner {
        inputs: vec![CaptureNode {
            id: 5,
            serial: 5,
            app: "Guitar".into(),
            media: String::new(),
        }],
        running: false,
    }));

    let resp = app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "tuner.inputs".into(),
        params: Value::Null,
    });
    assert!(resp.ok);
    let arr = resp.data.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], 5);
    assert_eq!(arr[0]["app"], "Guitar");
}

#[test]
fn tuner_stop_clears_running() {
    let mut app = make_app();
    let mock = server::tuner::MockTuner {
        inputs: vec![CaptureNode {
            id: 7,
            serial: 7,
            app: "Mic".into(),
            media: String::new(),
        }],
        running: false,
    };
    app.set_tuner(Box::new(mock));

    app.dispatch(server::protocol::Request {
        id: 1,
        cmd: "tuner.start".into(),
        params: serde_json::json!({ "node_id": 7 }),
    });
    let stop_resp = app.dispatch(server::protocol::Request {
        id: 2,
        cmd: "tuner.stop".into(),
        params: Value::Null,
    });
    assert!(stop_resp.ok);
}
