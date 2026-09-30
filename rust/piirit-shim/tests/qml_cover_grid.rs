//! The cover's avatars live through what happens around them.
//!
//! The grid is laid out again whenever anything anywhere is said: a
//! message lands in any chat of any profile and the cover reads its lists
//! again. That used to mean re-setting the model the cells are read from,
//! which destroys and remakes every avatar under it -- a picture, an
//! effect and a texture or two each, twenty-odd at a time, for every
//! arrival of an evening. The cover is the only screen that remakes its
//! avatars like that, and it is the only screen where they came back as
//! flat squares (issue #102).
//!
//! So this pins the thing itself: an arrival changes what the grid says,
//! and the avatars saying it stay the same avatars.

// Qt harness: see qml_chat_row.rs.
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
        id: probe
        property var held: []
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
        function allIn(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name) { found.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                allIn(kids[i], name, found)
            }
            return found
        }
        function markUnread(list, chatId) {
            allIn(loader.item, 'coverChats', [])[list].mark_unread(chatId)
            return 'ok'
        }
        // The avatars on screen now, held on to.
        function hold() {
            var cells = allIn(loader.item, 'gridCell', [])
            probe.held = cells.slice()
            return '' + cells.length
        }
        // Whether they are the same avatars still.
        function kept() {
            var cells = allIn(loader.item, 'gridCell', [])
            if (cells.length !== probe.held.length) {
                return 'reshaped:' + probe.held.length + '->' + cells.length
            }
            for (var i = 0; i < cells.length; i++) {
                if (cells[i] !== probe.held[i]) { return 'remade' }
            }
            return 'kept'
        }
    }
";

#[test]
fn the_cover_keeps_its_avatars_across_an_arrival() {
    let temp = std::env::temp_dir().join(format!("piirit-cover-grid-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("load", call!("load", QString::from(cover_url())));
    });

    // The lists have landed. Something new arrives, which costs the grid
    // the two cells the pill sits on -- one reshape, and no more.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("first", call!("markUnread", 0u32, 1u32));
    });

    // The pill is up and the grid is what it now says. Hold on to the
    // avatars saying it.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("hold", call!("hold"));
        record!("second", call!("markUnread", 0u32, 2u32));
    });

    // The second arrival is on the grid. The avatars must be the same
    // avatars: nothing here is remade for an arrival.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("kept", call!("kept"));
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
    let held: u32 = value("hold").parse().unwrap_or_default();
    assert!(
        held >= 7,
        "the cover held on to no avatars at all ({}) so the rest of this \
         proves nothing. {context}",
        value("hold")
    );
    assert_eq!(
        value("kept"),
        "kept",
        "the cover's avatars were destroyed and remade for one arrival. \
         Each of them is a picture and its drawing; remaking the lot on \
         every event is what wore the cover's textures out. {context}"
    );
}

/// Where `qml/cover/CoverPage.qml` is.
fn cover_url() -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    )
}
