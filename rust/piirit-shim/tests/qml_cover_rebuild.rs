//! The cover keeps the grid it has already drawn.
//!
//! The chat lists hear about every event the core reports, and each one
//! hands the cover a refresh that often changes nothing visible. Assigning
//! a new `cover.cells` array makes a `Repeater` tear down every delegate
//! and its `OpacityMask` texture, even when the grid is identical.
//!
//! What it must still do is move when something does change, so both
//! halves are here. Unchanged refreshes keep the delegates; a change to
//! what a cell shows gives it a fresh delegate and effect chain rather
//! than changing a picture source under a cached effect, which the device
//! has shown can leave an individual face square.

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

/// Loads the cover and reads back both whether it was redrawn and whether
/// it still says what is true.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 240, height: 360 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        /// The drawn cells, in the order the cover repeats them.
        function faces() { return allIn(loader.item, 'gridCell', []) }
        function planned() { return '' + loader.item.cells.length }
        /// How many cells are the phone's highlight rather than grey.
        function lit() {
            var cells = faces()
            var total = 0
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].highlight) { total += 1 }
            }
            return '' + total
        }
        function pill() {
            var labels = allIn(loader.item, 'unreadTotal', [])
            return labels.length ? '' + labels[0].text : 'missing'
        }
        /// Gather again with the lists as they are, and say whether the
        /// repeater kept the items it had or was handed a new grid.
        function rebuild() {
            var before = faces()
            loader.item.gather()
            var after = faces()
            if (before.length !== after.length) {
                return 'count:' + before.length + '>' + after.length
            }
            for (var i = 0; i < before.length; i++) {
                if (before[i] !== after[i]) { return 'rebuilt' }
            }
            return 'kept'
        }
        function markUnread(list, chatId) {
            allIn(loader.item, 'coverChats', [])[list].mark_unread(chatId)
            return 'ok'
        }
        /// The first drawn cell now, against the one remembered before a
        /// visible change landed.
        property var remembered: null
        function remember() {
            remembered = faces()[0]
            return remembered ? 'ok' : 'none'
        }
        function keptFace() {
            return faces()[0] === remembered ? 'kept' : 'rebuilt'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_cover_keeps_the_grid_it_has_already_drawn() {
    let temp = std::env::temp_dir().join(format!("piirit-cover-rebuild-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two configured profiles, each with the
    // fake's two chats, so there are faces to draw.
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

    // Everything has arrived: the lists have their chats and the cover has
    // drawn them. A refresh that changes nothing must not redraw it.
    let quiet = steps.clone();
    single_shot(Duration::from_secs(4), move || unsafe {
        common::record(&quiet, "planned", call!("planned").into());
        common::record(&quiet, "rebuild-no-change", call!("rebuild").into());
        common::record(&quiet, "mark-first", call!("markUnread", 0, 1).into());
    });

    // The first unread brings the pill, which takes two cells out of the
    // grid. That visible change gets a fresh set of delegates. Remember
    // this grid, then put a second chat on unread.
    let shaped = steps.clone();
    single_shot(Duration::from_secs(6), move || unsafe {
        common::record(&shaped, "lit-one", call!("lit").into());
        common::record(&shaped, "pill-one", call!("pill").into());
        common::record(&shaped, "planned-pill", call!("planned").into());
        common::record(&shaped, "remember", call!("remember").into());
        common::record(&shaped, "mark-second", call!("markUnread", 1, 2).into());
    });

    // The second message changes another cell while the grid shape stays
    // put. It still gets fresh delegates, so an Avatar's image/effect
    // chain is not asked to switch to a different picture in place.
    let after = steps.clone();
    single_shot(Duration::from_secs(8), move || unsafe {
        common::record(&after, "lit-two", call!("lit").into());
        common::record(&after, "pill-two", call!("pill").into());
        common::record(&after, "changed-face", call!("keptFace").into());
        common::record(&after, "rebuild-after-news", call!("rebuild").into());
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    let context = format!("steps: {steps:?}");

    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the cover did not load. {context}"
    );
    let planned: u32 = common::value_of(&steps, "planned").parse().unwrap_or(0);
    assert!(
        planned > 0,
        "the cover drew no cells, so nothing here was ever drawn or left \
         alone. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "rebuild-no-change"),
        "kept",
        "the cover rebuilt the whole grid for a refresh that changed \
         nothing, which is every arriving event for as long as it is \
         looked at. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "mark-first"),
        "ok",
        "a chat could not be put back on unread, so the cover was never \
         asked to change. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "lit-one"),
        "1",
        "the cover did not light the one who wrote, so it cannot tell a \
         change from no change. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "pill-one"),
        "1 new",
        "the pill did not count the one who wrote. {context}"
    );
    let with_pill: u32 = common::value_of(&steps, "planned-pill")
        .parse()
        .unwrap_or(0);
    assert_eq!(
        with_pill + 2,
        planned,
        "the pill did not take the two cells the cover gives it, so the \
         grid never changed shape and the second half of this proves \
         nothing. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "remember"),
        "ok",
        "there was no drawn cell to remember, so nothing here was left \
         alone or rebuilt. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "mark-second"),
        "ok",
        "a second chat could not be put on unread. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "lit-two"),
        "2",
        "the cover did not light both who wrote. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "pill-two"),
        "2 new",
        "the pill did not count both who wrote. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "changed-face"),
        "rebuilt",
        "the cover reused an Avatar when its visible cell changed, so its \
         Image source and cached mask had to change in place. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "rebuild-after-news"),
        "kept",
        "the cover rebuilt the grid again after the news had landed, for a \
         refresh that changed nothing. {context}"
    );
}
