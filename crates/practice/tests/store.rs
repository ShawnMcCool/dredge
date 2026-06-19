use practice::store::Store;

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
