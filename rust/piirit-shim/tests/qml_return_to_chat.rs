//! Coming back to a conversation from a page opened over it.
//!
//! Reported from a device: opening a picture full screen and going back
//! landed at the oldest loaded message rather than where the reader had
//! been. A page with another pushed over it is torn down far enough that
//! its list forgets where it was, and what a list forgets it replaces with
//! the beginning.
//!
//! Putting the place back when the page returns was not enough, and the
//! second report said exactly why: the reader saw the top of the chat and
//! was *then* yanked back. The list is reset while the page is away, and a
//! frame showing the wrong rows is painted before anything gets round to
//! correcting it. So the row is held from the moment the page goes rather
//! than restored when it comes back, and the reset is undone in the same
//! turn it happens.
//!
//! What this covers is that the reader comes back to their place at all:
//! without remembering and restoring it they land at message 6 instead of
//! 20. What it does not cover is the difference between holding the row
//! from the moment the page leaves and putting it back when the page
//! returns, because that difference is one frame -- and offscreen Qt has no
//! render loop to paint it. That half was confirmed on a device, which is
//! also where it was reported.

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

/// Long enough to have somewhere to be other than the ends.
const MESSAGES: u32 = 117;

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
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            mirror.model = find('messages').rows
            return 'ok'
        }
        function setStatus(value) { loader.item.status = value; return 'ok' }
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
        function get(property) {
            var list = find('messageList')
            return list ? '' + list[property] : 'missing'
        }
        function idAt(offset) {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            var index = list.indexAt(list.width / 2, list.contentY + offset)
            if (index < 0 || index >= mirror.count) { return 'no-row' }
            return '' + mirror.itemAt(index).mid
        }
        /// The message nearest the middle of the view.
        ///
        /// Scanned rather than probed once, and over the whole height:
        /// `indexAt` lands on nothing between rows, and a row that has not
        /// been filled in yet is a short placeholder, so a fixed handful of
        /// probes can fall through a screen of them and find nothing.
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
        /// Somewhere in the middle of the loaded messages, the way a reader
        /// who scrolled back and then tapped a picture is.
        function scrollTo(row) {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            list.jumpToRow(row)
            list.forceLayout()
            return middleId()
        }
        /// The relayout a page gets as it comes back: offscreen Qt has no
        /// render loop, so nothing polishes the view unless it is asked.
        function relayout() {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            list.forceLayout()
            return 'ok'
        }
        /// What the device does to the view while another page is over it.
        function forget() {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            list.positionViewAtBeginning()
            list.forceLayout()
            return middleId()
        }
    }
";

#[test]
fn a_page_opened_over_a_conversation_gives_the_reader_their_place_back() {
    let temp = std::env::temp_dir().join(format!("piirit-return-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_LONG_CHAT", MESSAGES.to_string());
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
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("active", call!("setStatus", 2)));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        // Back up the history, which is where a picture worth opening is.
        (*steps_ptr).push(("scrolled", call!("scrollTo", 20)));
    });
    single_shot(Duration::from_millis(6000), move || unsafe {
        (*steps_ptr).push(("before", call!("middleId")));
        // A page goes over this one: Deactivating, then the view loses its
        // place, then Active again on the way back.
        (*steps_ptr).push(("leaving", call!("setStatus", 3)));
        (*steps_ptr).push((
            "remembered-row",
            call!("get", QString::from("rememberedRow")),
        ));
        (*steps_ptr).push(("forgotten", call!("forget")));
        (*steps_ptr).push(("returning", call!("setStatus", 2)));
        (*steps_ptr).push(("straight-back", call!("middleId")));
    });
    single_shot(Duration::from_millis(6300), move || unsafe {
        // The rows are laid out again as the page settles, which is what
        // moves a reader off wherever they were just put -- and what the
        // hold answers, in the same turn.
        (*steps_ptr).push(("relayout", call!("relayout")));
        (*steps_ptr).push(("after", call!("middleId")));
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

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_ne!(
        value("before"),
        "no-row",
        "the test never established where the reader was, so it proved \
         nothing. {context}"
    );
    // The whole point. Not "it comes back right in the end" -- the reader
    // must never be shown anywhere else, because every moment they are is a
    // frame. The check is on the first look after the page returns, with no
    // pass in between for anything to correct it: with the place merely
    // remembered rather than held, this reports the top of the chat.
    //
    // The instant *during* the interruption is not asserted on. It is the
    // right moment in principle and the wrong one to measure: the view is
    // put back within the same turn it is knocked, so a probe there finds
    // a view mid-move rather than either state.
    assert_eq!(
        value("straight-back"),
        value("before"),
        "the page came back somewhere other than where the reader left \
         it: they were looking at message {:?} and returned to {:?}. \
         {context}",
        value("before"),
        value("straight-back")
    );
    assert_eq!(
        value("after"),
        value("before"),
        "the rows settled after the page came back and carried the reader \
         off with them: they were looking at message {:?} and ended up at \
         {:?}. {context}",
        value("before"),
        value("after")
    );
}
