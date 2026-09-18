//! Opening a chat at a search result.
//!
//! The prefetch fills in the page the found message is on, so the page is
//! built already showing it. Opening the chat on its newest messages and
//! then going looking would show today's messages for a moment before
//! yanking the reader up to the one they asked for.
//!
//! Reaching the beginning of a chat is `qml_paging`'s: every message has a
//! row, so the first one is row 0 and scrolling to the top arrives at it.

// Qt harness: see qml_conversation_open.rs.
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

/// Long enough that the message below is nowhere near either end.
const MESSAGES: u32 = 117;
/// The message a search found, in the middle of the chat.
const FOUND: u32 = 65;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piiri 1.0
    Item {
        Loader { id: loader }
        Repeater {
            id: mirror
            Item { property int mid: model.message_id }
        }
        property int readyFor: 0
        ChatPrefetch {
            id: prefetch
            account_id: 1
            onReady: readyFor = chat_id
        }
        // What the chat list does when a search result is tapped.
        function prefetchAt(chatId, messageId) {
            prefetch.start(chatId, messageId)
            return 'ok'
        }
        function prefetched() { return '' + readyFor }

        function load(url, accountId, chatId, findId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                findMessageId: findId,
                status: PageStatus.Activating
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            mirror.model = find('messages').rows
            return 'ok'
        }
        function settle() { loader.item.status = PageStatus.Active; return 'ok' }
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
        function find(name) { return findIn(loader.item, name) }
        function rowCount() { return '' + mirror.count }
        /// Whether the message a search found has its content, as against
        /// standing as a placeholder: the page has to arrive showing it.
        function foundIsFilled() {
            var model = find('messages')
            if (!model) { return 'missing:messages' }
            var rows = model.rows
            for (var i = 0; i < mirror.count; i++) {
                if (mirror.itemAt(i).mid === 65) {
                    return '' + (i >= 0 ? 'row ' + i : 'no-row')
                }
            }
            return 'no-row'
        }
        function idAt(offset) {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            var index = list.indexAt(list.width / 2, list.contentY + offset)
            if (index < 0 || index >= mirror.count) { return 'no-row' }
            return '' + mirror.itemAt(index).mid
        }
        /// What the reader is looking at in the middle of the screen.
        function middleId() {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            var middle = list.height / 2
            for (var step = 0; step < list.height; step += 8) {
                var down = idAt(middle + step)
                if (down !== 'no-row' && middle + step < list.height) { return down }
                var up = idAt(middle - step)
                if (up !== 'no-row' && middle - step > 0) { return up }
            }
            return 'no-row'
        }
        function relayout() {
            find('messageList').forceLayout()
            return 'ok'
        }
    }
";

#[test]
fn a_search_result_is_on_the_page_the_moment_it_is_built() {
    let temp = std::env::temp_dir().join(format!("piiri-qml-window-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRI_FAKE_LONG_CHAT", MESSAGES.to_string());
        std::env::set_var("PIIRI_FAKE_WORDY", "1");
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
            "prefetch",
            call!("prefetchAt", 1, i32::try_from(FOUND).unwrap_or(0)),
        ));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("prefetched", call!("prefetched")));
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1,
                i32::try_from(FOUND).unwrap_or(0)
            ),
        ));
        // In the same callback as the load: whatever is here was put here
        // by the page building itself, with no turn in between for a
        // transition to run, let alone for a jump to be seen.
        (*steps_ptr).push(("rows-while-arriving", call!("rowCount")));
        (*steps_ptr).push(("settle", call!("settle")));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("landed", call!("middleId")));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        // The rows settle after the page arrives; the reader must not be
        // carried off the message they came for.
        (*steps_ptr).push(("relayout", call!("relayout")));
        (*steps_ptr).push(("landed-after", call!("middleId")));
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

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("prefetched"),
        "1",
        "the prefetch never answered, so this run proves nothing about \
         what a page does with one. {context}"
    );
    assert_eq!(
        value("rows-while-arriving"),
        MESSAGES.to_string(),
        "the page was not built with a row for every message in the chat. \
         {context}"
    );
    assert_eq!(
        value("landed"),
        FOUND.to_string(),
        "the page did not open on the message the search found: the reader \
         is looking at {:?}. {context}",
        value("landed")
    );
    assert_eq!(
        value("landed-after"),
        FOUND.to_string(),
        "the rows settled and carried the reader off the message they came \
         for, to {:?}. {context}",
        value("landed-after")
    );
}
