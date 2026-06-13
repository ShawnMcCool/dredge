use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::Arc;

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
fn bass_focus_command_dispatches() {
    let mut app = test_app();
    req(&mut app, "bass_focus", json!({"on": true}));
    req(&mut app, "bass_focus", json!({"on": false}));
}
