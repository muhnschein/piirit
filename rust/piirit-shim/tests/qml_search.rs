//! Searching from the chat list finds three kinds of thing, and says which
//! is which.
//!
//! Before this, typing narrowed the chat list and nothing else: a search
//! for someone's name found the chat only if a chat with them already
//! existed, and a search for a word in a message found nothing at all. The
//! reference clients group the answer -- chats, then contacts, then
//! messages, each under a counted heading -- and that grouping is what
//! makes the result readable rather than a pile.

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
        Loader { id: loader }
        property var searchModel: null

        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: false })
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
        // The rows are only reachable through the model the page owns.
        function bind() {
            var model = findIn(loader.item, 'search')
            if (!model) { return 'missing:search' }
            searchModel = model.rows
            return 'ok'
        }
        Repeater {
            id: rows
            model: searchModel
            Item {
                property string kind: model.kind
                property string title: model.title
                property string subtitle: model.subtitle
            }
        }
        /// Every row as `kind:title`, in the order the list shows them.
        function listed() {
            var out = ''
            for (var i = 0; i < rows.count; i++) {
                out += rows.itemAt(i).kind + ':' + rows.itemAt(i).title + ','
            }
            return out
        }
        function subtitles(wanted) {
            var out = ''
            for (var i = 0; i < rows.count; i++) {
                if (rows.itemAt(i).kind === wanted) {
                    out += rows.itemAt(i).subtitle + ','
                }
            }
            return out
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_search_finds_chats_contacts_and_messages_separately() {
    let temp = std::env::temp_dir().join(format!("piirit-search-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
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
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1
            )
        );
    });

    // "a" is in every seeded chat name, both contact addresses and the
    // word "message", so one query reaches all three groups.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("bind", call!("bind"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("a")
            )
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("rows", call!("listed"));
        record!(
            "message-subtitles",
            call!("subtitles", QString::from("message"))
        );
        record!("chats", get!("search", "chat_count"));
        record!("contacts", get!("search", "contact_count"));
        record!("messages", get!("search", "message_count"));
        record!("total", get!("search", "message_total"));
        record!("results-shown", get!("searchResults", "visible"));
        record!("list-shown", get!("chatList", "visible"));
        record!(
            "cleared",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("")
            )
        );
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("after-clearing", call!("listed"));
        record!("list-back", get!("chatList", "visible"));
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
        value("load"),
        "ok",
        "the chat list page did not load. {context}"
    );
    assert_eq!(
        value("bind"),
        "ok",
        "the page has no search model. {context}"
    );

    // Grouped, and in the reference clients' order: chats, contacts, then
    // messages. The order is what makes `section` produce three headings
    // rather than three headings per row.
    assert_eq!(
        value("rows"),
        "chat:chat 1,chat:chat 2,chat:chat 3,\
         contact:ada,contact:grace,\
         message:chat 1,message:chat 1,message:chat 2,message:chat 3,",
        "the results are not grouped the way the headings claim. {context}"
    );
    // A message result names the chat it is in and shows the text that
    // matched: without the chat name it is a line of text from nowhere.
    assert_eq!(
        value("message-subtitles"),
        "message 1,message 2,message 10,message 30,",
        "message results do not show what matched. {context}"
    );

    assert_eq!(value("chats"), "3", "wrong chat count. {context}");
    assert_eq!(value("contacts"), "2", "wrong contact count. {context}");
    assert_eq!(value("messages"), "4", "wrong message count. {context}");
    assert_eq!(
        value("total"),
        "4",
        "the total is what the heading says when the list is cut short. {context}"
    );

    // Searching swaps the whole list, rather than leaving the chat list
    // underneath for the results to be confused with.
    assert_eq!(
        value("results-shown"),
        "true",
        "results stayed hidden. {context}"
    );
    assert_eq!(
        value("list-shown"),
        "false",
        "the chat list stayed up. {context}"
    );

    // And clearing the field puts the chat list back.
    assert_eq!(
        value("after-clearing"),
        "",
        "clearing the field left the results behind. {context}"
    );
    assert_eq!(
        value("list-back"),
        "true",
        "clearing the field did not bring the chat list back. {context}"
    );
}
