//! Restarted core is handed the reader's words again.
//!
//! Core keeps stock strings only while running. Shim respawns after death; new
//! server starts in English, so the table goes first, as to the first one.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::json;

mod common;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    Item {
        // Handed over before core is started, as the window does.
        Component.onCompleted: core.set_stock_strings('{"3": "Entwurf"}')
    }
"#;

#[test]
fn every_server_is_handed_the_stock_strings_before_anything_else() {
    let temp = std::env::temp_dir().join(format!(
        "piirit-stock-strings-restart-{}",
        std::process::id()
    ));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Servers die soon after start; respawn after 1s, then 2s.
        std::env::set_var("PIIRIT_FAKE_EXIT_AFTER_MS", "500");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let mut plan = common::Plan::default();

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: fires only while `exec()` runs.
    plan.after(5, move || unsafe {
        (*engine_ptr).quit();
    });
    plan.run();
    engine.exec();

    // Each server's one call is the table: one per server, >1 server.
    let calls = common::calls(&journal);
    assert!(
        calls.len() >= 2,
        "the core was never restarted, or never handed the table, so this \
         says nothing: {calls:?}"
    );
    for (method, params) in &calls {
        assert_eq!(
            (method.as_str(), params),
            ("set_stock_strings", &json!([{"3": "Entwurf"}])),
            "a restarted server was not handed the reader's words: {calls:?}"
        );
    }
}
