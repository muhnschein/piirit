//! Pinned chats get a heading, and so do the rest -- but only when both
//! kinds are there.
//!
//! One heading over the whole list says nothing, and "Pinned" over a list
//! with nothing pinned is a lie. Hiding the `SectionHeader` in place was
//! not enough: Silica keeps its own size and its label is a child of it,
//! so the text stayed drawn over the first row. A plain Item collapses
//! around it instead, and clips.
//!
//! Two lists, two cases, in one process: the ordinary list holds a pinned
//! chat and an unpinned one, and the archived list holds a single
//! unpinned chat. Nothing here sets `PIIRIT_FAKE_NO_ARCHIVED`, because a
//! list with no rows in it has no headings either way and would prove
//! nothing.

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
    Item {
        // A size, because this test measures. With none the page is zero
        // wide, every width in it is zero or negative, and a heading that
        // reaches past the screen cannot be told from one that does not.
        width: 540
        height: 960
        Loader { id: loader; anchors.fill: parent }
        function load(url, accountId, archived) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: archived })
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
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // A view with no window never lays out on its own, so no
        // delegate -- and no section header -- is ever built. Forcing it
        // is what makes what the list *shows* observable here at all.
        function layout(name) {
            var view = findIn(loader.item, name)
            if (!view) { return 'missing:' + name }
            if (view.forceLayout) { view.forceLayout() }
            return 'ok'
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_category_is_only_headed_when_there_is_another_one() {
    let temp = std::env::temp_dir().join(format!("piirit-sections-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

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
        record!(
            "ordinary",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("laid-out", call!("layout", QString::from("chatList")));
        record!("both-kinds-heading", get!("chatSection", "visible"));
        record!("heading-slot", get!("chatSectionSlot", "height"));
        record!("heading-x", get!("chatSection", "x"));
        record!("heading-width", get!("chatSection", "width"));
        record!("slot-width", get!("chatSectionSlot", "width"));
        record!(
            "archived",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                true
            )
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("archived-count", get!("chats", "count"));
        record!(
            "archived-laid-out",
            call!("layout", QString::from("chatList"))
        );
        record!("one-kind-heading", get!("chatSection", "visible"));
        record!("one-kind-slot", get!("chatSectionSlot", "height"));
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
        value("ordinary"),
        "ok",
        "the chat list did not load. {context}"
    );
    // The fake pins chat 1 and leaves chat 2 alone, so this list has both
    // kinds in it and the headings are worth showing.
    assert_eq!(
        value("both-kinds-heading"),
        "true",
        "a list with pinned and unpinned chats shows neither heading, so \
         nothing says why the pinned ones are at the top. {context}"
    );
    assert_ne!(
        value("heading-slot"),
        "0",
        "the heading is shown but the row it sits in has no height, so it \
         draws over the chat below it. {context}"
    );

    // And it has to fit in that row. Silica's SectionHeader sits at the
    // left page margin and is a margin narrower than its parent on each
    // side, with its text right-aligned against its right edge -- so a
    // page that assigns it the full parent width keeps the inset x and
    // pushes the text one whole margin off the screen. That is what a
    // device screenshot showed: the last two characters of both headings
    // cut off at the edge.
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(f64::NAN);
    assert!(
        number("slot-width") > 0.0,
        "the heading's row has no width, so nothing below can be measured. \
         {context}"
    );
    assert!(
        number("heading-x") >= 0.0 && number("heading-width") > 0.0,
        "the heading has no sensible geometry at all. {context}"
    );
    assert!(
        number("heading-x") + number("heading-width") <= number("slot-width"),
        "the heading reaches past the right-hand edge of the row it sits \
         in, so its right-aligned text is cut off by the screen. Let it \
         size itself: it already insets itself from both page margins. \
         {context}"
    );

    assert_eq!(
        value("archived"),
        "ok",
        "the archived list did not load. {context}"
    );
    // The guard: an empty list has no headings either way, and this test
    // would prove nothing about hiding them.
    assert_ne!(
        value("archived-count"),
        "0",
        "the archived list is empty, so there is no single-kind list to \
         check. {context}"
    );
    assert_eq!(
        value("one-kind-heading"),
        "false",
        "a list with only one kind of chat still labels the category. \
         {context}"
    );
    assert_eq!(
        value("one-kind-slot"),
        "0",
        "the heading is hidden but still takes up a row, so there is a gap \
         above the list. {context}"
    );
}
