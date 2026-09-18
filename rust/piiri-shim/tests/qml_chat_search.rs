//! Typing in the chat list's search field has to reach a model.
//!
//! On the ordinary list that model is the grouped search, which looks at
//! chats, contacts and messages; the chat list underneath is left as it
//! is, so clearing the field costs nothing. The archived list is a mode
//! over one kind of thing and filters itself.
//!
//! The field lives in the list's `header`, which is a Component property,
//! so everything inside it gets its own scope. A page-level timer reading
//! `searchField.text` therefore threw a `ReferenceError` on every keystroke
//! and the query was never set -- the field looked live and did nothing.
//!
//! The archived pulley is checked here too: every item in it was hidden,
//! so it opened onto nothing.
//!
//! What a *row* offers in archived mode is not checked. A `ContextMenu` is
//! held in a property rather than among an item's children, and is not
//! reachable from this probe -- so the archived row's menu (Unarchive and
//! Delete, rather than the ordinary chat's actions) rests on review and
//! `qml-lint`, not on a test.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, accountId, archived) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: archived })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // `data` rather than `children`: the model is a plain QObject, so
        // it is not among an Item's visual children at all.
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            // A list's header and delegates hang off contentItem.
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
fn typing_in_the_search_field_reaches_the_model() {
    let temp = std::env::temp_dir().join(format!("piiri-chat-search-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piiri_shim::register_qml_types();

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            ),
        ));
    });

    // Both chats are there before anything is typed.
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push((
            "before",
            call!("get", QString::from("chats"), QString::from("count")),
        ));
        (*steps_ptr).push((
            "typed",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("chat 2")
            ),
        ));
    });

    // Past the 250ms debounce and the round trip.
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push((
            "query",
            call!("get", QString::from("search"), QString::from("query")),
        ));
        (*steps_ptr).push((
            "after",
            call!("get", QString::from("search"), QString::from("count")),
        ));
        (*steps_ptr).push((
            "list-untouched",
            call!("get", QString::from("chats"), QString::from("count")),
        ));
        (*steps_ptr).push((
            "archived-load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                true
            ),
        ));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push((
            "pulley",
            call!(
                "get",
                QString::from("chatListPulley"),
                QString::from("visible")
            ),
        ));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// The field drives the model, and the archived mode does not offer a
/// pulley that opens onto nothing.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the chat list page did not load. {context}"
    );
    assert_eq!(
        value("typed"),
        "ok",
        "the search field was not found on the page. {context}"
    );
    assert_eq!(
        value("before"),
        "2",
        "the unfiltered list did not hold both seeded chats. {context}"
    );
    assert_eq!(
        value("query"),
        "chat 2",
        "what was typed never reached the model, so the search does nothing. {context}"
    );
    assert_eq!(
        value("after"),
        "1",
        "the query reached the model but nothing was found for it. {context}"
    );
    assert_eq!(
        value("list-untouched"),
        "2",
        "searching refetched the chat list underneath, which is a round trip \
         per keystroke for a list nobody is looking at. {context}"
    );

    assert_eq!(
        value("archived-load"),
        "ok",
        "the page did not load in archived mode. {context}"
    );
    assert_eq!(
        value("pulley"),
        "false",
        "the archived list still has a pulley menu, and every item in it is \
         hidden -- so it opens onto nothing. {context}"
    );
}
