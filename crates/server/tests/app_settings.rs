use engine::pipeline::EngineCmd;
use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};

fn test_app() -> App {
    App::new(
        Store::open_in_memory().unwrap(),
        Box::new(MockEngine::default()),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    )
}

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
fn settings_set_then_get_all() {
    let mut app = test_app();
    assert_eq!(req(&mut app, "settings.get_all", Value::Null), json!({}));

    req(
        &mut app,
        "settings.set",
        json!({"key": "ui_scale", "value": 1.75}),
    );
    req(
        &mut app,
        "settings.set",
        json!({"key": "grid_snap_default", "value": false}),
    );
    // overwrite in place
    req(
        &mut app,
        "settings.set",
        json!({"key": "ui_scale", "value": 2.0}),
    );

    assert_eq!(
        req(&mut app, "settings.get_all", Value::Null),
        json!({"grid_snap_default": false, "ui_scale": 2.0}),
    );
}

#[test]
fn volume_dispatches_set_volume_to_the_engine() {
    let mock = Arc::new(Mutex::new(MockEngine::default()));
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(mock.clone()),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    );
    req(&mut app, "volume", json!({"value": 0.8}));
    assert_eq!(mock.lock().unwrap().sent, vec![EngineCmd::SetVolume(0.8)]);
}
