//! The cover with nobody in it.
//!
//! The chat with oneself and the core's own device chat are chats, but
//! not people: a cover with only those says there are no messages rather
//! than drawing a grid of the reader's own face. Seeded so through the
//! fake core, which is why this is its own binary.

// Qt harness: see qml_cover.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 240, height: 360 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function countIn(node, name) {
            if (!node) { return 0 }
            var total = node.objectName === name && node.visible ? 1 : 0
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                total += countIn(kids[i], name)
            }
            return total
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function count(name) {
            return '' + countIn(loader.item, name)
        }
        function rows() {
            return '' + findIn(loader.item, 'coverChats').count
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_cover_with_only_oneself_and_the_device_says_there_are_no_messages() {
    let temp =
        std::env::temp_dir().join(format!("postivene-qml-cover-empty-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. One configured profile, whose two chats
    // are made the chat with oneself and the device chat.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_FAKE_ACCOUNTS", "1");
        std::env::set_var("POSTIVENE_FAKE_SELF_TALK", "1");
        std::env::set_var("POSTIVENE_FAKE_DEVICE_TALK", "2");
    }

    postivene_shim::register_qml_types();
    common::register_cover_enum();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        let url = format!(
            "file://{}",
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../qml/cover/CoverPage.qml")
                .display()
        );
        record!("load", call!("load", QString::from(url)));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("rows", call!("rows"));
        record!("empty-shown", get!("emptyLabel", "visible"));
        record!("empty-says", get!("emptyLabel", "text"));
        record!("empty-wraps", get!("emptyLabel", "wrapMode"));
        record!("grid", call!("count", QString::from("gridCell")));
        record!("count-says", get!("unreadTotal", "text"));
        record!("brand", get!("brand", "text"));
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

    assert_eq!(value("load"), "ok", "the cover did not load. {context}");
    assert_eq!(
        value("rows"),
        "2",
        "the two chats did not load, so this proves nothing. {context}"
    );
    assert_eq!(
        value("empty-shown"),
        "true",
        "the cover does not say there are no messages when the only chats \
         are with oneself and the device. {context}"
    );
    assert_eq!(
        value("empty-says"),
        "No messages",
        "the line is not the one asked for. {context}"
    );
    assert_ne!(
        value("empty-wraps"),
        "0",
        "the line cannot wrap, so a longer language runs off the cover. {context}"
    );
    assert_eq!(
        value("grid"),
        "0",
        "a grid is drawn from chats that are nobody. {context}"
    );
    assert_eq!(
        value("count-says"),
        "0",
        "the count is not zero with nobody there. {context}"
    );
    assert_eq!(
        value("brand"),
        "Postivene",
        "the cover does not name the app in its corner. {context}"
    );
}
