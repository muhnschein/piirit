//! Deleting a message for everybody in the chat, from the row's menu to
//! the core.
//!
//! The whole of the new half, driven through the real conversation page
//! against the fake core: a long press on a message of this account's
//! own, Delete, the page that asks which kind of delete this is, the
//! answer, the wait -- and then the one call that means "and on the
//! other ends too", `delete_messages_for_all`.
//!
//! The two ways differ in what nobody can take back, so the plumbing
//! between them is worth pinning end to end: an answer that arrived as
//! the other kind would quietly delete a message everywhere the reader
//! meant to delete it here, or leave it everywhere they meant it gone.
//! Which messages the page offers the second way for is
//! `qml_delete_choice`; what the wait does with either is
//! `qml_delete_run`.

// Qt harness: see qml_conversation_open.rs.
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

/// The message sent here and then deleted again. The fake core numbers
/// what it is sent from 101, above the ids it seeds a chat with.
const SENT_ID: u32 = 101;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }

        /// The last page pushed, as `Page.qml:body=...,everyone=...`.
        property string pushed: ''

        // The dialog Delete leads to, as the stack hands it back:
        // Silica answers a push with the page it made, and this one
        // reports which kind of delete was accepted on it.
        QtObject {
            id: chooser
            signal picked(bool forEveryone)
            function pick(forEveryone) { picked(forEveryone) }
        }
        QtObject {
            id: stack
            function push(url, props) {
                var name = ('' + url).split('/').pop()
                pushed = name + ':everyone=' + props.canDeleteForEveryone
                return chooser
            }
            function pop() { }
        }
        function stackObject() { return stack }
        function pushedPage() { return pushed }
        function pick(forEveryone) { chooser.pick(forEveryone); return 'ok' }

        function open(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Active
            })
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
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            if (node.contentItem && node.contentItem !== node) {
                findAll(node.contentItem, name, out)
            }
            return out
        }
        function find(name) { return findIn(loader.item, name) }

        /// A message of this account's own, which is what the core will
        /// delete for everyone.
        function send(text) {
            var model = find('messages')
            if (!model) { return 'missing:messages' }
            model.send(text)
            return 'ok'
        }
        // The rows, mirrored, so the test can say which messages are
        // left rather than only how many.
        Repeater { id: mirror; Item { property int mid: model.message_id } }
        function watch() {
            var model = find('messages')
            mirror.model = model.rows
            return 'ok'
        }
        function ids() {
            var out = ''
            for (var i = 0; i < mirror.count; i++) { out += mirror.itemAt(i).mid + ';' }
            return out
        }
        function count() {
            var model = find('messages')
            return model ? '' + model.count : 'missing:messages'
        }
        /// The wait before a message goes, turned down from four seconds.
        function hurry(ms) {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            list.pendingDelay = ms
            return 'ok'
        }
        /// The row showing this message. A row does not say which message
        /// it holds, but the menu's reaction strip does.
        function rowFor(messageId) {
            var found = findAll(find('messageList'), 'messageRow', [])
            for (var i = 0; i < found.length; i++) {
                var menu = found[i].menu
                var picker = menu ? findIn(menu, 'reactionPicker') : null
                if (picker && picker.messageId === messageId) {
                    return found[i]
                }
            }
            return null
        }
        /// Delete, from that message's own menu.
        function deleteRow(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var item = findIn(row.menu, 'deleteItem')
            if (!item) { return 'missing:deleteItem' }
            item.clicked()
            return 'ok'
        }
        /// Whether the platform's countdown is up over that message.
        function goingOut(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var remorse = findIn(row, 'messageRemorse')
            return '' + (remorse ? remorse.active : false)
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_message_the_reader_deletes_for_everyone_reaches_the_core_as_that() {
    let temp = std::env::temp_dir().join(format!("piirit-delete-all-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // A message sent from here comes back as this account's own,
        // which is what the core takes a deletion for everyone of.
        std::env::set_var("PIIRIT_FAKE_SELF_SENT", "1");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));
    // Named for the page before it is loaded, as Silica names its own.
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let page = common::page_url_in(&tree, "ConversationPage.qml");

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

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("open", call!("open", QString::from(page.clone()), 1, 1));
        record!("hurry", call!("hurry", 300));
        record!("watch", call!("watch"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("send", call!("send", QString::from("on my way")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("count-before", call!("count"));
        record!("delete", call!("deleteRow", SENT_ID));
        // The tap asks; nothing is waiting to go until the page answers.
        record!("pushed", call!("pushedPage"));
        record!("countdown-before-pick", call!("goingOut", SENT_ID));
        record!("pick", call!("pick", true));
        record!("countdown-after-pick", call!("goingOut", SENT_ID));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("count-after", call!("count"));
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!("count-later", call!("count"));
        record!("ids-later", call!("ids"));
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
    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, calls: {names:?}");

    for label in ["open", "hurry", "watch", "send", "delete", "pick"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("count-before"),
        "3",
        "the message to delete is not in the chat. {context}"
    );
    assert_eq!(
        value("pushed"),
        "DeleteMessageDialog.qml:everyone=true",
        "Delete did not ask which kind of delete it is, or did not say \
         that the core would take this one for everyone. {context}"
    );
    assert_eq!(
        value("countdown-before-pick"),
        "false",
        "the wait before the message goes started on the tap, before the \
         reader had said which kind of delete they meant. {context}"
    );
    assert_eq!(
        value("countdown-after-pick"),
        "true",
        "nothing is waiting to go after the reader picked, so the answer \
         never reached the list. {context}"
    );
    assert_eq!(
        value("count-after"),
        "2",
        "the message is still in the chat after the core deleted it. \
         {context}"
    );
    assert_eq!(
        value("ids-later"),
        "1;2;",
        "the deleted message is still one of the chat's rows. The id list \
         the core's event is answered with cannot say that the newest \
         message has gone -- a row past every id in it reads as one the \
         list is too old to know about -- so the row goes when the core \
         agrees to the deletion instead. {context}"
    );

    assert!(
        calls.contains(&(
            "delete_messages_for_all".to_string(),
            serde_json::json!([1, [SENT_ID]])
        )),
        "deleting for everyone did not reach the core as the call that \
         means it, so the other ends keep a message the reader asked \
         everybody to drop. {context}"
    );
    assert!(
        !calls.iter().any(|(name, _)| name == "delete_messages"),
        "the message went from this account alone, which is not what the \
         reader picked. {context}"
    );
}
