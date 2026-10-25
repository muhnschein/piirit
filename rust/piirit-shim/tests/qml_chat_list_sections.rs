//! Pinned and rest get headings, only when both kinds present (one heading says nothing; "Pinned"
//! over nothing pinned is a lie). Hiding `SectionHeader` in place not enough: Silica keeps its size
//! and label is its child, so text stayed drawn over first row. Plain Item collapses and clips.
//!
//! Two lists, one process: ordinary list has a pinned and an unpinned chat; archived list a single
//! unpinned one. Nothing sets `PIIRIT_FAKE_NO_ARCHIVED`: list with no rows has no headings either
//! way, proving nothing.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

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

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
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

    plan.after(2, move || unsafe {
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

    plan.after(3, move || unsafe {
        record!("archived-count", get!("chats", "count"));
        record!(
            "archived-laid-out",
            call!("layout", QString::from("chatList"))
        );
        record!("one-kind-heading", get!("chatSection", "visible"));
        record!("one-kind-slot", get!("chatSectionSlot", "height"));
        (*engine_ptr).quit();
    });

    plan.run();
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
    // Fake pins chat 1, not chat 2: both kinds, headings worth showing.
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

    // Must fit row. Silica's `SectionHeader` sits at left page margin, a margin narrower than
    // parent each side, text right-aligned: assigning full parent width keeps inset x, pushes text
    // a margin off screen (device screenshot: last two chars of both headings cut off).
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
    // Guard: empty list has no headings either way.
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
