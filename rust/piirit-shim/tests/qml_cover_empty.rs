//! The cover with nobody in it.
//!
//! The chat with oneself and the core's own device chat are chats, but
//! not people: a cover with only those is a grid of empty circles, with
//! no pill, rather than the reader's own face drawn again and again. Seeded so through the
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

use piirit_shim::DeltaChatCore;
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
        // How many cells hold someone rather than an empty circle.
        function faces() {
            var cells = loader.item.cells
            var total = 0
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].person) { total += 1 }
            }
            return '' + total
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_cover_with_only_oneself_and_the_device_is_a_grid_of_empty_circles() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-cover-empty-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. One configured profile, whose two chats
    // are made the chat with oneself and the device chat.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("PIIRIT_FAKE_SELF_TALK", "1");
        std::env::set_var("PIIRIT_FAKE_DEVICE_TALK", "2");
    }

    piirit_shim::register_qml_types();
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
        record!("grid", call!("count", QString::from("gridCell")));
        record!("faces", call!("faces"));
        record!("pill-shown", get!("unreadPill", "visible"));
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
    let grid: u32 = value("grid").parse().unwrap_or(0);
    assert!(
        grid >= 7,
        "the grid of empty circles is not drawn with nobody there. {context}"
    );
    assert_eq!(
        value("faces"),
        "0",
        "a face is drawn from chats that are nobody. {context}"
    );
    assert_eq!(
        value("pill-shown"),
        "false",
        "the pill is drawn with nobody there and nothing new. {context}"
    );
}
