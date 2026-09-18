//! A message a search found opens its chat at that message.
//!
//! Opening at the newest message instead is the difference between
//! finding something and being told roughly where it is: the reader
//! searched for a word from last March and landed at the bottom of a
//! thousand-message thread with no idea which row matched.
//!
//! The other way a message is asked for is from a page over this one: a
//! media page's Show in chat tells the conversation which message, then
//! pops back to it. The ask has to survive the page being away, and land
//! over the place the page puts back on its way in rather than under it.
//!
//! The page is loaded from a copy of the QML tree with `EnterKey` taken
//! out; see `common::qml_tree_without_enter_key` for why the shipped file
//! cannot be loaded headlessly as it stands.

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
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Created the way pageStack.push does, still on its way in.
        function load(url, accountId, chatId, findMessageId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                findMessageId: findMessageId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function settle() { loader.item.status = PageStatus.Active; return 'ok' }
        // A page pushed over this one, and this one asked for a message
        // from it while away: what a media page's Show in chat does.
        function leave() { loader.item.status = PageStatus.Deactivating; return 'ok' }
        function ask(messageId) { loader.item.showMessage(messageId); return 'ok' }
        function forgetFound() { everFound = '0'; return 'ok' }
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
        // Latched rather than read at a fixed moment: the flash clears
        // itself after a couple of seconds, so a single late read cannot
        // tell 'never marked' from 'marked and already faded'.
        property string everFound: '0'
        Timer {
            interval: 100; running: true; repeat: true
            onTriggered: {
                var list = findIn(loader.item, 'messageList')
                if (list && list.foundMessageId !== 0) {
                    everFound = '' + list.foundMessageId
                }
                if (anyFound(list)) { everLit = 'yes' }
            }
        }
        function everFoundValue() { return everFound }

        // Whether any row on screen is actually marked. The list holding
        // an id says nothing about whether the delegate was ever told:
        // the binding between the two was missing, and a test that only
        // read the list passed the whole way through that.
        function anyFound(node) {
            if (!node) { return false }
            if (node.isFound === true) { return true }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                if (anyFound(kids[i])) { return true }
            }
            if (node.contentItem && node.contentItem !== node) {
                return anyFound(node.contentItem)
            }
            return false
        }
        property string everLit: 'no'
        function everLitValue() { return everLit }

        /// Which row the model puts a message in, straight from the model.
        function rowOf(messageId) {
            var model = findIn(loader.item, 'messages')
            return model ? '' + model.row_of(messageId) : 'missing:messages'
        }
    }
";

/// Chat 1 in the fake holds messages 1 and 2; message 1 is the older one,
/// so a jump to it is a jump away from the newest.
const OLDER_MESSAGE: i32 = 1;
/// The other one, asked for from a page over the chat.
const NEWER_MESSAGE: i32 = 2;

#[test]
#[allow(clippy::too_many_lines)]
fn a_message_a_search_found_is_where_the_chat_opens() {
    let temp = std::env::temp_dir().join(format!("piiri-find-message-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

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
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1,
                OLDER_MESSAGE
            )
        );
        record!("settle", call!("settle"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("row", call!("rowOf", OLDER_MESSAGE));
        record!("found", call!("everFoundValue"));
        record!("lit", call!("everLitValue"));
        record!(
            "flash-cleared",
            call!(
                "get",
                QString::from("messageList"),
                QString::from("foundMessageId")
            )
        );
        record!(
            "following",
            call!(
                "get",
                QString::from("messageList"),
                QString::from("stickToBottom")
            )
        );
        // Away under another page, asked for the other message from
        // there, and back: the ask is kept and acted on over the place
        // the page puts back.
        record!("forget", call!("forgetFound"));
        record!("leave", call!("leave"));
        record!("ask", call!("ask", NEWER_MESSAGE));
        record!("asked-while-away", call!("everFoundValue"));
        record!("return", call!("settle"));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("found-on-return", call!("everFoundValue"));
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
        "the conversation page did not load. {context}"
    );
    assert_eq!(
        value("row"),
        "0",
        "the model cannot say which row a message is in, so nothing can \
         jump to one. {context}"
    );
    assert_eq!(
        value("found"),
        OLDER_MESSAGE.to_string(),
        "the chat did not mark the message the search found, so the reader \
         lands in a wall of text with no idea which row matched. {context}"
    );
    // The row itself has to change, not just a number on the list.
    assert_eq!(
        value("lit"),
        "yes",
        "no message on screen was ever marked, so the reader arrives in a \
         wall of text with nothing picked out. {context}"
    );
    // And the flash gets out of the way again: a message left lit for the
    // rest of the session reads as a state, not as an answer.
    assert_eq!(
        value("flash-cleared"),
        "0",
        "the found message stayed lit. {context}"
    );
    // The jump is only real if the view stopped following the newest
    // message: otherwise the next arrival drags it straight back down.
    assert_eq!(
        value("following"),
        "false",
        "the view is still stuck to the newest message, so the jump would \
         be undone by the next thing to arrive. {context}"
    );
    // Asked for from a page over the chat: nothing happens while the page
    // is away, and the message is lit once it is back on screen.
    for (label, expected, complaint) in [
        ("leave", "ok", "the page could not be left"),
        ("ask", "ok", "the page has no showMessage to ask"),
        (
            "asked-while-away",
            "0",
            "a message asked for while the page was away was gone to at \
             once, under the place the page puts back on its way in",
        ),
        (
            "found-on-return",
            &NEWER_MESSAGE.to_string(),
            "a message asked for from a page over the chat was not shown \
             once the chat was back on screen",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
}
