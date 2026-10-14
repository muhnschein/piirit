//! Scroll to top of long chat reaches first message.
//!
//! Shape, not view, makes it reliable: a moving-window model must be replaced to get
//! anywhere, and positioning into a just-replaced model depends on row-measure speed.
//! Model holds a row per message; first is row 0 once id list arrives. Reaching it
//! is scrolling: nothing to fetch or replace. Filling rows around reader moves nothing.

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

/// Several pages: top is far from where chat opens.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        Repeater {
            id: mirror
            Item { property int mid: model.message_id }
        }
        function load(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function settle() {
            loader.item.status = PageStatus.Active
            mirror.model = find('messages').rows
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
        function find(name) { return findIn(loader.item, name) }
        function rowCount() { return '' + mirror.count }
        /// The message at the top of the view: what the reader is looking
        /// at, and what the whole report was about.
        function topId() {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            for (var y = 1; y < list.height; y += 8) {
                var index = list.indexAt(list.width / 2, list.contentY + y)
                if (index >= 0 && index < mirror.count) {
                    return '' + mirror.itemAt(index).mid
                }
            }
            return 'no-row'
        }
        /// To the top, which is all reaching the beginning takes now.
        ///
        /// Exactly what the system's own scroll-to-top does, and nothing
        /// else: no telling the list to stop following first. A chat opens
        /// at its newest message and is following it, so a helper that
        /// turned that off would be testing a jump nobody makes.
        function toTop() {
            var list = find('messageList')
            list.positionViewAtBeginning()
            list.forceLayout()
            return 'ok'
        }
        /// Where the topmost day heading and the topmost row actually sit.
        ///
        /// Both are children of the list's content item, so their `y` is in
        /// one coordinate space and can be compared. A heading whose bottom
        /// is past the top of the row beneath it is drawn over that row.
        function geometry() {
            var list = find('messageList')
            var kids = list.contentItem.children
            var row = null
            for (var i = 0; i < kids.length; i++) {
                var kid = kids[i]
                if (kid.objectName === 'messageRow'
                        && (row === null || kid.y < row.y)) {
                    row = kid
                }
            }
            if (!row) { return 'no-row' }
            var head = findIn(row, 'dayLabel')
            if (!head) { return 'no-heading' }
            if (!head.visible) { return 'heading-hidden' }
            // In the row's own coordinates, so containment is the question.
            var top = head.mapToItem(row, 0, 0).y
            var bottom = head.mapToItem(row, 0, head.height).y
            if (top < 0) { return 'above-row:' + Math.round(top) }
            if (bottom > row.height) {
                return 'past-row:' + Math.round(bottom - row.height)
            }
            return 'inside'
        }
        /// Whether the list is still trying to keep the reader at the
        /// newest message after they asked for the oldest.
        function stillFollowing() {
            return find('messageList').stickToBottom ? 'yes' : 'no'
        }
        /// The rows around the reader being filled in, which on a device is
        /// the answer to `hydrateRequested` arriving.
        function fillTop() {
            find('messages').hydrate(0, 20)
            return 'ok'
        }
        function relayout() {
            find('messageList').forceLayout()
            return 'ok'
        }
    }
";

#[test]
fn the_top_of_the_view_is_the_first_message_in_the_chat() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-paging-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_LONG_CHAT", MESSAGES.to_string());
        // Wrapping messages: filling a row changes its height, which can move reader.
        std::env::set_var("PIIRIT_FAKE_WORDY", "1");
        // First row a core notice, as in real chats: date over encryption notice.
        std::env::set_var("PIIRIT_FAKE_INFO_FIRST", "1");
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1
            ),
        ));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("settle", call!("settle")));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("opened-rows", call!("rowCount")));
        // Straight to top: no control, fetch or window move.
        (*steps_ptr).push(("scrolled", call!("toTop")));
        (*steps_ptr).push(("following", call!("stillFollowing")));
        (*steps_ptr).push(("top", call!("topId")));
        (*steps_ptr).push(("fill", call!("fillTop")));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        // Visible rows gain text and grow; must not take reader with them.
        (*steps_ptr).push(("relayout", call!("relayout")));
        (*steps_ptr).push(("geometry", call!("geometry")));
        (*steps_ptr).push(("top-after", call!("topId")));
        (*steps_ptr).push(("rows-after", call!("rowCount")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Run's outcome, out of test body.
fn assert_outcome(steps: &[(&str, String)]) {
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
        value("opened-rows"),
        MESSAGES.to_string(),
        "a {MESSAGES}-message chat did not open with a row per message, so \
         where its beginning is depends on what has been fetched. {context}"
    );

    assert_eq!(
        value("following"),
        "no",
        "the list was still following the newest message after the reader \
         was taken to the oldest, so the next row to be measured hauls them \
         back down again. {context}"
    );

    // Day heading part of row, not separate section item: date drawn over encryption notice
    // (reported against section delegate) impossible; row cannot draw over itself.
    assert_eq!(
        value("geometry"),
        "inside",
        "the day heading is not inside the row it heads ({:?}), so where it \
         is drawn depends on the view's own section bookkeeping. {context}",
        value("geometry")
    );

    // Whole report, one assertion.
    assert_eq!(
        value("top"),
        "1",
        "scrolling to the top of the chat did not reach its first message: \
         the reader is looking at {:?}, somewhere in the middle of a chat \
         they asked to see the start of. {context}",
        value("top")
    );
    assert_eq!(
        value("top-after"),
        "1",
        "the rows around the reader gained their text, grew, and carried \
         the reader off the first message to {:?}. {context}",
        value("top-after")
    );
    assert_eq!(
        value("rows-after"),
        MESSAGES.to_string(),
        "filling rows in changed how many there are; it has to happen in \
         place, or the view loses its position every time. {context}"
    );
}
