//! The cover does no work while nobody is looking at it.
//!
//! A cover is drawn only when the app is minimised, the home screen is
//! showing and the display is on. The rest of the time -- which is most of
//! it, and all of the night -- the app is still receiving and the cover's
//! chat lists are still following every arrival, so every message used to
//! rebuild a grid of faces that nothing was going to draw. See
//! docs/POWER.md.
//!
//! What it must not do is stay wrong: whatever it missed has to be there by
//! the time it is looked at. Both halves are here.

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

/// Loads the cover at a size and out of sight -- `Cover.Inactive`, which is
/// what the stub's `status` stands in for -- and reads back both what it has
/// drawn and what it knows.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 240, height: 360, status: 0 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Cover.Active, once someone is looking.
        function wake() { loader.item.status = 2; return 'ok' }
        function planned() { return '' + loader.item.cells.length }
        function stale() { return loader.item.stale ? 'yes' : 'no' }
        function lists() {
            return '' + allIn(loader.item, 'coverChats', []).length
        }
        // What the lists know, whether or not the cover has drawn it.
        function known() {
            var lists = allIn(loader.item, 'coverChats', [])
            var total = 0
            for (var i = 0; i < lists.length; i++) {
                total += lists[i].rowCount()
            }
            return '' + total
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
    }
";

#[test]
fn a_cover_nobody_is_looking_at_waits_and_catches_up_when_looked_at() {
    let temp = std::env::temp_dir().join(format!("postivene-cover-asleep-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_FAKE_ACCOUNTS", "1,2");
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
    let cover = format!(
        "file://{}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    );

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

    let steps: common::Steps = common::Steps::default();

    let loading = steps.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        common::record(
            &loading,
            "load",
            call!("load", QString::from(cover.clone())).into(),
        );
    });

    // Everything has arrived by now: the lists have their chats and have
    // told the cover so. Nothing is looking at it.
    let asleep = steps.clone();
    single_shot(Duration::from_secs(4), move || unsafe {
        common::record(&asleep, "lists", call!("lists").into());
        common::record(&asleep, "known", call!("known").into());
        common::record(&asleep, "drawn-asleep", call!("planned").into());
        common::record(&asleep, "stale", call!("stale").into());
        call!("wake");
    });

    let awake = steps.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
        common::record(&awake, "drawn-awake", call!("planned").into());
        common::record(&awake, "stale-awake", call!("stale").into());
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the cover did not load"
    );
    assert_eq!(
        common::value_of(&steps, "lists"),
        "2",
        "the cover did not make one chat list per profile, so nothing here \
         was following anything and this test proves nothing"
    );
    assert_ne!(
        common::value_of(&steps, "known"),
        "0",
        "the lists never filled, so there was nothing for the cover to draw \
         whether it was looking or not and this test proves nothing"
    );
    assert_eq!(
        common::value_of(&steps, "drawn-asleep"),
        "0",
        "the cover laid out its grid while nothing was looking at it, which \
         is once per arriving message with the screen off"
    );
    assert_eq!(
        common::value_of(&steps, "stale"),
        "yes",
        "the cover did not remember that it had work waiting, so it would \
         never do it"
    );
    assert_ne!(
        common::value_of(&steps, "drawn-awake"),
        "0",
        "the cover was looked at and never caught up, so it shows nothing"
    );
    assert_eq!(
        common::value_of(&steps, "stale-awake"),
        "no",
        "the cover caught up and still thinks it has work waiting"
    );
}
