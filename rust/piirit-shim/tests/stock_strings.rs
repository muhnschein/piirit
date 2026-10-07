//! The core's own words reach it in the reader's language.
//!
//! The core writes text of its own -- "Draft" in front of a chat list
//! preview, "Member %1$s added by %2$s." in a group, the errors it
//! reports -- and has only English for it until it is handed the rest.
//! The window hands over `js/StockStrings.js`, every one of them through
//! `qsTr()`, before it starts the core, so the first thing a server is
//! asked is to take them; a table handed over later goes straight to the
//! server that is running; and a table that is not one is refused before
//! the core sees it.
//!
//! No catalog is installed here, so the table is the English source.
//! That the catalogs translate it is `piirit-app`'s translations test.

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
use serde_json::Value;

mod common;

/// Silica's `pageStack`; see `network_hint.rs`. Nothing here navigates.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property string errors: ''
        Loader { id: window }
        Connections {
            target: core
            onCore_error: errors += message + ';'
        }
        function loadWindow(url) {
            window.setSource(url, {})
            return window.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function hand(json) { core.set_stock_strings(json); return 'ok' }
        function heard() { return errors }
    }
";

/// Every table the core was handed, in order.
fn tables(journal: &std::path::Path) -> Vec<Value> {
    common::calls(journal)
        .into_iter()
        .filter(|(name, _)| name == "set_stock_strings")
        .map(|(_, params)| params[0].clone())
        .collect()
}

#[test]
fn the_core_is_handed_the_readers_words_first_and_again_when_they_change() {
    let temp = std::env::temp_dir().join(format!("piirit-stock-strings-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; the window reaches pages that use it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    common::register_dbus_enum();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let window = format!("file://{}", tree.join("piirit.qml").display());
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let first = journal.clone();
    let later = journal.clone();

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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        assert_eq!(call!("loadWindow", QString::from(window.clone())), "ok");
    });

    // The core is up. What it was asked first, and then a table that is
    // not one, and one that is.
    single_shot(Duration::from_secs(3), move || unsafe {
        let methods = common::methods(&first);
        (*steps_ptr).push(("first", methods.first().cloned().unwrap_or_default()));
        call!("hand", QString::from("[\"not a table\"]"));
        call!("hand", QString::from("{\"draft\": \"Entwurf\"}"));
        call!("hand", QString::from("{\"3\": \"Entwurf\"}"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("errors", call!("heard")));
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
    let handed = tables(&later);
    let context = format!("steps: {steps:?}, tables: {}", handed.len());

    assert_eq!(
        value("first"),
        "set_stock_strings",
        "the core was asked something before it had the reader's words, \
         and text it writes into a message as it arrives stays English. \
         {context}"
    );
    let table = handed.first().cloned().unwrap_or_default();
    let entries = table.as_object().map_or(0, serde_json::Map::len);
    assert!(
        entries >= 100,
        "the window handed over {entries} stock strings, not the table. {context}"
    );
    assert_eq!(table["3"], "Draft", "{context}");
    assert_eq!(table["129"], "Member %1$s added by %2$s.", "{context}");

    // The two that are not tables reached nobody; the one that is did.
    assert_eq!(
        handed.len(),
        2,
        "expected the window's table and the one handed over later, and \
         nothing for the two that are not tables. {context}"
    );
    assert_eq!(handed[1], serde_json::json!({"3": "Entwurf"}), "{context}");
    let errors = value("errors");
    assert_eq!(
        errors.matches("stock strings:").count(),
        2,
        "a table that is not one was not reported. {context}"
    );
}
