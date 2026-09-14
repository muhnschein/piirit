//! Unsent text is kept.
//!
//! What is typed survives going back to the list and opening the chat
//! again. The core keeps drafts itself, so the draft goes there rather
//! than being held in the page: it then survives the app being closed,
//! and the chat list says which chats are holding one
//! without anything here building that text -- a chat with a draft comes
//! back with `summaryText1` "Draft", which the row already shows in front
//! of the preview. `deltachat-jsonrpc/tests/real_server.rs` pins both
//! halves against the real core.
//!
//! What is checked here is the round trip through a page: type, leave,
//! come back, and find it still there -- and that leaving is enough on its
//! own, without waiting for the debounce that would otherwise write it.

// Qt harness: see qml_conversation_open.rs.
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

const DRAFT: &str = "half a thought";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Postivene 1.0
    Item {
        Loader { id: loader }
        function open(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Active
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        /// Back to the chat list, which is this page going away.
        function leave() {
            loader.item.status = PageStatus.Deactivating
            return 'ok'
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
        function type(text) {
            var field = findIn(loader.item, 'messageField')
            if (!field) { return 'missing:messageField' }
            field.text = text
            return 'ok'
        }
        function typed() {
            var field = findIn(loader.item, 'messageField')
            return field ? field.text : 'missing:messageField'
        }

        // The other half: a chat holding a draft says so in the list. The
        // row itself is qml_chat_row's -- it already pins that a preview
        // with a sender reads 'Ada: see you there' -- so what is left to
        // check here is that the sender a draft arrives with is 'Draft'.
        ChatList { id: chats; account_id: 1 }
        Repeater {
            id: rows
            model: chats.rows
            Item {
                property string rowName: model.name
                property string rowPreview: model.preview
                property string rowSender: model.preview_sender
            }
        }
        function refreshList() { chats.reload(); return 'ok' }
        function summaryOfChatOne() {
            for (var i = 0; i < rows.count; i++) {
                var row = rows.itemAt(i)
                if (row.rowName === 'chat 1') {
                    return row.rowSender + ': ' + row.rowPreview
                }
            }
            return 'no-row'
        }
    }
";

#[test]
fn an_unsent_message_is_kept_and_the_chat_list_says_so() {
    let temp = std::env::temp_dir().join(format!("postivene-drafts-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

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
    let page = common::page_url_in(&tree, "ConversationPage.qml");
    let reopen = page.clone();

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
        (*steps_ptr).push(("open", call!("open", QString::from(page.clone()), 1, 1)));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("typed", call!("type", QString::from(DRAFT))));
        // Straight out again, well inside the debounce: leaving has to be
        // enough on its own.
        (*steps_ptr).push(("left", call!("leave")));
    });
    // A fresh page onto the same chat, the way reopening it from the list
    // builds one -- and soon enough that the debounce on the page just
    // destroyed cannot have been what wrote the draft.
    single_shot(Duration::from_millis(2400), move || unsafe {
        (*steps_ptr).push(("reopen", call!("open", QString::from(reopen.clone()), 1, 1)));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("back", call!("typed")));
        (*steps_ptr).push(("listed", call!("refreshList")));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("row", call!("summaryOfChatOne")));
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

    assert_eq!(value("open"), "ok", "the page did not load. {context}");
    assert_eq!(value("typed"), "ok", "nothing was typed. {context}");
    assert_eq!(
        value("reopen"),
        "ok",
        "the page did not load a second time. {context}"
    );
    assert_eq!(
        value("back"),
        DRAFT,
        "what was typed and not sent was lost on the way to the chat list \
         and back. {context}"
    );
    // The prefix is the core's own word, not one built here: the row
    // already puts `summaryText1` in front of the preview, which is why
    // this needed no new field and gets whatever language the core is in.
    assert_eq!(
        value("row"),
        format!("Draft: {DRAFT}"),
        "the chat list does not say which chats are holding something \
         unsent. {context}"
    );
}
