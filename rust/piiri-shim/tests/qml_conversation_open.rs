//! Opening a chat puts the messages on the page before it arrives.
//!
//! Two failure modes bound this. Starting the fetch as the page is created
//! builds rows on the Qt thread while the page is still transitioning in,
//! which stutters the transition; deferring the handover to
//! `PageStatus.Active` trades that stutter for a wait, with the page
//! arriving empty and filling in behind itself.
//!
//! Neither applies now. A chat opens on one page of fifty rather than on
//! all of it, and `ChatPrefetch` has usually built
//! those rows before the push. So the handover goes back to the page's own
//! construction, where a prefetch hit makes it a move rather than a fetch,
//! and the page comes in already full.
//!
//! What is pinned below is that last part, in the only terms that mean
//! anything: the rows are there in the same turn the page is built, before
//! anything could have run a transition, let alone finished one.
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
    import Piiri 1.0
    Item {
        Loader { id: loader }
        // What the chat list does before it pushes the page.
        property int readyFor: 0
        ChatPrefetch {
            id: prefetch
            account_id: 1
            onReady: readyFor = chat_id
        }
        // 0 for the message to open at: this is an ordinary tap on a
        // chat, not a search result.
        function prefetchChat(chatId) {
            prefetch.start(chatId, 0)
            return 'ok'
        }
        function prefetched() { return '' + readyFor }

        // Created the way pageStack.push does, still on its way in.
        function loadInactive(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function chatOfModel() {
            var model = findIn(loader.item, 'messages')
            return model ? '' + model.chat_id : 'missing:messages'
        }
        function rowsOfModel() {
            var model = findIn(loader.item, 'messages')
            return model ? '' + model.count : 'missing:messages'
        }
        // Whether the chat counts as read yet. Handing the chat over before
        // `reading_history` has been bound would mark it read on open,
        // behind a page the reader has not seen.
        function readingHistory() {
            var model = findIn(loader.item, 'messages')
            return model ? '' + model.reading_history : 'missing:messages'
        }
    }
";

/// What the fake server seeds chat 1 with.
const SEEDED: &str = "2";

#[test]
fn a_chat_is_on_its_page_before_the_page_arrives() {
    let temp = std::env::temp_dir().join(format!("piiri-open-chat-{}", std::process::id()));
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

    // The tap on the chat list: load the chat, then push the page.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push(("prefetch", call!("prefetchChat", 1)));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("prefetched", call!("prefetched")));
        (*steps_ptr).push((
            "load",
            call!(
                "loadInactive",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1
            ),
        ));
        // Read in the same callback as the load, on purpose: nothing has
        // had a turn in between, so whatever is here was put here by the
        // page building itself.
        (*steps_ptr).push(("chat-while-arriving", call!("chatOfModel")));
        (*steps_ptr).push(("rows-while-arriving", call!("rowsOfModel")));
        (*steps_ptr).push(("reading-history", call!("readingHistory")));
        (*steps_ptr).push(("settle", call!("settle")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("rows-after-arriving", call!("rowsOfModel")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the run has to show for itself, out of the test body.
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
        "the conversation page did not load. {context}"
    );
    assert_eq!(
        value("prefetched"),
        "1",
        "the prefetch never answered, so this run proves nothing about what \
         a page does with one. {context}"
    );
    assert_eq!(
        value("chat-while-arriving"),
        "1",
        "the page was built without being told which chat it is showing, so \
         it can only start loading one once it has arrived. {context}"
    );
    // The point of all of it.
    assert_eq!(
        value("rows-while-arriving"),
        SEEDED,
        "the page was built empty: the messages the prefetch had already \
         loaded were not in it, so the reader watches an empty conversation \
         fill in after the transition instead of one that arrives whole. \
         {context}"
    );
    assert_eq!(
        value("reading-history"),
        "true",
        "the chat was handed to the model before `reading_history` was \
         bound, so opening a chat marks it read behind a page that is still \
         transitioning in. {context}"
    );
    assert_eq!(
        value("rows-after-arriving"),
        SEEDED,
        "the messages did not survive the page arriving. {context}"
    );
}
