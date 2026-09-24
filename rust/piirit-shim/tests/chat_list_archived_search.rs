//! Searching the archived list has to return archived chats.
//!
//! The core has no single call meaning "archived chats matching this".
//! Verified against the pinned binary: with `DC_GCL_ARCHIVED_ONLY` set it
//! never looks at the query -- asking for archived chats matching "Beta"
//! returns the archived "Alpha group" all the same -- while a plain query
//! searches every chat and does include archived ones. So the model asks
//! twice and intersects, and this pins that it does.

// Qt harness: see qml_chat_list.rs.
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

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats }
        function open() {
            chats.account_id = 1
            chats.archived = true
        }
        function search(text) { chats.query = text }
        function count() { return chats.count }
    }
";

#[test]
fn searching_the_archived_list_finds_archived_chats() {
    let temp = std::env::temp_dir().join(format!("piirit-arch-search-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
        (*engine_ptr).invoke_method("open".into(), &[]);
    });

    // The archived list holds chat 3, and nothing else.
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("archived", call!("count")));
        call!("search", QString::from("chat 3"));
    });

    // A term that matches the archived chat keeps it.
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("hit", call!("count")));
        call!("search", QString::from("chat 1"));
    });

    // A term that matches only an *ordinary* chat must empty the list --
    // the archived page must not start showing unarchived hits.
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("miss", call!("count")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("archived"),
        "1",
        "the archived list did not load, so nothing below is under test. {context}"
    );
    assert_eq!(
        value("hit"),
        "1",
        "searching the archived list for a term that matches an archived chat \
         lost it. {context}"
    );
    assert_eq!(
        value("miss"),
        "0",
        "searching the archived list for a term that matches only an ordinary \
         chat returned something -- either the query is being ignored, or \
         unarchived chats are leaking into the archived page. {context}"
    );
}
