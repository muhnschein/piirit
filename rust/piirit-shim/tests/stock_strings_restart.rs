//! A core that comes back is handed the reader's words again.
//!
//! The core keeps stock strings only for as long as it runs. When it dies
//! and the shim spawns another, the new one starts with English, so the
//! table goes to it before it is asked anything else -- the same as the
//! first one, and without the window having to notice.

// Qt harness: see qml_share.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::json;

mod common;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    Item {
        // Handed over before the core is started, as the window does.
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

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Every server dies a moment after it starts, and the shim spawns
        // the next one a second and then two seconds later.
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

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: fires only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(5), move || unsafe {
        (*engine_ptr).quit();
    });
    engine.exec();

    // Nothing else is asked of any of them, so each server's one call
    // is the table: one per server, and more than one server.
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
